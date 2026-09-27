//! Repository chores.
//!
//! `check-hygiene` is the important one: it turns the project's two structural
//! safety rules into something CI enforces, rather than something reviewers have
//! to remember.

mod fetch;
mod icons;
mod package;

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("check-hygiene") => check_hygiene(),
        Some("fetch-toolchain") => fetch::run(&args[1..]),
        Some("fetch-sources") => fetch::sources(&args[1..]),
        Some("package") => package::run(&args[1..]),
        Some("gen-icons") => icons::run(&args[1..]),
        Some("version") => {
            println!("{}", package::workspace_version(&repo_root())?);
            Ok(())
        }
        Some(other) => bail!("unknown task `{other}`\n\n{USAGE}"),
        None => bail!("{USAGE}"),
    }
}

const USAGE: &str = "\
USAGE:
    cargo xtask check-hygiene
    cargo xtask fetch-toolchain [--target <name>] [--out <dir>] [--offline]
    cargo xtask fetch-sources   [--target <name>] [--out <dir>] [--list]
    cargo xtask package         [--target <name>] [--profile <p>] [--no-archive]
    cargo xtask version
    cargo xtask gen-icons
";

struct Violation {
    file: PathBuf,
    line: usize,
    text: String,
    rule: &'static str,
}

/// The committed icons must be what `logo.png` produces today.
///
/// They are generated and committed, which is only honest if something checks
/// them: replace the artwork, forget `cargo xtask gen-icons`, and the executable,
/// the window, the installer and the uninstaller all go quietly stale. Nothing
/// else would notice — CI never runs the generator, the release consumes the
/// committed `.ico` directly, and `build.rs` watches the `.ico` rather than the
/// logo it came from.
///
/// This is the same job `check_manifest_is_a_placeholder` does for the other
/// generated artefact. Regeneration is byte-stable, so a mismatch means the
/// source moved and the outputs did not.
fn check_icons_match_logo(root: &Path, v: &mut Vec<Violation>) {
    let Ok(want) = crate::icons::generate(root) else {
        // No logo, or an unreadable one. `gen-icons` reports that properly; this
        // check has nothing to say about it.
        return;
    };
    for (path, want) in [
        (crate::icons::ico_path(root), want.ico),
        (crate::icons::window_png_path(root), want.window_png),
    ] {
        let rel = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
        match std::fs::read(&path) {
            Ok(have) if have == want => {}
            Ok(_) => v.push(Violation {
                file: rel,
                line: 0,
                text: String::new(),
                rule: "generated from logo.png and out of date; run `cargo xtask gen-icons`",
            }),
            Err(_) => v.push(Violation {
                file: rel,
                line: 0,
                text: String::new(),
                rule: "generated from logo.png and missing; run `cargo xtask gen-icons`",
            }),
        }
    }
}

/// Every translation key named in the code must exist in both catalogs.
///
/// `i18n::lookup` falls back to returning the key itself, so a mistyped or
/// never-added key does not fail, does not warn, and renders in the interface as
/// `libs.heading` — in front of a user who reads Vietnamese. That silent failure
/// is the whole reason this check exists.
///
/// Only literal keys can be checked. A key chosen at run time (a build error's
/// `reason_key`, say) is invisible here, which is why unused catalog entries are
/// reported as a note rather than a failure.
fn check_i18n_keys(root: &Path, v: &mut Vec<Violation>) -> Result<()> {
    let mut catalogs: Vec<(String, std::collections::BTreeSet<String>)> = Vec::new();
    for lang in ["vi", "en"] {
        let path = root.join(format!("crates/etb-core/assets/i18n/{lang}.toml"));
        let text = std::fs::read_to_string(&path)?;
        let parsed: toml::Value = text.parse()?;
        let keys = parsed
            .as_table()
            .map(|t| t.keys().cloned().collect())
            .unwrap_or_default();
        catalogs.push((lang.to_string(), keys));
    }

    let mut referenced: std::collections::BTreeSet<String> = Default::default();
    for file in rust_sources(root) {
        let rel = file.strip_prefix(root).unwrap_or(&file).to_path_buf();
        let text = blank_comments(&std::fs::read_to_string(&file)?);
        // Scanned over the whole file, not line by line: a `tr!(` call is often
        // wrapped across lines by rustfmt, and its key then sits on a line of its
        // own. Checking single lines silently skipped exactly those calls.
        for (offset, key) in literal_keys_in(&text) {
            referenced.insert(key.clone());
            let line = text[..offset].matches('\n').count() + 1;
            for (lang, keys) in &catalogs {
                if !keys.contains(&key) {
                    v.push(Violation {
                        file: rel.clone(),
                        line,
                        text: format!("key `{key}`"),
                        rule: match lang.as_str() {
                            "vi" => "translation key is missing from vi.toml",
                            _ => "translation key is missing from en.toml",
                        },
                    });
                }
            }
        }
    }

    let unused: Vec<&String> = catalogs[0]
        .1
        .iter()
        .filter(|k| !referenced.contains(*k))
        .collect();
    if !unused.is_empty() {
        eprintln!(
            "note: {} catalog key(s) never named literally in the code \
             (fine if chosen at run time, dead weight otherwise):",
            unused.len()
        );
        for k in unused {
            eprintln!("    {k}");
        }
    }
    Ok(())
}

/// Does a `#[cfg(...)]` attribute gate on code being *built as a test*?
///
/// Two things it must not say yes to, both of which would hand an exemption to
/// ordinary code and silently switch these rules off for it:
///
///   * `#[cfg(feature = "test-utils")]` — a feature whose name reads like one.
///     String literals are removed before anything else is looked at.
///   * `#[cfg(not(test))]` — which gates on *not* being a test, so it marks
///     production code. Every `not(...)` group is removed, nesting included.
fn cfg_gates_on_test(line: &str) -> bool {
    let mut chars: Vec<char> = Vec::new();
    let mut in_string = false;
    for c in line.chars() {
        if c == '"' {
            in_string = !in_string;
            continue;
        }
        if !in_string {
            chars.push(c);
        }
    }

    let mut kept = String::new();
    let mut i = 0;
    while i < chars.len() {
        let starts_not = chars[i..].starts_with(&['n', 'o', 't', '('])
            && (i == 0 || !(chars[i - 1].is_alphanumeric() || chars[i - 1] == '_'));
        if starts_not {
            let mut depth = 0usize;
            let mut j = i + 3;
            while j < chars.len() {
                if chars[j] == '(' {
                    depth += 1;
                } else if chars[j] == ')' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                j += 1;
            }
            i = j + 1;
            continue;
        }
        kept.push(chars[i]);
        i += 1;
    }

    kept.split(|c: char| !c.is_alphanumeric() && c != '_')
        .any(|token| token == "test")
}

/// Blank out `//` comments, keeping every byte offset and newline in place.
///
/// Without this the doc comment on the `tr!` macro — which spells out
/// `tr!(lang, "key")` — is read as a call site naming a key called `key`.
fn blank_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        match line.find("//") {
            Some(at) => {
                out.push_str(&line[..at]);
                for c in line[at..].chars() {
                    out.push(if c == '\n' { '\n' } else { ' ' });
                }
            }
            None => out.push_str(line),
        }
    }
    out
}

/// Pull the literal key out of `tr!(lang, "k")`, `i18n::lookup(lang, "k")` and
/// `i18n::format(lang, "k", ..)`, anywhere in a file. Returns each key with the
/// byte offset it was found at, so a line number can be recovered.
///
/// A non-literal second argument — a run-time `reason_key`, say — yields nothing,
/// because there is no key to check at this point.
fn literal_keys_in(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for opener in ["tr!(", "i18n::lookup(", "i18n::format("] {
        let mut from = 0;
        while let Some(at) = text[from..].find(opener) {
            let start = from + at + opener.len();
            from = start;
            let Some(comma) = text[start..].find(',') else {
                continue;
            };
            let after = start + comma + 1;
            let rest = text[after..].trim_start();
            let Some(stripped) = rest.strip_prefix('"') else {
                continue;
            };
            if let Some(end) = stripped.find('"') {
                out.push((after, stripped[..end].to_string()));
            }
        }
    }
    out
}

fn check_hygiene() -> Result<()> {
    let root = repo_root();
    let mut v: Vec<Violation> = Vec::new();

    check_no_binaries_tracked(&root, &mut v)?;
    check_no_turbo_basic_in_the_tree(&root, &mut v);
    check_basic_provenance(&root, &mut v)?;
    check_manifest_is_a_placeholder(&root, &mut v);
    check_installer_keeps_its_bom(&root, &mut v);
    check_i18n_keys(&root, &mut v)?;
    check_icons_match_logo(&root, &mut v);

    for file in rust_sources(&root) {
        let rel = file.strip_prefix(&root).unwrap_or(&file).to_path_buf();
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let text = std::fs::read_to_string(&file)?;

        let is_fs_guard = rel_str.ends_with("etb-core/src/fs_guard.rs");
        let is_test = rel_str.contains("/tests/")
            || rel_str.contains("etb-testkit/")
            || rel_str.contains("xtask/");

        let mut in_test_mod = false;
        let mut pending_test_attr = false;
        let mut brace_depth_at_test = 0usize;
        let mut depth = 0usize;

        for (i, raw) in text.lines().enumerate() {
            let line = raw.trim();
            // Track a test module so unit tests may use anything. `#[cfg(test)]`
            // is the usual spelling, but `#[cfg(all(test, unix))]` is just as
            // ordinary, and matching the literal string let one slip past --
            // silently withdrawing the exemption from a module that plainly is
            // tests, and reporting it as a violation.
            //
            // Only a `mod` opens the exempt region. A `#[cfg(test)]` on a `use`
            // or a single function does not: taking the attribute alone as the
            // trigger left the exemption standing until the next closing brace,
            // which is to say over whatever ordinary code happened to follow.
            if line.starts_with("#[cfg(") && cfg_gates_on_test(line) {
                pending_test_attr = true;
            } else if pending_test_attr && !line.is_empty() && !line.starts_with('#') {
                if line.starts_with("mod ") || line.starts_with("pub mod ") {
                    in_test_mod = true;
                    brace_depth_at_test = depth;
                }
                pending_test_attr = false;
            }
            depth += raw.matches('{').count();
            depth = depth.saturating_sub(raw.matches('}').count());
            if in_test_mod && depth <= brace_depth_at_test && line.contains('}') {
                in_test_mod = false;
            }
            if line.starts_with("//") || line.starts_with("///") || line.starts_with("//!") {
                continue;
            }
            let exempt = is_test || in_test_mod;

            // Rule 1: only fs_guard may write, create or delete.
            if !is_fs_guard && !exempt {
                for pat in [
                    "File::create(",
                    "fs::write(",
                    "fs::remove_file(",
                    "fs::remove_dir_all(",
                    "fs::rename(",
                    "fs::create_dir_all(",
                    "OpenOptions::new(",
                    "File::options(",
                    "fs::copy(",
                    "fs::hard_link(",
                    "set_modified(",
                ] {
                    if line.contains(pat) {
                        v.push(Violation {
                            file: rel.clone(),
                            line: i + 1,
                            text: line.to_string(),
                            rule: "only fs_guard.rs may create, write, rename or delete files",
                        });
                    }
                }
            }

            // Rule 2: never spawn through a shell. No exemption but tests --
            // the module that used to need one wrote a launcher script, and it
            // is gone.
            if !exempt {
                for pat in ["cmd /c", "cmd.exe", "\"sh\"", "sh -c", "/bin/sh"] {
                    if line.contains(pat) {
                        v.push(Violation {
                            file: rel.clone(),
                            line: i + 1,
                            text: line.to_string(),
                            rule: "never spawn a child through a shell; use Command::arg per argument",
                        });
                    }
                }
            }

            // Rule 3: the GUI holds no logic.
            if rel_str.contains("etb-gui/src/") && !exempt {
                for pat in ["std::process::Command", "std::fs::"] {
                    if line.contains(pat) {
                        v.push(Violation {
                            file: rel.clone(),
                            line: i + 1,
                            text: line.to_string(),
                            rule:
                                "etb-gui must not touch the filesystem or spawn processes directly",
                        });
                    }
                }
            }

            // Rule 4: flags that would break containment or usability.
            //
            // Matched as string literals, quote included, so the comments that
            // explain why they are never passed do not trip over themselves.
            for (pat, why) in [
                (
                    "\"-i\"",
                    "-i adds an include search path, so a program's $INCLUDE could \
                     reach outside the staged copy that build/stage.rs assembled",
                ),
                (
                    "\"-p\"",
                    "-p adds a library search path, so a program could link something \
                     from outside the bundle the manifest attests",
                ),
                (
                    "\"-s\"",
                    "-s gui builds a program with no console, so everything it PRINTs \
                     goes nowhere; the subsystem belongs in build/fbargs.rs, with a \
                     test, if it is ever needed",
                ),
            ] {
                if line.contains(pat) && !exempt && !rel_str.ends_with("xtask/src/main.rs") {
                    v.push(Violation {
                        file: rel.clone(),
                        line: i + 1,
                        text: line.to_string(),
                        rule: why,
                    });
                }
            }
        }
    }

    if v.is_empty() {
        println!("hygiene: ok");
        return Ok(());
    }
    for x in &v {
        // A tracked-file violation has no line to point at.
        if x.line == 0 {
            println!("{}: {}", x.file.display(), x.rule);
            if !x.text.is_empty() {
                println!("    {}", x.text);
            }
        } else {
            println!(
                "{}:{}: {}\n    {}",
                x.file.display(),
                x.line,
                x.rule,
                x.text
            );
        }
    }
    bail!("{} hygiene violation(s)", v.len())
}

/// The committed toolchain manifest must stay a placeholder.
///
/// A generated one describes exactly one fetched bundle. Committed, it travels
/// to everyone else, who then has a checkout that reports every file of their
/// bundle as missing and refuses to build. The fetch task overwrites this file
/// as a matter of course, so it is easy to commit by accident -- easy enough
/// that it happened once.
/// The NSIS script must keep its UTF-8 byte order mark.
///
/// `Unicode true` inside the script makes the *installer* Unicode and says
/// nothing about how makensis reads the script itself. Without the mark it reads
/// it in the build machine's ANSI codepage, so the Vietnamese survives a build on
/// a UTF-8 machine and arrives doubly encoded from a build on Windows. That is
/// not hypothetical: it is how `Gỡ cài đặt` reached a Start Menu as
/// `Gá»¡ cÃ i Ä‘áº·t`, in a release, in the first screen a Vietnamese-speaking
/// user sees.
///
/// A byte order mark is the kind of thing an editor removes without mentioning
/// it, and the damage only appears on a machine nobody is looking at.
fn check_installer_keeps_its_bom(root: &Path, v: &mut Vec<Violation>) {
    let rel = "packaging/windows/installer.nsi";
    let path = root.join(rel);
    let Ok(bytes) = std::fs::read(&path) else {
        return;
    };
    if !bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        v.push(Violation {
            file: PathBuf::from(rel),
            line: 1,
            text: "(no UTF-8 byte order mark)".into(),
            rule: "installer.nsi must begin with a UTF-8 BOM, or makensis reads it \
                   in the build machine's codepage and mangles every Vietnamese string",
        });
    }
}

fn check_manifest_is_a_placeholder(root: &Path, v: &mut Vec<Violation>) {
    let rel = PathBuf::from("crates/etb-gui/assets/toolchain-manifest.txt");

    // Read what git has staged, not what is on disk. A generated manifest in the
    // working tree is the normal state in the middle of a release build -- the
    // fetch writes it and the build embeds it -- so checking the file would fail
    // during ordinary work, and a check that cries wolf gets switched off.
    // What must never happen is *committing* one.
    let staged = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["show", ":crates/etb-gui/assets/toolchain-manifest.txt"])
        .output();
    let text = match staged {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
        // Not staged, or not a checkout: fall back to the file itself.
        _ => match std::fs::read_to_string(root.join(&rel)) {
            Ok(t) => t,
            Err(_) => return,
        },
    };
    let hashes = text
        .lines()
        .filter(|l| {
            let l = l.trim();
            l.len() > 64 && l[..64].bytes().all(|b| b.is_ascii_hexdigit())
        })
        .count();
    if hashes > 0 {
        v.push(Violation {
            text: format!(
                "{hashes} hashes staged; run `git checkout {}` before committing",
                rel.display()
            ),
            file: rel,
            line: 0,
            rule: "the committed toolchain manifest must be a placeholder, not a generated one",
        });
    }
}

/// The compiler must never enter git history.
///
/// `.gitignore` is a convenience that a `git add -f` or a stray `--out` can walk
/// straight past. This asks git what is actually tracked, so the guarantee holds
/// however the file got there. It also keeps the repository small enough to
/// clone quickly, which is the other half of why the toolchain is fetched rather
/// than committed.
/// The first line every BASIC file in the repository carries.
pub const PROVENANCE: &str = "' Written for Easy Turbo Basic";

/// Every tracked BASIC file says it was written for this project.
///
/// The programs this application exists to build belong to the people who
/// wrote them, and are tested locally, from where they already live (see
/// `tests/local_corpus.rs`). The failure this prevents is one of them being
/// copied into the corpus or the examples to make a test easier: it would then
/// be published with every push. A line at the top is a small thing to ask of
/// a file we wrote, and a check that needs no list of names to protect.
fn check_basic_provenance(root: &Path, v: &mut Vec<Violation>) -> Result<()> {
    const BASIC_EXT: &[&str] = &["bas", "inc", "bi", "bm", "tb"];

    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z"])
        .output();
    let Ok(out) = out else {
        return Ok(());
    };
    if !out.status.success() {
        return Ok(());
    }
    for name in String::from_utf8_lossy(&out.stdout).split('\0') {
        let rel = PathBuf::from(name);
        let ext = rel
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if name.is_empty() || !BASIC_EXT.contains(&ext.as_str()) {
            continue;
        }
        let bytes = std::fs::read(root.join(&rel)).unwrap_or_default();
        if !bytes.starts_with(PROVENANCE.as_bytes()) {
            v.push(Violation {
                file: rel,
                line: 1,
                text: String::new(),
                rule: "every BASIC file in the repository starts with \
                       `' Written for Easy Turbo Basic`; a program someone else wrote \
                       is tested locally, never committed",
            });
        }
    }
    Ok(())
}

/// Turbo Basic itself, and anything written in it that is not ours, must not
/// be in this repository at all — not committed, and not sitting in the
/// working tree either.
///
/// It is Borland's, and a licensed copy belongs to whoever licensed it. The
/// handbook is theirs too. And a user's own program is theirs: it goes in a
/// case directory outside the repository, named by `ETB_LOCAL_CORPUS`.
///
/// Checked over the whole tree and not just what git tracks, because the way
/// this goes wrong is a copy made "just to test something" under `target/`,
/// where `.gitignore` hides it from every other check and from the person who
/// made it. `tools/tb-oracle/README.md` says how to use a copy that stays
/// where it belongs.
fn check_no_turbo_basic_in_the_tree(root: &Path, v: &mut Vec<Violation>) {
    const FORBIDDEN: &[&str] = &["tb.exe", "tbconfig.tb", "tb-handbook.txt"];
    const FORBIDDEN_EXT: &[&str] = &["tbc"];

    fn walk(dir: &Path, root: &Path, v: &mut Vec<Violation>, depth: usize) {
        if depth > 12 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let path = e.path();
            let name = e.file_name().to_string_lossy().to_ascii_lowercase();
            if path.is_dir() {
                // What the build generates is skipped by name: it is tens of
                // gigabytes of somebody else's files, and a walk through it
                // would make this check too slow to run. Anything under
                // `target/` that is NOT on this list is walked, because an
                // ad-hoc copy made "just to test something" is exactly what
                // lands in a directory nobody thought to name.
                const GENERATED: &[&str] = &[
                    ".git",
                    "debug",
                    "release",
                    "dist",
                    "toolchain",
                    "toolchain-cache",
                    "toolchain-staging",
                    "corresponding-source",
                    "wineprefix",
                    "wineprefix-vi",
                    "package",
                ];
                let in_target = path.parent().is_some_and(|p| p.ends_with("target"));
                if name == ".git" || (in_target && GENERATED.contains(&name.as_str())) {
                    continue;
                }
                walk(&path, root, v, depth + 1);
                continue;
            }
            let ext = path
                .extension()
                .map(|x| x.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            if FORBIDDEN.contains(&name.as_str()) || FORBIDDEN_EXT.contains(&ext.as_str()) {
                v.push(Violation {
                    file: path.strip_prefix(root).unwrap_or(&path).to_path_buf(),
                    line: 0,
                    text: name.clone(),
                    rule: "Turbo Basic is not ours to keep here: use a copy that stays \
                           outside the repository (tools/tb-oracle/README.md)",
                });
            }
        }
    }
    walk(root, root, v, 0);
}

fn check_no_binaries_tracked(root: &Path, v: &mut Vec<Violation>) -> Result<()> {
    const MAX_TRACKED_BYTES: u64 = 1_000_000;
    const BINARY_EXT: &[&str] = &[
        "conda", "exe", "dll", "so", "a", "o", "obj", "lib", "dylib", "ttf", "otf", "rpm", "7z",
        "tbc", "com",
    ];

    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z"])
        .output();
    let Ok(out) = out else {
        // Not a checkout, or no git. Nothing to check rather than a failure.
        return Ok(());
    };
    if !out.status.success() {
        return Ok(());
    }

    for name in String::from_utf8_lossy(&out.stdout).split('\0') {
        if name.is_empty() {
            continue;
        }
        let rel = PathBuf::from(name);
        let full = root.join(&rel);
        let ext = rel
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();

        if BINARY_EXT.contains(&ext.as_str()) {
            v.push(Violation {
                file: rel.clone(),
                line: 0,
                text: String::new(),
                rule: "binaries are never committed; the toolchain is fetched from its recipe",
            });
            continue;
        }
        if let Ok(md) = std::fs::metadata(&full) {
            if md.is_file() && md.len() > MAX_TRACKED_BYTES {
                v.push(Violation {
                    file: rel,
                    line: 0,
                    text: format!("{:.1} MB", md.len() as f64 / 1e6),
                    rule: "tracked files stay under 1 MB; large artefacts belong in a release",
                });
            }
        }
    }
    Ok(())
}

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn rust_sources(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.join("crates")];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if p.file_name().is_some_and(|n| n == "target") {
                    continue;
                }
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

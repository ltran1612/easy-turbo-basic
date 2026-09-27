//! Corpus cases: a program, and what building and running it must do.
//!
//! Shared by the committed corpus (`tests/corpus.rs`) and the local-only one
//! (`tests/local_corpus.rs`), whose cases live outside the repository because
//! the programs in them are not ours to publish.
//!
//! A case is a directory holding `expect.toml`. Its `sources` are relative to
//! that directory, and may reach outside it — a local case sits beside the
//! program it describes rather than holding a copy.

use crate::hash_tree;
use etb_core::build::{self, BuildRequest};
use etb_core::fs_guard::FsGuard;
use etb_core::paths::{AppPaths, WorkLayout};
use etb_core::project::{Program, SourceRef};
use etb_core::toolchain::Toolchain;
use etb_core::translate::TestMode;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Expect {
    pub description: String,
    /// The program first, then any included files. Relative to the case.
    pub sources: Vec<String>,
    pub success: bool,
    /// Run the built program. Always in console test mode, so it reads
    /// `stdin` and its screen output can be checked.
    pub run: bool,
    pub stdin: String,
    pub stdout_contains: Vec<String>,
    /// Text the program must not print: what a skipped branch would have said.
    pub stdout_lacks: Vec<String>,
    /// `[name, text]`: after the run, the file `name` in the program's folder
    /// contains `text`.
    pub files_contain: Vec<(String, String)>,
    pub error_contains: Vec<String>,
    /// The error names this file — the user's, never our staged copy.
    pub error_in_file: Option<String>,
    pub error_line: Option<u32>,
    /// The reason key when the build refuses a file outright.
    pub file_problem: Option<String>,
    /// Keys the translator must report.
    pub finding_keys: Vec<String>,
    /// Run in its own window on a virtual screen, as the user would see it,
    /// rather than in the console; then look at the screenshot it leaves.
    /// Needs `xvfb-run` and a QB64-PE that runs on this machine.
    pub window: bool,
    /// The screenshot's size in pixels.
    pub screen_size: Option<(u32, u32)>,
    /// `[r, g, b, n]`: at least `n` pixels of that colour on the screen.
    pub screen_colors: Vec<(u8, u8, u8, u32)>,
}

impl Default for Expect {
    fn default() -> Self {
        Self {
            description: String::new(),
            sources: Vec::new(),
            success: true,
            run: false,
            stdin: String::new(),
            stdout_contains: Vec::new(),
            stdout_lacks: Vec::new(),
            files_contain: Vec::new(),
            error_contains: Vec::new(),
            error_in_file: None,
            error_line: None,
            file_problem: None,
            finding_keys: Vec::new(),
            window: false,
            screen_size: None,
            screen_colors: Vec::new(),
        }
    }
}

pub struct Case {
    pub name: String,
    pub expect: Expect,
    pub dir: PathBuf,
}

/// Every case directory under `root`, in name order.
pub fn cases(root: &Path) -> Vec<Case> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(root) else {
        return out;
    };
    for e in rd.flatten() {
        let dir = e.path();
        let toml_path = dir.join("expect.toml");
        if !toml_path.is_file() {
            continue;
        }
        let text = std::fs::read_to_string(&toml_path)
            .unwrap_or_else(|e| panic!("{}: {e}", toml_path.display()));
        let expect: Expect =
            toml::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", toml_path.display()));
        out.push(Case {
            name: dir.file_name().unwrap().to_string_lossy().to_string(),
            expect,
            dir,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// The compiler to run the corpus against, or `None` to skip.
///
/// `ETB_REQUIRE_TOOLCHAIN=1` turns a missing compiler into a failure, so CI
/// can never skip the corpus without noticing.
pub fn toolchain() -> Option<Toolchain> {
    match etb_core::toolchain::discover(None) {
        Ok(tc) => Some(tc),
        Err(e) => {
            if std::env::var("ETB_REQUIRE_TOOLCHAIN").as_deref() == Ok("1") {
                panic!("ETB_REQUIRE_TOOLCHAIN=1 but no compiler was found: {e}");
            }
            eprintln!("SKIPPING the corpus: {e}");
            eprintln!("  point ETB_TOOLCHAIN_BUNDLE at a QB64-PE bundle to run it");
            None
        }
    }
}

/// Build and, if asked, run one case, checking everything it promises.
///
/// The sources are copied into `scratch` first and the build is made from the
/// copies, so a regression cannot damage the originals — and the originals
/// are hashed before and after all the same.
pub fn run_case(
    tc: &Toolchain,
    case: &Case,
    scratch: &Path,
    paths: &AppPaths,
    guard: &FsGuard,
    seq: u64,
) -> Result<(), String> {
    let originals: Vec<PathBuf> = case
        .expect
        .sources
        .iter()
        .map(|s| case.dir.join(s))
        .collect();
    let before: Vec<_> = originals.iter().map(|p| hash_file(p)).collect();

    let copies = scratch.join(&case.name);
    std::fs::create_dir_all(&copies).map_err(|e| e.to_string())?;
    let mut program = Program::new(&case.name);
    for src in &originals {
        let name = src.file_name().ok_or("a source with no file name")?;
        let copy = copies.join(name);
        std::fs::copy(src, &copy).map_err(|e| format!("{}: {e}", src.display()))?;
        program.sources.push(SourceRef::new(copy));
    }
    let copies_before = hash_tree(&copies);

    if case.expect.window && !can_run_windows(tc) {
        // Driven through a launcher (wine), a program's window cannot be put
        // on a virtual screen here: that is how the arrangement is, not a
        // fault. A native QB64-PE with no xvfb-run is a machine missing a
        // package, and with a compiler required, that is worth failing on.
        if tc.launcher().is_none() && std::env::var("ETB_REQUIRE_TOOLCHAIN").as_deref() == Ok("1") {
            return Err("a window case needs xvfb-run".into());
        }
        eprintln!(
            "  SKIPPING {}: needs xvfb-run and a native QB64-PE",
            case.name
        );
        return Ok(());
    }
    let layout = WorkLayout::new(paths.build_dir(seq));
    let req = BuildRequest {
        test_mode: Some(if case.expect.window {
            TestMode::Window
        } else {
            TestMode::Console
        }),
        // The real thing does this, so the corpus does too: it is how a case
        // at a path QB64-PE cannot be given is made to build at all.
        prepare: Some(build::Preparation {
            tool_root: paths.tool_root(),
            lock_dir: paths.lock_dir(),
            manifest: std::sync::Arc::new(Default::default()),
        }),
    };
    let outcome = build::build_with(
        guard,
        tc,
        &layout,
        &program,
        &req,
        None,
        &AtomicBool::new(false),
    );

    let verdict = check(tc, &case.expect, &outcome);

    // The promise, checked around every single case.
    if hash_tree(&copies) != copies_before {
        return Err("THE SOURCE FILES WERE MODIFIED (the copies)".into());
    }
    let after: Vec<_> = originals.iter().map(|p| hash_file(p)).collect();
    if before != after {
        return Err("THE ORIGINAL SOURCE FILES WERE MODIFIED".into());
    }
    verdict
}

fn hash_file(p: &Path) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(std::fs::read(p).unwrap_or_default()))
}

fn check(tc: &Toolchain, e: &Expect, outcome: &build::BuildOutcome) -> Result<(), String> {
    for key in &e.finding_keys {
        if !outcome.findings.iter().any(|f| f.key == key) {
            return Err(format!(
                "expected the translator to report {key}; it reported {:?}",
                outcome.findings.iter().map(|f| f.key).collect::<Vec<_>>()
            ));
        }
    }
    if let Some(key) = &e.file_problem {
        return match &outcome.file_problem {
            Some(fp) if fp.reason_key == key => Ok(()),
            other => Err(format!(
                "expected the file to be refused with {key}, got {other:?}"
            )),
        };
    }

    if e.success != outcome.success {
        return Err(format!(
            "expected build success={}, got {} ({} errors, internal: {:?})\n--- compiler output ---\n{}",
            e.success,
            outcome.success,
            outcome.errors,
            outcome.internal_error,
            outcome.raw.trim()
        ));
    }

    if !e.success {
        let all: String = outcome
            .diagnostics
            .iter()
            .map(|d| d.message.clone())
            .collect::<Vec<_>>()
            .join("\n");
        for needle in &e.error_contains {
            if !all.to_lowercase().contains(&needle.to_lowercase()) {
                return Err(format!(
                    "expected an error mentioning {needle:?}, got:\n{all}"
                ));
            }
        }
        if let Some(f) = &e.error_in_file {
            if !outcome
                .diagnostics
                .iter()
                .any(|d| d.file.as_deref() == Some(f.as_str()))
            {
                return Err(format!(
                    "expected an error in {f:?} (the user's name, not the staged one); got {:?}",
                    outcome
                        .diagnostics
                        .iter()
                        .map(|d| &d.file)
                        .collect::<Vec<_>>()
                ));
            }
        }
        if let Some(l) = e.error_line {
            if !outcome.diagnostics.iter().any(|d| d.line == Some(l)) {
                return Err(format!(
                    "expected an error on line {l}; got {:?}",
                    outcome
                        .diagnostics
                        .iter()
                        .map(|d| d.line)
                        .collect::<Vec<_>>()
                ));
            }
        }
        return Ok(());
    }

    if !e.run {
        return Ok(());
    }

    // The application builds programs; it does not run them. The corpus runs
    // what was built anyway, because that is the only way to prove the
    // translation produces a program that behaves as it did under Turbo Basic.
    let exe = outcome
        .exe
        .clone()
        .ok_or("build succeeded but produced no executable")?;
    // A QB64 program changes to its own folder as it starts (see `libqb.cpp`),
    // so the files it writes by name land beside it, wherever it was started
    // from. That is also what the user sees after double-clicking it.
    let run_dir = exe
        .parent()
        .ok_or("the program has no folder")?
        .to_path_buf();
    let (code, text) = if e.window {
        run_in_window(&exe, &run_dir)?
    } else {
        run_built(tc, &exe, &run_dir, &e.stdin)?
    };

    if code != Some(0) {
        return Err(format!("the program exited with {code:?}; output:\n{text}"));
    }
    for needle in &e.stdout_contains {
        if !text.contains(needle.as_str()) {
            return Err(format!(
                "expected output to contain {needle:?}, got:\n{text}"
            ));
        }
    }
    if e.window {
        check_screen(e, &run_dir.join(etb_core::translate::TEST_SCREENSHOT))?;
    }
    for needle in &e.stdout_lacks {
        if text.contains(needle.as_str()) {
            return Err(format!(
                "expected output NOT to contain {needle:?}, got:\n{text}"
            ));
        }
    }
    for (name, needle) in &e.files_contain {
        let got = std::fs::read(run_dir.join(name))
            .map_err(|err| format!("expected the program to write {name}: {err}"))?;
        let got = String::from_utf8_lossy(&got);
        if !got.contains(needle.as_str()) {
            return Err(format!(
                "expected {name} to contain {needle:?}, got:\n{got}"
            ));
        }
    }
    Ok(())
}

/// Window cases run the program itself on a virtual X screen, so they need
/// `xvfb-run`, and a QB64-PE whose programs run here without a launcher.
pub fn can_run_windows(tc: &Toolchain) -> bool {
    tc.launcher().is_none() && which::which("xvfb-run").is_ok()
}

/// Run a window-mode program on a virtual screen. It saves a screenshot and
/// exits by itself; what it prints goes to its window, so there is no text.
fn run_in_window(exe: &Path, cwd: &Path) -> Result<(Option<i32>, String), String> {
    use std::process::Stdio;
    let mut child = crate::spawn_tolerating_busy(
        std::process::Command::new("xvfb-run")
            .args(["-a", "-s", "-screen 0 1024x768x24"])
            .arg(exe)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null()),
    )
    .map_err(|err| format!("could not start xvfb-run: {err}"))?;
    let deadline = Instant::now() + RUN_LIMIT;
    loop {
        match child.try_wait() {
            Ok(Some(s)) => return Ok((s.code(), String::new())),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{} was still running after {}s",
                    exe.display(),
                    RUN_LIMIT.as_secs()
                ));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// What the screenshot must show.
fn check_screen(e: &Expect, png: &Path) -> Result<(), String> {
    let img = image::open(png)
        .map_err(|err| format!("no screenshot at {}: {err}", png.display()))?
        .to_rgb8();
    if let Some((w, h)) = e.screen_size {
        if img.dimensions() != (w, h) {
            return Err(format!(
                "screen is {:?}, expected {w} x {h}",
                img.dimensions()
            ));
        }
    }
    for &(r, g, b, min) in &e.screen_colors {
        let n = img.pixels().filter(|p| p.0 == [r, g, b]).count() as u32;
        if n < min {
            return Err(format!(
                "expected at least {min} pixels of rgb({r}, {g}, {b}) on screen, found {n}"
            ));
        }
    }
    Ok(())
}

/// How long a test program may run. They print a few lines and finish; one
/// still running after this is waiting for input it will never get.
const RUN_LIMIT: Duration = Duration::from_secs(60);

/// Run a built program with a fixed input and collect what it printed, with
/// line endings and terminal control sequences removed.
pub fn run_built(
    tc: &Toolchain,
    exe: &Path,
    cwd: &Path,
    stdin: &str,
) -> Result<(Option<i32>, String), String> {
    use std::io::{Read, Write};
    use std::process::Stdio;

    // Through the bundle's launcher when there is one, so a Windows bundle can
    // be exercised from Linux under wine.
    let mut child = crate::spawn_tolerating_busy(
        tc.run_binary(exe)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null()),
    )
    .map_err(|err| format!("could not start {}: {err}", exe.display()))?;

    if let Some(mut w) = child.stdin.take() {
        let _ = w.write_all(stdin.as_bytes());
        // Dropping the handle closes stdin, so a program reading until end of
        // file terminates instead of waiting forever.
    }
    let mut stdout = child.stdout.take().ok_or("no stdout")?;
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });

    let deadline = Instant::now() + RUN_LIMIT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{} was still running after {}s",
                    exe.display(),
                    RUN_LIMIT.as_secs()
                ));
            }
            Err(e) => return Err(e.to_string()),
        }
    };
    let out = reader.join().unwrap_or_default();
    Ok((status.code(), clean(&String::from_utf8_lossy(&out))))
}

/// Remove carriage returns and ANSI escape sequences, which a console program
/// emits for COLOR, CLS and LOCATE and which say nothing about the text.
pub fn clean(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {}
            '\u{1b}' => {
                if chars.peek() == Some(&'[') {
                    chars.next();
                    for d in chars.by_ref() {
                        if d.is_ascii_alphabetic() {
                            break;
                        }
                    }
                }
            }
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_codes_and_carriage_returns_are_removed() {
        assert_eq!(clean("\u{1b}[2J\u{1b}[1;1Ha\r\nb\u{1b}[0m"), "a\nb");
    }
}

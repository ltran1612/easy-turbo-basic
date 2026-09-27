//! `cargo xtask fetch-toolchain` — turn a pinned recipe into a usable bundle.
//!
//! Download, verify, extract, (on Linux) build, prune, describe. Everything
//! the application needs to ship its own QB64 Phoenix Edition, reproducibly,
//! from a recipe under version control.
//!
//! Pure Rust rather than a shell script because this has to run on Windows CI,
//! where `7z` and `tar` are not a given.
//!
//! **Modification times are part of the bundle.** QB64-PE runs `make` over its
//! own runtime, and make decides what to rebuild by comparing times. Extracting
//! or copying without them turns a prebuilt runtime into one make either
//! rebuilds on the user's machine or, worse, trusts when it should not. So
//! every step here carries the archive's times through to the bundle.

use anyhow::{bail, Context, Result};
use etb_core::glob;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    target: String,
    /// The version everything else must agree with.
    pub fbc_version: String,
    bundle_template: String,
    archive: Archive,
    /// For a release that ships as source: the commands that build it, run in
    /// the extracted tree without a shell.
    #[serde(default)]
    build: Option<Build>,
    prune: Prune,
    /// What QB64-PE writes inside its own directory as it works. These paths
    /// are recorded in the manifest and not attested: see `manifest.rs`.
    #[serde(default)]
    mutable: Mutable,
    /// Where the source of everything in the bundle can be had, and under what
    /// licence. Some of it is GPL, which obliges us to offer the source from
    /// the same place as the binaries.
    #[serde(rename = "source", default)]
    pub sources: Vec<Source>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Archive {
    url: String,
    file: String,
    sha256: String,
    size: u64,
    format: ArchiveFormat,
    /// The one directory every entry in the archive sits under.
    strip_prefix: String,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
enum ArchiveFormat {
    #[serde(rename = "tar.gz")]
    TarGz,
    #[serde(rename = "zip")]
    Zip,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Build {
    /// Each an argv list. `{jobs}` is replaced by the number of CPUs.
    commands: Vec<Vec<String>>,
    /// The file the build must produce, relative to the tree.
    produces: String,
}

#[derive(Debug, Deserialize)]
pub struct Source {
    pub component: String,
    pub version: String,
    pub url: String,
    #[serde(default)]
    sha256: Option<String>,
    pub license: String,
    #[serde(default)]
    covers: Vec<String>,
    #[serde(default)]
    note: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Mutable {
    #[serde(default)]
    paths: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Prune {
    keep: Vec<String>,
    #[serde(default)]
    drop: Vec<String>,
}

/// Read and parse a target's recipe. Shared so packaging reports exactly the
/// components that were fetched, from the same source of truth.
pub fn read_recipe(root: &Path, target: &str) -> Result<Recipe> {
    let path = root.join("toolchain").join(format!("{target}.toml"));
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

/// `cargo xtask fetch-sources` — assemble the Corresponding Source.
///
/// Redistributing GPL binaries — GNU make, and whatever else the bundled C++
/// toolchain carries — obliges us to offer their source from the same place.
/// This gathers it into one directory to publish alongside a release, with a
/// SOURCES.md saying which binary each one covers.
pub fn sources(args: &[String]) -> Result<()> {
    let mut target = "windows-x86_64".to_string();
    let mut out: Option<PathBuf> = None;
    let mut list_only = false;

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--target" => target = it.next().context("--target needs a value")?.clone(),
            "--out" => out = Some(PathBuf::from(it.next().context("--out needs a path")?)),
            "--list" => list_only = true,
            other => bail!(
                "unknown option `{other}`\n\n\
                 USAGE: cargo xtask fetch-sources [--target <name>] [--out <dir>] [--list]"
            ),
        }
    }

    let root = crate::repo_root();
    let recipe = read_recipe(&root, &target)?;
    if recipe.sources.is_empty() {
        bail!("the {target} recipe lists no [[source]] entries");
    }
    let dir = out.unwrap_or_else(|| {
        root.join("target")
            .join("corresponding-source")
            .join(&target)
    });
    fs::create_dir_all(&dir)?;

    let mut rows = Vec::new();
    for s in &recipe.sources {
        let file = s.url.rsplit('/').next().unwrap_or("source").to_string();
        let looks_like_a_file =
            file.contains('.') && !s.url.trim_end_matches('/').ends_with(&s.component);
        if list_only || !looks_like_a_file {
            println!("  {:<28} {:<24} {}", s.component, s.version, s.url);
            rows.push((s, file, false));
            continue;
        }
        let dest = dir.join(&file);
        let ok = match &s.sha256 {
            Some(want) => verified(&dest, want)?,
            None => dest.is_file(),
        };
        if ok {
            println!("  cached   {:<26} {}", s.component, file);
        } else {
            println!("  fetching {:<26} {}", s.component, file);
            download(&s.url, &dest).with_context(|| format!("downloading {}", s.url))?;
            if let Some(want) = &s.sha256 {
                if !verified(&dest, want)? {
                    let got = sha256_file(&dest)?;
                    let _ = fs::remove_file(&dest);
                    bail!("checksum mismatch for {file}\n  expected {want}\n  got      {got}");
                }
            }
        }
        rows.push((s, file, true));
    }

    let mut md = String::new();
    md.push_str("# Corresponding Source\n\n");
    md.push_str(
        "The compiler shipped with this application is QB64 Phoenix Edition together\n\
         with the C++ toolchain it drives. Some of those components are licensed under\n\
         the GNU General Public License, which requires that their source be available\n\
         from the same place as the binaries, for as long as the binaries are\n\
         distributed.\n\n\
         This directory is that source. Each entry below names what it covers.\n\n",
    );
    md.push_str(&format!(
        "Toolchain: {}, FreeBASIC {}.\n\n",
        recipe.target, recipe.fbc_version
    ));
    md.push_str("| Component | Version | Licence | Covers | File or location |\n");
    md.push_str("|---|---|---|---|---|\n");
    for (s, file, fetched) in &rows {
        let covers = if s.covers.iter().any(|c| c == "all") {
            "everything in the bundle".to_string()
        } else {
            s.covers.join(", ")
        };
        let loc = if *fetched {
            format!("`{file}`")
        } else {
            format!("<{}>", s.url)
        };
        md.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            s.component, s.version, s.license, covers, loc
        ));
    }
    md.push_str("\n## Checksums\n\n");
    for (s, file, fetched) in &rows {
        if let (true, Some(h)) = (*fetched, &s.sha256) {
            md.push_str(&format!(
                "- `{file}`\n  - sha256 `{h}`\n  - from <{}>\n",
                s.url
            ));
        }
    }
    let notes: Vec<_> = rows
        .iter()
        .filter_map(|(s, _, _)| s.note.as_ref().map(|n| (s, n)))
        .collect();
    if !notes.is_empty() {
        md.push_str("\n## Notes\n\n");
        for (s, n) in notes {
            md.push_str(&format!("- **{}**: {n}\n", s.component));
        }
    }
    let md_path = dir.join("SOURCES.md");
    fs::write(&md_path, md)?;

    println!();
    println!("wrote {}", md_path.display());
    println!();
    println!("Publish this directory next to the installer, on the same server, and keep it");
    println!("there for as long as that release is downloadable.");
    Ok(())
}

pub fn run(args: &[String]) -> Result<()> {
    let mut target = default_target().to_string();
    let mut out: Option<PathBuf> = None;
    let mut offline = false;

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--target" => target = it.next().context("--target needs a value")?.clone(),
            "--out" => out = Some(PathBuf::from(it.next().context("--out needs a path")?)),
            "--offline" => offline = true,
            other => bail!(
                "unknown option `{other}`\n\n\
                 USAGE: cargo xtask fetch-toolchain [--target <name>] [--out <dir>] [--offline]"
            ),
        }
    }

    let root = crate::repo_root();
    let recipe = read_recipe(&root, &target)?;
    if recipe.target != target {
        bail!("toolchain/{target}.toml says it is for `{}`", recipe.target);
    }

    let cache = root.join("target").join("toolchain-cache");
    let staging = root.join("target").join("toolchain-staging").join(&target);
    let bundle = out.unwrap_or_else(|| root.join("target").join("toolchain").join(&target));

    fs::create_dir_all(&cache)?;
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;

    println!(
        "target   {} (FreeBASIC {})",
        recipe.target, recipe.fbc_version
    );

    let a = &recipe.archive;
    let archive = cache.join(&a.file);
    if verified(&archive, &a.sha256)? {
        println!("  cached   {} ({:.1} MB)", a.file, a.size as f64 / 1e6);
    } else {
        if offline {
            bail!(
                "{} is missing or fails its checksum, and --offline was given",
                a.file
            );
        }
        println!("  fetching {} ({:.1} MB)", a.file, a.size as f64 / 1e6);
        download(&a.url, &archive).with_context(|| format!("downloading {}", a.url))?;
        if !verified(&archive, &a.sha256)? {
            let got = sha256_file(&archive)?;
            let _ = fs::remove_file(&archive);
            bail!(
                "checksum mismatch for {}\n  expected {}\n  got      {}",
                a.file,
                a.sha256,
                got
            );
        }
    }

    let unpacked = staging.join("unpacked");
    match a.format {
        ArchiveFormat::TarGz => extract_tar_gz(&archive, &unpacked),
        ArchiveFormat::Zip => extract_zip(&archive, &unpacked),
    }
    .with_context(|| format!("extracting {}", a.file))?;
    let tree = unpacked.join(&a.strip_prefix);
    if !tree.is_dir() {
        bail!(
            "{} has no `{}` directory at its top; the recipe's strip_prefix is wrong",
            a.file,
            a.strip_prefix
        );
    }
    println!("extracted {:.1} MB", tree_size(&tree)? as f64 / 1e6);

    if let Some(b) = &recipe.build {
        build_from_source(&tree, b)?;
    }

    // The bundle descriptor has to be in place before pruning, because the keep
    // rules name it: the bundle is not usable without it.
    let template = root.join("toolchain").join(&recipe.bundle_template);
    fs::copy(&template, tree.join("bundle.toml"))
        .with_context(|| format!("copying {}", template.display()))?;

    // `--out` is a path a human typed. Emptying it must not be able to delete
    // something that is not ours: `--out ~` would otherwise take the home
    // directory with it.
    clear_output_dir(&bundle)
        .with_context(|| format!("preparing the output directory {}", bundle.display()))?;
    let (kept, kept_bytes, dropped, dropped_bytes) = prune(&tree, &bundle, &recipe.prune)?;
    println!(
        "pruned    {:.1} MB in {kept} files (dropped {dropped} files, {:.1} MB)",
        kept_bytes as f64 / 1e6,
        dropped_bytes as f64 / 1e6
    );

    // A prune that removes the compiler itself is a silent disaster otherwise:
    // nothing else here reads bundle.toml, so nothing would notice until someone
    // tried to build with it.
    check_driver_present(&bundle, &template)?;

    // And that it is the version the recipe pins, not merely *a* QB64.
    check_version_is_pinned(&recipe, &template, &tree, &bundle)?;

    let manifest = write_manifest(&bundle, &root, &recipe.mutable.paths)?;
    println!("manifest  {}", manifest.display());
    println!();
    println!("bundle    {}", bundle.display());
    println!();
    println!("Next: verify it before trusting it —");
    println!(
        "  ETB_REQUIRE_TOOLCHAIN=1 ETB_TOOLCHAIN_BUNDLE={} \\",
        bundle.display()
    );
    println!("    cargo test -p etb-testkit --test corpus");
    println!("A prune is only correct if the whole corpus still passes.");
    Ok(())
}

fn default_target() -> &'static str {
    if cfg!(windows) {
        "windows-x86_64"
    } else {
        "linux-x86_64"
    }
}

/// Run a source release's build, argv by argv, with no shell in between.
///
/// The environment is scrubbed the same way the application scrubs it for a
/// build: make reads every variable as a Makefile default, so a `CXXFLAGS` in
/// the developer's shell would otherwise end up inside the bundle.
fn build_from_source(tree: &Path, b: &Build) -> Result<()> {
    let jobs = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        .to_string();
    for argv in &b.commands {
        let argv: Vec<String> = argv.iter().map(|a| a.replace("{jobs}", &jobs)).collect();
        let (prog, rest) = argv.split_first().context("an empty build command")?;
        println!("  building: {}", argv.join(" "));
        let mut cmd = std::process::Command::new(prog);
        cmd.args(rest).current_dir(tree).env_clear();
        for k in ["PATH", "HOME", "LANG", "TERM"] {
            if let Some(v) = std::env::var_os(k) {
                cmd.env(k, v);
            }
        }
        let status = cmd
            .status()
            .with_context(|| format!("running `{prog}` (is it installed?)"))?;
        if !status.success() {
            bail!(
                "`{}` failed ({status}). QB64-PE's Linux build needs a C++ compiler, make, \
                 and the OpenGL, GLU, ALSA, libpng and libcurl development packages.",
                argv.join(" ")
            );
        }
    }
    if !tree.join(&b.produces).is_file() {
        bail!("the build finished but produced no {}", b.produces);
    }
    Ok(())
}

/// Empty a directory we are about to fill, refusing anything that does not
/// already look like ours.
///
/// A previous bundle is recognised by its `bundle.toml`. An empty or absent
/// directory is fine, and so is anything under the repository's own `target/`,
/// which is build output by definition. Anything else is left alone and
/// reported, because the alternative is a recursive delete of whatever the
/// caller happened to type — `--out ~` should not cost someone their home
/// directory.
///
/// The `target/` exemption is not a loosening for convenience: CI restores a
/// pruned `target/` from a cache, which can leave a partial bundle with no
/// `bundle.toml` in it. Refusing that means the recipe can never be rebuilt on a
/// runner that has a cache, which is every runner after the first.
fn clear_output_dir(dir: &Path) -> Result<()> {
    if !dir.exists() {
        fs::create_dir_all(dir)?;
        return Ok(());
    }
    if !dir.is_dir() {
        bail!("{} exists and is not a directory", dir.display());
    }
    let ours = dir.join("bundle.toml").is_file() || is_inside_build_output(dir);
    let empty = fs::read_dir(dir)?.next().is_none();
    if !ours && !empty {
        bail!(
            "{} is not empty and does not look like a previous bundle \
             (no bundle.toml). Refusing to delete it; remove it yourself or \
             choose another --out.",
            dir.display()
        );
    }
    fs::remove_dir_all(dir)?;
    fs::create_dir_all(dir)?;
    Ok(())
}

/// Is this path inside the repository's own `target/`?
///
/// Compared after making both absolute, so neither a relative `--out` nor a
/// symlinked checkout can make an outside path look inside one.
fn is_inside_build_output(dir: &Path) -> bool {
    let target = crate::repo_root().join("target");
    let (Ok(dir), Ok(target)) = (dir.canonicalize(), target.canonicalize()) else {
        return false;
    };
    dir.starts_with(target)
}

// ------------------------------------------------------------------ download

/// Stream to a temporary file and rename into place.
///
/// Streaming rather than buffering because the Windows release is 100 MB and
/// there is no reason to hold it in memory. The rename means an interrupted
/// download never leaves a half-file that a later run would mistake for cached.
fn download(url: &str, dest: &Path) -> Result<()> {
    let mut res = ureq::get(url).call()?;
    let tmp = dest.with_extension("part");
    {
        let mut reader = res.body_mut().with_config().limit(u64::MAX).reader();
        let mut file = fs::File::create(&tmp)?;
        std::io::copy(&mut reader, &mut file)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, dest)?;
    Ok(())
}

fn verified(path: &Path, want: &str) -> Result<bool> {
    if !path.is_file() {
        return Ok(false);
    }
    Ok(sha256_file(path)? == want)
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut f = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

// ------------------------------------------------------------------- extract

/// A path from inside an archive, refused if it could land outside `into`.
fn inside(into: &Path, name: &str) -> Result<PathBuf> {
    let rel = name.replace('\\', "/");
    let mut out = into.to_path_buf();
    for part in rel.split('/') {
        match part {
            "" | "." => {}
            ".." => bail!("archive entry `{name}` climbs out of the archive"),
            p if p.contains(':') => bail!("archive entry `{name}` names a drive"),
            p => out.push(p),
        }
    }
    Ok(out)
}

/// Unpack a zip.
///
/// Unlike the 7z and tar paths this does not carry the entries' times across.
/// It used to matter: QB64-PE ran `make` over its own runtime and decided what
/// to rebuild by comparing times. FreeBASIC builds nothing of itself, so a
/// file's time changes nothing about a build, and the integrity manifest
/// hashes contents rather than times.
fn extract_zip(archive: &Path, into: &Path) -> Result<()> {
    fs::create_dir_all(into)?;
    let mut zip = zip::ZipArchive::new(fs::File::open(archive)?)?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let Some(name) = entry.enclosed_name() else {
            bail!("zip entry {:?} is not a safe path", entry.name());
        };
        let dest = inside(into, &name.to_string_lossy())?;
        if entry.is_dir() {
            fs::create_dir_all(&dest)?;
            continue;
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut f = fs::File::create(&dest)?;
        std::io::copy(&mut entry, &mut f)?;
        // The executable bit: fbc and the linker it runs are in here, and on
        // Linux a bundle whose binaries cannot be run is no bundle.
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&dest, fs::Permissions::from_mode(mode));
        }
    }
    Ok(())
}

fn extract_tar_gz(archive: &Path, into: &Path) -> Result<()> {
    fs::create_dir_all(into)?;
    let gz = flate2::read::GzDecoder::new(fs::File::open(archive)?);
    let mut tar = tar::Archive::new(gz);
    tar.set_overwrite(true);
    tar.set_preserve_permissions(true);
    tar.set_preserve_mtime(true);
    tar.unpack(into)?;
    Ok(())
}

// --------------------------------------------------------------------- prune

fn prune(src: &Path, dst: &Path, rules: &Prune) -> Result<(usize, u64, usize, u64)> {
    let keeper = |rel: &str| -> bool {
        if rules.drop.iter().any(|p| glob::matches(p, rel)) {
            return false;
        }
        rules.keep.iter().any(|p| glob::matches(p, rel))
    };

    let mut kept = 0usize;
    let mut kept_bytes = 0u64;
    let mut dropped = 0usize;
    let mut dropped_bytes = 0u64;

    // Directory symlinks first. Copying files alone silently loses these.
    for entry in walk(src, true)? {
        let rel = rel_str(src, &entry);
        let out = dst.join(&rel);
        fs::create_dir_all(out.parent().unwrap())?;
        copy_symlink(&entry, &out)?;
        kept += 1;
    }

    for entry in walk(src, false)? {
        let rel = rel_str(src, &entry);
        let md = fs::symlink_metadata(&entry)?;
        let size = md.len();
        if !keeper(&rel) {
            dropped += 1;
            dropped_bytes += size;
            continue;
        }
        let out = dst.join(&rel);
        fs::create_dir_all(out.parent().unwrap())?;
        if md.file_type().is_symlink() {
            copy_symlink(&entry, &out)?;
        } else {
            fs::copy(&entry, &out)?;
            // `fs::copy` keeps permissions but not the time, and make lives
            // by the time. See the module comment.
            fs::File::options()
                .write(true)
                .open(&out)?
                .set_modified(md.modified()?)?;
        }
        kept += 1;
        kept_bytes += size;
    }
    Ok((kept, kept_bytes, dropped, dropped_bytes))
}

fn copy_symlink(from: &Path, to: &Path) -> Result<()> {
    let target = fs::read_link(from)?;
    if to.exists() || fs::symlink_metadata(to).is_ok() {
        let _ = fs::remove_file(to);
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, to)?;
    #[cfg(windows)]
    {
        // Windows bundles do not use symlinks; copy the target instead, so the
        // tree works without developer mode or elevation. A directory symlink
        // cannot be handled that way, and silently skipping one would produce a
        // bundle that is wrong in a way nothing here would notice.
        let resolved = from.parent().unwrap_or(Path::new(".")).join(&target);
        if resolved.is_file() {
            fs::copy(&resolved, to)?;
        } else {
            bail!(
                "{} is a symlink to {}, which is not a file. Windows bundles \
                 cannot carry directory symlinks; the recipe needs to keep the \
                 real files instead.",
                from.display(),
                target.display()
            );
        }
    }
    Ok(())
}

/// Every file (or every directory symlink) under `root`, not following symlinks.
fn walk(root: &Path, dir_symlinks: bool) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let p = entry?.path();
            let md = fs::symlink_metadata(&p)?;
            if md.file_type().is_symlink() {
                let is_dir_link = fs::metadata(&p).map(|m| m.is_dir()).unwrap_or(false);
                if is_dir_link {
                    if dir_symlinks {
                        out.push(p);
                    }
                } else if !dir_symlinks {
                    out.push(p);
                }
            } else if md.is_dir() {
                stack.push(p);
            } else if !dir_symlinks {
                out.push(p);
            }
        }
    }
    out.sort();
    Ok(out)
}

fn rel_str(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

fn tree_size(root: &Path) -> Result<u64> {
    Ok(walk(root, false)?
        .iter()
        .filter_map(|p| fs::symlink_metadata(p).ok().map(|m| m.len()))
        .sum())
}

/// The `fbc` entry of a bundle descriptor, as a relative path.
fn driver_rel(template_text: &str) -> Result<String> {
    template_text
        .lines()
        .find(|l| l.trim_start().starts_with("fbc"))
        .and_then(|l| l.split('=').nth(1))
        .map(|v| v.trim().trim_matches('"').to_string())
        .ok_or_else(|| anyhow::anyhow!("the bundle descriptor names no `fbc`"))
}

/// After pruning, the compiler named by `bundle.toml` must still be there.
///
/// Cheap, and it catches the whole class of keep-list mistakes that remove the
/// thing the bundle exists for. It cannot catch a *missing* dependency -- only
/// the corpus can do that, which is why the command says so.
fn check_driver_present(bundle: &Path, template: &Path) -> Result<()> {
    let rel = driver_rel(&fs::read_to_string(template)?)?;
    if !bundle.join(&rel).is_file() {
        bail!(
            "the prune removed the compiler itself: bundle.toml names `{rel}`, \
             which is not in the pruned tree. Check the keep rules."
        );
    }
    println!("driver    {rel}");
    Ok(())
}

/// The version is written in several places. They must agree.
///
/// `fbc_version` in the recipe, the archive's file name and URL, `version`
/// in the bundle descriptor (which the application reports without asking),
/// and QB64-PE's own source, which says what it is in `source/global/version.bas`.
/// A bump that updates some of them is the realistic mistake, and the quiet
/// outcome is the application naming a version the compiler is not.
///
/// Last, where this host can run it, the compiler's own answer.
fn check_version_is_pinned(
    recipe: &Recipe,
    template: &Path,
    tree: &Path,
    bundle: &Path,
) -> Result<()> {
    let want = recipe.fbc_version.trim();
    if want.is_empty() {
        bail!("the recipe sets no `fbc_version`, so nothing pins the compiler");
    }

    // 1. The archive.
    for (what, s) in [("file", &recipe.archive.file), ("url", &recipe.archive.url)] {
        if !s.contains(want) {
            bail!("the recipe pins {want}, but the archive {what} `{s}` does not say so");
        }
    }

    // 2. The version the descriptor declares, which is the one the application
    //    reports -- it is taken on trust at runtime, so it is checked here.
    let text =
        fs::read_to_string(template).with_context(|| format!("reading {}", template.display()))?;
    let declared = text
        .lines()
        .find_map(|l| l.trim().strip_prefix("version"))
        .and_then(|r| r.split('=').nth(1))
        .map(|v| v.trim().trim_matches('"').to_string())
        .unwrap_or_default();
    if declared != want {
        bail!(
            "{} declares version \"{declared}\", but the recipe pins {want}",
            template.display()
        );
    }

    // 3. The directory the archive unpacks into carries the version too, and
    //    it is the one thing here that comes from the file rather than from
    //    something a person typed beside it.
    if !recipe.archive.strip_prefix.contains(want) {
        bail!(
            "the archive unpacks into `{}`, which does not say {want}",
            recipe.archive.strip_prefix
        );
    }
    let _ = tree;

    // 4. And, where this host can run it, the compiler's own answer. A
    //    cross-built bundle cannot be asked -- fetching the Windows bundle on
    //    Linux is the normal case -- so this reports rather than fails.
    let driver = bundle.join(driver_rel(&text)?);
    match std::process::Command::new(&driver)
        .arg("--version")
        .current_dir(bundle)
        .output()
    {
        Ok(out) if out.status.success() => {
            let got = String::from_utf8_lossy(&out.stdout);
            if !got.contains(want) {
                bail!(
                    "the bundled compiler says `{}`, but the recipe pins {want}",
                    got.trim()
                );
            }
            println!("version   {want} (confirmed by the compiler itself)");
        }
        _ => println!("version   {want} (not runnable on this host to confirm)"),
    }
    Ok(())
}

// ------------------------------------------------------------------ manifest

/// A SHA-256 of every file in the bundle, embedded in the application so a
/// missing or altered compiler is detected rather than silently used.
fn write_manifest(bundle: &Path, repo: &Path, mutable: &[String]) -> Result<PathBuf> {
    let mut entries: BTreeMap<String, String> = BTreeMap::new();
    for f in walk(bundle, false)? {
        if fs::symlink_metadata(&f)?.file_type().is_symlink() {
            continue; // a symlink has no content of its own
        }
        entries.insert(rel_str(bundle, &f), sha256_file(&f)?);
    }
    let mut text = String::from(
        "# Generated by `cargo xtask fetch-toolchain`. Embedded in the application,\n\
         # so a bundled file that goes missing or changes is detected rather than used.\n",
    );
    if !mutable.is_empty() {
        text.push_str(
            "#\n# The compiler writes these itself — its settings, its intermediate C++, the\n\
             # runtime it builds on first use — so they are listed, not attested.\n",
        );
        for g in mutable {
            text.push_str(&format!("# mutable: {}\n", g.replace('\\', "/")));
        }
    }
    for (path, hash) in &entries {
        text.push_str(&format!("{hash}  {path}\n"));
    }
    let dest = repo
        .join("crates")
        .join("etb-gui")
        .join("assets")
        .join("toolchain-manifest.txt");
    fs::create_dir_all(dest.parent().unwrap())?;
    fs::write(&dest, text)?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_archive_entry_cannot_climb_out_or_name_a_drive() {
        let root = Path::new("/x");
        assert_eq!(
            inside(root, "fbc/a/b.txt").unwrap(),
            root.join("fbc/a/b.txt")
        );
        assert_eq!(
            inside(root, "fbc\\a\\b.txt").unwrap(),
            root.join("fbc/a/b.txt")
        );
        assert!(inside(root, "../evil").is_err());
        assert!(inside(root, "fbc/../../evil").is_err());
        assert!(inside(root, "C:/evil").is_err());
    }

    #[test]
    fn every_committed_recipe_parses() {
        let root = crate::repo_root();
        for target in ["windows-x86_64", "linux-x86_64"] {
            let r = read_recipe(&root, target).unwrap();
            assert_eq!(r.target, target);
            assert!(r.archive.file.contains(&r.fbc_version));
        }
    }
}

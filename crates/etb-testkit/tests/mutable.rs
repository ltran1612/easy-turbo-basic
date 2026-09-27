//! Tier 2: a build writes nothing inside the compiler at all.
//!
//! The integrity check hashes the bundled compiler against a manifest, and
//! skips the paths the recipe lists as `[mutable]`. QB64-PE needed that list:
//! it built its own runtime, kept settings, and wrote its intermediate C++
//! inside its own directory. FreeBASIC writes where it is told and nowhere
//! else, so the list is empty — and this test is what keeps that true, because
//! the first user to be told "the compiler has been altered" after a perfectly
//! ordinary build would have no way to know it was a false alarm.

use etb_core::build::{self, BuildRequest};
use etb_core::fs_guard::FsGuard;
use etb_core::glob;
use etb_core::paths::{AppPaths, WorkLayout};
use etb_core::project::{Program, SourceRef};
use etb_testkit::corpus::toolchain;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// The recipe that produced this compiler. `fetch-toolchain` puts a bundle in
/// a directory named for its target, which is the only label the bundle
/// carries; failing that, the host's own target, which is what a development
/// checkout has.
fn mutable_globs(root: &Path) -> Option<Vec<String>> {
    let host = format!(
        "{}-{}",
        if cfg!(windows) { "windows" } else { "linux" },
        std::env::consts::ARCH
    );
    let named = root.file_name().map(|n| n.to_string_lossy().into_owned());
    for target in named.into_iter().chain([host]) {
        let path = repo().join("toolchain").join(format!("{target}.toml"));
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let v: toml::Value = toml::from_str(&text).expect("the recipe parses");
        let paths = v
            .get("mutable")
            .and_then(|m| m.get("paths"))
            .and_then(|p| p.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|g| g.as_str().map(str::to_string))
                    .collect()
            });
        eprintln!("checked against {}", path.display());
        return paths;
    }
    None
}

/// Size and modification time of every file under `root`, relative and
/// `/`-separated — enough to see what a build touched, and cheap on a tree of
/// tens of thousands of files.
fn census(root: &Path) -> BTreeMap<String, (u64, std::time::SystemTime)> {
    fn walk(dir: &Path, root: &Path, out: &mut BTreeMap<String, (u64, std::time::SystemTime)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let path = e.path();
            let Ok(md) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if md.is_dir() {
                walk(&path, root, out);
            } else {
                let rel = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let when = md.modified().unwrap_or(std::time::UNIX_EPOCH);
                out.insert(rel, (md.len(), when));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

#[test]
fn a_build_writes_only_where_the_recipe_says_it_may() {
    let Some(tc) = toolchain() else { return };
    let root = tc.root().to_path_buf();
    // An empty list is the answer for FreeBASIC, and it has to be written
    // down rather than absent: a missing `[mutable]` section would mean
    // nobody had thought about it.
    let globs = mutable_globs(&root)
        .unwrap_or_else(|| panic!("no [mutable] section in the recipe for {}", root.display()));

    let before = census(&root);

    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("MUTABLE.BAS");
    std::fs::write(&src, b"PRINT \"x\"\r\nEND\r\n").unwrap();
    let paths = AppPaths::under(tmp.path().join("app"));
    let guard = FsGuard::new(paths.write_roots().to_vec()).unwrap();
    let mut program = Program::new("mutable");
    program.sources.push(SourceRef::new(src));
    // Deliberately no `prepare`: the compiler is used where it is installed,
    // which is the tree being watched.
    let outcome = build::build_with(
        &guard,
        &tc,
        &WorkLayout::new(paths.build_dir(1)),
        &program,
        &BuildRequest::default(),
        None,
        &AtomicBool::new(false),
    );
    assert!(outcome.success, "{outcome:?}");

    let after = census(&root);
    let mut stray = Vec::new();
    let mut touched = 0usize;
    for (rel, now) in &after {
        let changed = match before.get(rel) {
            Some(was) => was != now,
            None => true,
        };
        if !changed {
            continue;
        }
        touched += 1;
        if !globs.iter().any(|g| glob::matches(g, rel)) {
            stray.push(rel.clone());
        }
    }
    // Nothing is expected to change. That the census sees the compiler at
    // all is checked by the count, so a test pointed at an empty directory
    // cannot pass by finding nothing in it.
    assert!(
        before.len() > 100,
        "only {} files under {}: this is not a compiler",
        before.len(),
        root.display()
    );
    let _ = touched;
    // A file the build *removed* would fail the integrity check just as loudly.
    for rel in before.keys() {
        if !after.contains_key(rel) && !globs.iter().any(|g| glob::matches(g, rel)) {
            stray.push(format!("{rel} (removed)"));
        }
    }
    assert!(
        stray.is_empty(),
        "a build wrote inside the compiler where the recipe does not allow it.\n\
         Add these to `[mutable] paths` in the recipe, or find out why they moved:\n  {}",
        stray.join("\n  ")
    );
}

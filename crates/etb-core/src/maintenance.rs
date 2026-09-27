//! Throwing away what the application made for itself.
//!
//! Two directories hold nothing the user would miss: the work trees a build
//! writes into, and the copy of the compiler made for a machine whose own path
//! QB64-PE cannot be given (`toolchain::Toolchain::prepare`). Both are rebuilt
//! on demand, and together they are most of a gigabyte.
//!
//! They are cleared from two places: a button in Settings, when a build has
//! gone strange enough that starting the compiler afresh is worth a try, and
//! the uninstaller, which would otherwise leave that gigabyte behind for
//! having been tidy enough to put it outside its own directory.
//!
//! What is *not* cleared: the configuration. The user's list of programs is
//! theirs, and surviving an uninstall is the behaviour the installer promises.

use crate::error::Result;
use crate::fs_guard::FsGuard;
use crate::paths::AppPaths;
use std::path::Path;

/// Remove the work trees and any copy of the compiler. Returns how many bytes
/// went, as far as it could tell.
pub fn clear_scratch(paths: &AppPaths, guard: &FsGuard) -> Result<u64> {
    let mut freed = 0;
    for root in [paths.work_root(), paths.tool_root()] {
        // The roots themselves are write roots, which `remove_dir_all` refuses
        // by design — it is the guard against a bug that deletes everything we
        // are allowed to write. So the contents go, one at a time.
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for e in entries.flatten() {
            let path = e.path();
            freed += size_of(&path);
            if path.is_dir() {
                guard.remove_dir_all(&path)?;
            } else {
                guard.remove_file(&path)?;
            }
        }
    }
    Ok(freed)
}

fn size_of(path: &Path) -> u64 {
    let Ok(md) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if !md.is_dir() {
        return md.len();
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries.flatten().map(|e| size_of(&e.path())).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_work_trees_and_the_compiler_copy_go_and_the_settings_stay() {
        let td = tempfile::tempdir().unwrap();
        let paths = AppPaths::under(td.path());
        let guard = FsGuard::new(paths.write_roots().to_vec()).unwrap();

        guard
            .write_file(&paths.build_dir(1).join("src").join("prog.bas"), b"PRINT 1")
            .unwrap();
        guard
            .write_file(&paths.tool_root().join("abc123").join("fbc"), b"compiler")
            .unwrap();
        guard
            .write_file(&paths.settings_file(), b"language = \"vi\"\n")
            .unwrap();

        let freed = clear_scratch(&paths, &guard).unwrap();
        assert_eq!(freed, b"PRINT 1".len() as u64 + b"compiler".len() as u64);
        assert!(!paths.tool_root().join("abc123").exists());
        assert!(!paths.build_dir(1).exists());
        assert!(
            paths.work_root().is_dir() && paths.tool_root().is_dir(),
            "the directories themselves stay: they are where the next build goes"
        );
        assert!(
            paths.settings_file().is_file(),
            "the user's settings are not scratch"
        );
    }

    #[test]
    fn clearing_twice_is_not_an_error() {
        let td = tempfile::tempdir().unwrap();
        let paths = AppPaths::under(td.path());
        let guard = FsGuard::new(paths.write_roots().to_vec()).unwrap();
        assert_eq!(clear_scratch(&paths, &guard).unwrap(), 0);
        assert_eq!(clear_scratch(&paths, &guard).unwrap(), 0);
    }
}

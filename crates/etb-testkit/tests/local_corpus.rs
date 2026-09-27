//! Tier 2, local only: real programs that are not ours to publish.
//!
//! `ETB_LOCAL_CORPUS` names a directory of case directories, each holding an
//! `expect.toml` whose `sources` point at the program where it already lives.
//! Nothing about these programs — not their names, not the answers they are
//! fed, not the numbers they print — is committed: the cases are kept beside
//! the programs, outside the repository.
//!
//! Skipped when the variable is unset, unless `ETB_REQUIRE_LOCAL_CORPUS=1`.
//! Every case is built from a copy, and the real file is hashed before and
//! after besides.

use etb_core::fs_guard::FsGuard;
use etb_core::paths::AppPaths;
use etb_testkit::corpus::{cases, run_case, toolchain};
use std::path::PathBuf;

#[test]
fn every_local_case_behaves_as_documented() {
    let root = match std::env::var_os("ETB_LOCAL_CORPUS") {
        Some(r) if !r.is_empty() => PathBuf::from(r),
        _ => {
            if std::env::var("ETB_REQUIRE_LOCAL_CORPUS").as_deref() == Ok("1") {
                panic!("ETB_REQUIRE_LOCAL_CORPUS=1 but ETB_LOCAL_CORPUS is not set");
            }
            eprintln!("SKIPPING the local corpus: ETB_LOCAL_CORPUS is not set");
            return;
        }
    };
    let cases = cases(&root);
    assert!(!cases.is_empty(), "no cases under {}", root.display());
    let Some(tc) = toolchain() else { return };

    let tmp = tempfile::tempdir().unwrap();
    let paths = AppPaths::under(tmp.path().join("app"));
    let guard = FsGuard::new(paths.write_roots().to_vec()).unwrap();

    let mut failures = Vec::new();
    for (i, case) in cases.iter().enumerate() {
        let verdict = run_case(
            &tc,
            case,
            &tmp.path().join("sources"),
            &paths,
            &guard,
            i as u64 + 1,
        );
        eprintln!(
            "  {:<18} -> {}",
            case.name,
            if verdict.is_ok() { "ok" } else { "MISMATCH" }
        );
        if let Err(e) = verdict {
            failures.push(format!("[{}] {e}", case.name));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

//! Tier 2: a real QB64-PE against a corpus of Turbo Basic programs.
//!
//! Skipped automatically when no compiler is available, so the rest of the
//! suite still runs on a bare machine. Set `ETB_REQUIRE_TOOLCHAIN=1` (CI does)
//! to turn a missing compiler into a failure instead of a skip.
//!
//! Every program here was written for this corpus: `check-hygiene` refuses a
//! `.BAS` in the repository that does not say so on its first line.

use etb_core::build;
use etb_core::fs_guard::FsGuard;
use etb_core::paths::{AppPaths, WorkLayout};
use etb_core::project::{Program, SourceRef};
use etb_testkit::corpus::{cases, run_built, run_case, toolchain};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus")
}

#[test]
fn the_corpus_is_well_formed() {
    // Runs with or without a compiler.
    let cases = cases(&corpus_root());
    assert!(cases.len() >= 3, "expected a corpus");
    for c in &cases {
        assert!(
            !c.expect.description.trim().is_empty(),
            "{}: no description",
            c.name
        );
        assert!(
            !c.expect.sources.is_empty(),
            "{}: no sources listed",
            c.name
        );
        for s in &c.expect.sources {
            assert!(
                c.dir.join(s).exists(),
                "{}: expect.toml lists {s}, which does not exist",
                c.name
            );
        }
    }
}

#[test]
fn every_corpus_case_behaves_as_documented() {
    let Some(tc) = toolchain() else { return };
    eprintln!("using {}", tc.id().display());

    let tmp = tempfile::tempdir().unwrap();
    let paths = AppPaths::under(tmp.path().join("app"));
    let guard = FsGuard::new(paths.write_roots().to_vec()).unwrap();

    let mut failures: Vec<String> = Vec::new();
    for (i, case) in cases(&corpus_root()).iter().enumerate() {
        let t0 = std::time::Instant::now();
        let verdict = run_case(
            &tc,
            case,
            &tmp.path().join("sources"),
            &paths,
            &guard,
            i as u64 + 1,
        );
        eprintln!(
            "  {:<18} {:>6.0}ms -> {}",
            case.name,
            t0.elapsed().as_secs_f64() * 1000.0,
            if verdict.is_ok() { "ok" } else { "MISMATCH" }
        );
        if let Err(e) = verdict {
            failures.push(format!(
                "[{}] {e}\n    {}",
                case.name,
                case.expect.description.trim()
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} corpus case(s) failed:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

/// Vietnamese diacritics everywhere a path can carry them.
///
/// On a Vietnamese Windows install the user's programs sit in folders the user
/// named themselves, and the application's own directories are under
/// `C:\Users\Nguyễn Văn A\...`. The user's paths are only ever read by us, and
/// QB64 only ever sees our ASCII-named copy, so none of this should reach it —
/// which is exactly what needs proving.
///
/// The application's own directories are the other half, and the harder one:
/// a Windows QB64-PE receives its arguments through the ANSI code page, which
/// has no Vietnamese, so the path to our staged copy must itself be ASCII.
/// On Windows `choose_work_root` sees to that by moving the work tree under
/// ProgramData. That fallback is Windows-only, so when the Windows bundle is
/// driven from Linux through wine the application's directories are given an
/// ASCII path here instead, the way Windows would have; under wine it was
/// seen to fail otherwise, with the path arriving as `Nguy?n Van A`.
#[test]
fn paths_full_of_vietnamese_diacritics_build_and_run() {
    let Some(tc) = toolchain() else { return };

    let tmp = tempfile::tempdir().unwrap();
    let user_dir = tmp
        .path()
        .join("Nguyễn Văn A")
        .join("Tài liệu")
        .join("Dự án Đường sắt");
    std::fs::create_dir_all(&user_dir).unwrap();
    std::fs::write(
        user_dir.join("Tính hệ số.BAS"),
        b"' Written for Easy Turbo Basic\r\nheso = 2.5\r\nprint using \"HESO = #.##\"; heso\r\nend\r\n",
    )
    .unwrap();

    let app_base = if tc.launcher().is_some() {
        tmp.path().join("ProgramData")
    } else {
        tmp.path()
            .join("Nguyễn Văn A")
            .join("AppData")
            .join("Local")
    };
    let paths = AppPaths::under(app_base);
    let guard = FsGuard::new(paths.write_roots().to_vec()).unwrap();
    let layout = WorkLayout::new(paths.build_dir(1));

    let mut program = Program::new("Tính dầm bê tông");
    program
        .sources
        .push(SourceRef::new(user_dir.join("Tính hệ số.BAS")));

    let req = build::BuildRequest {
        test_mode: Some(etb_core::translate::TestMode::Console),
        ..Default::default()
    };
    let outcome = build::build_with(
        &guard,
        &tc,
        &layout,
        &program,
        &req,
        None,
        &AtomicBool::new(false),
    );
    assert!(
        outcome.success,
        "a build under Vietnamese paths failed:\n{}",
        outcome.raw
    );

    let run_dir = layout.root.join("corpus-run");
    std::fs::create_dir_all(&run_dir).unwrap();
    let (code, text) =
        run_built(&tc, &outcome.exe.unwrap(), &run_dir, "").expect("the program should run");
    assert_eq!(code, Some(0), "output was:\n{text}");
    assert!(text.contains("HESO = 2.50"), "output:\n{text}");
}

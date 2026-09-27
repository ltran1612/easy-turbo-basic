//! Exercises the *bundled* toolchain path.
//!
//! A bundle is a directory with a `bundle.toml` and a `fbc` in it. These
//! tests lay one out around the fake `fbc`, so discovery, the descriptor and
//! the environment plumbing are proved on any machine, with or without a real
//! FreeBASIC.

use etb_core::build;
use etb_core::fs_guard::FsGuard;
use etb_core::paths::{AppPaths, WorkLayout};
use etb_core::project::{Program, SourceRef};
use etb_core::toolchain::{bundle::Bundle, Toolchain, ToolchainKind};
use std::path::Path;
use std::sync::atomic::AtomicBool;

const FAKE_FBC: &str = env!("CARGO_BIN_EXE_fake_fbc");

/// Lay out a bundle around the fake `fbc`.
fn make_bundle(root: &Path, extra_toml: &str) {
    std::fs::create_dir_all(root.join("internal/c/c_compiler/bin")).unwrap();
    let exe = format!("fbc{}", std::env::consts::EXE_SUFFIX);
    std::fs::copy(FAKE_FBC, root.join(&exe)).unwrap();
    std::fs::write(
        root.join("bundle.toml"),
        format!(
            "id = \"test-bundle\"\nversion = \"4.6.0\"\nfbc = \"{exe}\"\n\
             path_dirs = [\"internal/c/c_compiler/bin\"]\n{extra_toml}"
        ),
    )
    .unwrap();
}

#[test]
fn a_bundle_descriptor_round_trips_without_a_compiler() {
    let td = tempfile::tempdir().unwrap();
    make_bundle(td.path(), "");
    let b = Bundle::load(td.path()).unwrap().unwrap();
    assert_eq!(b.id, "test-bundle");
    assert_eq!(b.version, "4.6.0");
    assert!(b.fbc.is_file());
    assert_eq!(
        b.path_dirs,
        vec![b
            .root
            .join("internal")
            .join("c")
            .join("c_compiler")
            .join("bin")]
    );
}

#[test]
fn a_bundled_toolchain_builds_a_program() {
    let td = tempfile::tempdir().unwrap();
    let root = td.path().join("toolchain");
    make_bundle(&root, "");
    let tc = Toolchain::from_bundle(&root).unwrap();
    assert_eq!(tc.id().kind, ToolchainKind::Bundled);
    assert_eq!(tc.id().version, "4.6.0", "taken from the descriptor");
    assert_eq!(tc.home(), dunce::canonicalize(&root).unwrap());

    let user = td.path().join("Documents");
    std::fs::create_dir_all(&user).unwrap();
    std::fs::write(user.join("A.BAS"), b"PRINT 1\r\nEND\r\n").unwrap();
    let mut p = Program::new("t");
    p.sources.push(SourceRef::new(user.join("A.BAS")));

    let paths = AppPaths::under(td.path().join("app"));
    let guard = FsGuard::new(paths.write_roots().to_vec()).unwrap();
    let layout = WorkLayout::new(paths.build_dir(1));
    let outcome = build::build(&guard, &tc, &layout, &p, None, &AtomicBool::new(false));
    assert!(outcome.success, "{outcome:?}");
    assert!(outcome.exe.unwrap().is_file());
}

#[test]
fn a_bundle_for_another_platform_names_its_targets_suffix() {
    // How the Windows bundle is driven from Linux: its programs end in .exe
    // wherever it runs, and the build must look for that name.
    let td = tempfile::tempdir().unwrap();
    make_bundle(td.path(), "exe_suffix = \".exe\"\n");
    let tc = Toolchain::from_bundle(td.path()).unwrap();
    assert_eq!(tc.exe_suffix(), ".exe");
}

#[test]
fn a_bundle_missing_its_compiler_is_reported_as_a_damaged_installation() {
    let td = tempfile::tempdir().unwrap();
    make_bundle(td.path(), "");
    std::fs::remove_file(
        td.path()
            .join(format!("fbc{}", std::env::consts::EXE_SUFFIX)),
    )
    .unwrap();
    match Toolchain::from_bundle(td.path()) {
        Err(etb_core::EtbError::ToolchainInvalid { reason, .. }) => {
            assert!(reason.contains("antivirus"), "{reason}");
        }
        other => panic!("expected a damaged-installation error, got {other:?}"),
    }
}

// -------------------------------------------- a real bundle at a real path

/// A real FreeBASIC installed where a Vietnamese user name puts it still builds.
///
/// Ignored by default: it needs the fetched Windows bundle and copies it, so
/// it costs a gigabyte and a minute. `toolchain/verify-under-wine.sh` runs it,
/// because wine is where the problem it is about was first seen — a Windows
/// A Windows `fbc` takes its arguments through the ANSI code page, which has no
/// Vietnamese, so `Nguyễn Văn A` reaches it as `Nguy?n Van A`
/// (`docs/verification.md`, F3).
///
/// The bundle is put at the Vietnamese path outright rather than linked there:
/// a bundle resolves its own root, so a link would lead straight back to the
/// plain path and the test would prove nothing.
#[test]
#[ignore = "needs a fetched bundle; run by toolchain/verify-under-wine.sh"]
fn a_compiler_installed_under_a_vietnamese_name_still_builds() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap();
    let real = match std::env::var_os("ETB_TOOLCHAIN_BUNDLE") {
        Some(p) => std::path::PathBuf::from(p),
        None => repo.join("target").join("toolchain").join("windows-x86_64"),
    };
    assert!(
        real.join("bundle.toml").is_file(),
        "no bundle at {} — run `cargo xtask fetch-toolchain` first",
        real.display()
    );

    // Everything this test writes stays on the same disk as the repository:
    // the copy is the size of a compiler, which is no size for a temp file
    // system.
    let scratch = repo.join("target").join("vietnamese-path-test");
    let _ = std::fs::remove_dir_all(&scratch);
    let installed = scratch.join("Người dùng").join("Easy Turbo Basic");
    std::fs::create_dir_all(&scratch).unwrap();
    let laying_out = FsGuard::new(vec![scratch.clone()]).unwrap();
    laying_out
        .materialise_tree(&real, &installed, &AtomicBool::new(false))
        .unwrap()
        .expect("laying out the installation");

    let tc = Toolchain::from_bundle(&installed).unwrap();
    assert!(
        !tc.usable_in_place(),
        "the installed path is ASCII, so this test proves nothing"
    );

    let paths = AppPaths::under(scratch.join("app"));
    let guard = FsGuard::new(paths.write_roots().to_vec()).unwrap();
    let src = scratch.join("Chương trình.BAS");
    std::fs::write(&src, b"PRINT \"CHAO\"\r\nEND\r\n").unwrap();
    let mut program = Program::new("vietnamese");
    program.sources.push(SourceRef::new(src));

    let started = std::time::Instant::now();
    let req = build::BuildRequest {
        prepare: Some(build::Preparation {
            tool_root: paths.tool_root(),
            lock_dir: paths.lock_dir(),
            manifest: std::sync::Arc::new(Default::default()),
        }),
        ..Default::default()
    };
    let outcome = build::build_with(
        &guard,
        &tc,
        &WorkLayout::new(paths.build_dir(1)),
        &program,
        &req,
        None,
        &AtomicBool::new(false),
    );
    eprintln!("first build, including the copy: {:?}", started.elapsed());
    assert!(outcome.success, "{outcome:?}");
    assert!(outcome.exe.as_ref().is_some_and(|e| e.is_file()));

    // Second time round the copy is reused, so this build is an ordinary one.
    let again = std::time::Instant::now();
    let outcome = build::build_with(
        &guard,
        &tc,
        &WorkLayout::new(paths.build_dir(2)),
        &program,
        &req,
        None,
        &AtomicBool::new(false),
    );
    eprintln!("second build, reusing the copy: {:?}", again.elapsed());
    assert!(outcome.success, "{outcome:?}");
    let _ = std::fs::remove_dir_all(&scratch);
}

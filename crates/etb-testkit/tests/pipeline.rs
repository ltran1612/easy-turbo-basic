//! Tier 1: the whole build pipeline, driven by a fake FreeBASIC.
//!
//! These tests need no compiler, no window server and no network, so they are the
//! ones that run everywhere and catch the most.

use etb_core::build::{self, diagnostics::Severity, BuildEvent, BuildPhase};
use etb_core::fs_guard::FsGuard;
use etb_core::paths::{AppPaths, WorkLayout};
use etb_core::project::{Program, SourceRef};
use etb_core::toolchain::{Toolchain, ToolchainKind};
use etb_testkit::hash_tree;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const FAKE_FBC: &str = env!("CARGO_BIN_EXE_fake_fbc");
const FAKE_PROGRAM: &str = env!("CARGO_BIN_EXE_fake_program");

struct Fixture {
    _tmp: tempfile::TempDir,
    /// Stands in for the user's Documents folder. Nothing here may ever change.
    user_files: PathBuf,
    paths: AppPaths,
    guard: FsGuard,
    layout: WorkLayout,
}

impl Fixture {
    fn new(sources: &[(&str, &[u8])]) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let user_files = tmp.path().join("Documents").join("BASIC");
        std::fs::create_dir_all(&user_files).unwrap();
        for (name, body) in sources {
            std::fs::write(user_files.join(name), body).unwrap();
        }
        let paths = AppPaths::under(tmp.path().join("app"));
        let guard = FsGuard::new(paths.write_roots().to_vec()).unwrap();
        let layout = WorkLayout::new(paths.build_dir(1));
        Self {
            _tmp: tmp,
            user_files,
            paths,
            guard,
            layout,
        }
    }

    fn program(&self, names: &[&str]) -> Program {
        let mut p = Program::new("test");
        p.sources = names
            .iter()
            .map(|n| SourceRef::new(self.user_files.join(n)))
            .collect();
        p
    }

    /// Tell the fake compiler how to behave for this build only.
    fn set_mode(&self, mode: &str) {
        self.guard.create_dir_all(&self.layout.root).unwrap();
        self.guard
            .write_file(&self.layout.root.join("FAKE_MODE"), mode.as_bytes())
            .unwrap();
    }

    fn set_program(&self, p: &str) {
        self.guard
            .write_file(&self.layout.root.join("FAKE_PROGRAM"), p.as_bytes())
            .unwrap();
    }

    fn toolchain(&self) -> Toolchain {
        Toolchain::from_path(PathBuf::from(FAKE_FBC), ToolchainKind::UserSpecified).unwrap()
    }

    fn build(&self, program: &Program) -> build::BuildOutcome {
        let cancel = AtomicBool::new(false);
        build::build(
            &self.guard,
            &self.toolchain(),
            &self.layout,
            program,
            None,
            &cancel,
        )
    }

    fn argv_log(&self) -> String {
        std::fs::read_to_string(self.layout.root.join("argv.log")).unwrap_or_default()
    }
}

const SRC: &[u8] = b"10 PRINT \"HELLO\"\r\n20 END\r\n";

// ---------------------------------------------------------------- file safety

#[test]
fn a_build_never_modifies_the_users_files() {
    // The top-priority requirement, expressed executably. The program has
    // every construct the translator rewrites, so a rewrite that somehow wrote
    // back would show here.
    let f = Fixture::new(&[
        (
            "CALC.BAS",
            b"\r\n? \"x\"\r\nopen f$ for output as 4\r\nprint# 4,\"y\"\r\nclose\r\nend\r\n\x1a",
        ),
        ("OTHER.INC", b"' included\r\n"),
        ("NOTES.TXT", b"not part of the program\r\n"),
    ]);
    let before = hash_tree(&f.user_files);
    assert_eq!(before.len(), 3);

    f.set_mode("ok");
    let outcome = f.build(&f.program(&["CALC.BAS", "OTHER.INC"]));
    assert!(outcome.success, "{outcome:?}");

    let after = hash_tree(&f.user_files);
    assert_eq!(before, after, "the user's files changed during a build");
}

#[test]
fn a_failed_build_also_leaves_user_files_alone() {
    let f = Fixture::new(&[("BAD.BAS", b"x = = 1 ' ERROR_HERE\r\n")]);
    let before = hash_tree(&f.user_files);
    f.set_mode("syntax");
    let outcome = f.build(&f.program(&["BAD.BAS"]));
    assert!(!outcome.success);
    assert_eq!(before, hash_tree(&f.user_files));
}

#[test]
fn every_artefact_lands_inside_the_work_tree() {
    let f = Fixture::new(&[("A.BAS", SRC)]);
    f.set_mode("ok");
    let outcome = f.build(&f.program(&["A.BAS"]));
    assert!(outcome.success, "{outcome:?}");

    let names: Vec<String> = std::fs::read_dir(&f.user_files)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names,
        vec!["A.BAS".to_string()],
        "nothing new beside the source"
    );

    assert!(f.layout.src().join("prog.bas").exists());
    assert!(f.layout.src().join("etb_prelude.bas").exists());
    let exe = outcome.exe.unwrap();
    assert!(exe.starts_with(f.paths.data_dir()));
}

// ---------------------------------------------------------------- errors

#[test]
fn an_error_is_reported_on_the_users_file_and_line_with_the_users_text() {
    // Line 4 is rewritten by the translator (`print#` gets its space, `?`
    // becomes PRINT) and is also where the fake reports its error. The
    // message must still name the user's file, line 4, and quote the line as
    // the user typed it.
    let f = Fixture::new(&[(
        "TINH.BAS",
        b"\r\n?\r\nopen f$ for output as 4\r\nprint# 4,space$(15);\"x\" ' ERROR_HERE\r\nend\r\n",
    )]);
    f.set_mode("syntax");
    let outcome = f.build(&f.program(&["TINH.BAS"]));
    assert!(!outcome.success);
    assert_eq!(outcome.failed_at, Some(build::FailedAt::Compile));
    assert_eq!(outcome.errors, 1);

    let d = &outcome.diagnostics[0];
    assert_eq!(d.severity, Severity::Error);
    assert_eq!(d.message, "Expected '='");
    assert_eq!(d.file.as_deref(), Some("TINH.BAS"));
    assert_eq!(d.line, Some(4));
    assert_eq!(d.snippet, ["print# 4,space$(15);\"x\" ' ERROR_HERE"]);
    assert!(!d.ours, "this one is in the user's code");
    assert!(outcome.internal_error.is_none());
}

#[test]
fn an_error_in_our_own_runtime_support_is_owned_as_ours() {
    let f = Fixture::new(&[("A.BAS", SRC)]);
    f.set_mode("include_err");
    let outcome = f.build(&f.program(&["A.BAS"]));
    assert!(!outcome.success);
    assert!(outcome.diagnostics[0].ours);
    assert!(
        outcome.internal_error.is_some(),
        "the user must not be told to fix our file"
    );
}

#[test]
fn a_failure_behind_the_compiler_is_reported_as_ours_with_its_output() {
    let f = Fixture::new(&[("A.BAS", SRC)]);
    f.set_mode("cpp_fail");
    let outcome = f.build(&f.program(&["A.BAS"]));
    assert!(!outcome.success);
    assert!(outcome.internal_error.is_some());
    assert!(outcome.raw.contains("cannot find -lXext"));
}

#[test]
fn a_runaway_error_cascade_is_capped() {
    let f = Fixture::new(&[("A.BAS", SRC)]);
    f.set_mode("flood");
    let outcome = f.build(&f.program(&["A.BAS"]));
    assert!(!outcome.success);
    assert!(
        outcome.raw.len() <= build::COMPILER_OUTPUT_CAP + 1024,
        "raw output was {} bytes",
        outcome.raw.len()
    );
}

#[test]
fn a_missing_file_is_refused_before_anything_is_staged() {
    let f = Fixture::new(&[("A.BAS", SRC)]);
    f.set_mode("ok");
    let mut p = f.program(&["A.BAS"]);
    p.sources
        .push(SourceRef::new(f.user_files.join("GONE.BAS")));
    let outcome = f.build(&p);
    assert!(!outcome.success);
    assert!(outcome.internal_error.is_some());
    assert!(!f.layout.src().join("prog.bas").exists());
}

#[test]
fn a_file_that_is_not_a_program_is_explained_not_compiled() {
    let f = Fixture::new(&[("A.BAS", b"\xff\x0b\x08\x0a\x00\x91\x20\x12\x00\x00\x00")]);
    f.set_mode("ok");
    let outcome = f.build(&f.program(&["A.BAS"]));
    assert!(!outcome.success);
    let fp = outcome.file_problem.expect("a named problem");
    assert_eq!(fp.name, "A.BAS");
    assert_eq!(fp.reason_key, "src.reject.gw_tokenized");
    assert!(
        f.argv_log().is_empty(),
        "the compiler must not have been run"
    );
}

// ---------------------------------------------------------------- events

#[test]
fn the_event_stream_reports_each_phase_in_order() {
    let f = Fixture::new(&[("A.BAS", SRC)]);
    f.set_mode("ok");
    let cancel = Arc::new(AtomicBool::new(false));
    let rx = build::spawn(
        f.guard.clone(),
        Arc::new(f.toolchain()),
        f.layout.clone(),
        f.program(&["A.BAS"]),
        build::BuildRequest::default(),
        cancel,
    );
    let mut phases = Vec::new();
    let mut finished = false;
    while let Ok(ev) = rx.recv_timeout(Duration::from_secs(20)) {
        match ev {
            BuildEvent::Phase(p) => phases.push(p),
            BuildEvent::Finished(o) => {
                assert!(o.success, "{o:?}");
                finished = true;
                break;
            }
            BuildEvent::Diagnostics(_) => {}
        }
    }
    assert!(finished, "the build never reported completion");
    assert_eq!(
        phases,
        [
            BuildPhase::Preflight,
            BuildPhase::Translating,
            BuildPhase::Compiling
        ]
    );
}

#[test]
fn the_compiler_is_given_only_the_pinned_flags_and_our_copy() {
    let f = Fixture::new(&[("A.BAS", SRC)]);
    f.set_mode("ok");
    assert!(f.build(&f.program(&["A.BAS"])).success);
    let log = f.argv_log();
    assert!(log.contains("-lang qb"), "{log}");
    assert!(log.contains("prog.bas"), "{log}");
    assert!(
        !log.contains("A.BAS"),
        "the compiler must only ever see our copy, never the user's file: {log}"
    );
}

#[test]
fn a_build_can_be_cancelled_while_the_compiler_is_running() {
    let f = Fixture::new(&[("SLOW.BAS", SRC)]);
    f.set_mode("slow");
    let cancel = Arc::new(AtomicBool::new(false));
    let rx = build::spawn(
        f.guard.clone(),
        Arc::new(f.toolchain()),
        f.layout.clone(),
        f.program(&["SLOW.BAS"]),
        build::BuildRequest::default(),
        Arc::clone(&cancel),
    );

    // Wait for the build to say it has started compiling, rather than sleeping
    // a fixed time and hoping: on a loaded machine the translation could
    // outlast the sleep, and the test would pass having exercised none of the
    // kill path.
    loop {
        match rx.recv_timeout(Duration::from_secs(20)) {
            Ok(BuildEvent::Phase(BuildPhase::Compiling)) => break,
            Ok(_) => {}
            Err(e) => panic!("the compiler never started: {e}"),
        }
    }
    // The phase is emitted just before the spawn, so give the child a moment to
    // exist before killing it.
    std::thread::sleep(Duration::from_millis(150));
    cancel.store(true, Ordering::SeqCst);

    let start = std::time::Instant::now();
    loop {
        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(BuildEvent::Finished(o)) => {
                assert!(o.cancelled, "{o:?}");
                break;
            }
            Ok(_) => {}
            Err(e) => panic!("cancellation did not take effect: {e}"),
        }
    }
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "cancelling should not wait for the 30s compiler"
    );
}

// ---------------------------------------------------- keeping the program

/// Build something that is actually executable, using the fake program as the
/// "compiler output", so the saved file can be run.
fn build_runnable(f: &Fixture) -> PathBuf {
    f.set_mode("interactive");
    f.set_program(FAKE_PROGRAM);
    let outcome = f.build(&f.program(&["PROG.BAS"]));
    assert!(outcome.success, "{outcome:?}");
    outcome.exe.unwrap()
}

#[test]
fn the_built_program_can_be_saved_where_the_user_chooses_and_still_runs() {
    // The build tree is scratch space that gets cleaned up, so saving is how the
    // user ends up with a program to keep. It has to still be a working program
    // on the other side of the copy.
    let f = Fixture::new(&[("PROG.BAS", SRC)]);
    let exe = build_runnable(&f);

    // A name with diacritics and a space, because that is what the user will
    // type, and the export is what has to survive it.
    let dest = f
        .user_files
        .parent()
        .unwrap()
        .join(format!("Tính cọc{}", std::env::consts::EXE_SUFFIX));
    f.guard.export_built_program(&exe, &dest).unwrap();
    assert!(dest.exists(), "saved to {}", dest.display());

    let out = etb_testkit::spawn_tolerating_busy(
        std::process::Command::new(&dest)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped()),
    )
    .expect("the saved program must be executable")
    .wait_with_output()
    .expect("waiting for the saved program");
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        text.contains("Nhap so N:"),
        "saved program printed: {text:?}"
    );
}

#[test]
fn saving_never_writes_over_one_of_the_users_source_files() {
    let f = Fixture::new(&[("PROG.BAS", SRC)]);
    let exe = build_runnable(&f);
    let source = f.user_files.join("PROG.BAS");
    let before = std::fs::read(&source).unwrap();

    assert!(
        f.guard.export_built_program(&exe, &source).is_err(),
        "a save dialog should never produce this, but it must be refused if it does"
    );
    assert_eq!(std::fs::read(&source).unwrap(), before);
}

#[test]
fn saving_refuses_anything_we_did_not_build() {
    // Export writes outside our own tree, so its source must be something we
    // produced -- never one of the user's files copied somewhere else.
    let f = Fixture::new(&[("PROG.BAS", SRC)]);
    let source = f.user_files.join("PROG.BAS");
    let dest = f.user_files.parent().unwrap().join("copy.bin");
    assert!(f.guard.export_built_program(&source, &dest).is_err());
    assert!(!dest.exists());
}

// ---------------------------------------------------------------- toolchain

#[test]
fn the_toolchain_reports_its_version() {
    let tc = Toolchain::from_path(PathBuf::from(FAKE_FBC), ToolchainKind::System).unwrap();
    assert_eq!(tc.id().version, "1.10.1");
    assert_eq!(tc.id().display(), "FreeBASIC 1.10.1");
}

// ---------------------------------------------------- a compiler it can be given

/// On Windows a compiler is handed every path through the ANSI code page, which
/// has no Vietnamese: installed under `C:\Users\Nguyễn Văn A\…` it is given
/// `Nguy?n Van A` and cannot find its own files (`docs/verification.md`, F3).
/// So the build copies it somewhere it can be named, once, and says so while
/// it does.
#[test]
fn a_compiler_at_a_path_it_cannot_be_given_is_copied_first() {
    let f = Fixture::new(&[("A.BAS", SRC)]);
    f.set_mode("ok");

    let home = f.paths.data_dir().join("Nguyễn Văn A").join("FreeBASIC");
    std::fs::create_dir_all(&home).unwrap();
    let installed = home.join(PathBuf::from(FAKE_FBC).file_name().unwrap());
    std::fs::copy(FAKE_FBC, &installed).unwrap();

    let tc = Toolchain::from_path(installed.clone(), ToolchainKind::UserSpecified).unwrap();
    assert!(!tc.usable_in_place(), "the test is not testing anything");

    let req = build::BuildRequest {
        prepare: Some(build::Preparation {
            tool_root: f.paths.tool_root(),
            lock_dir: f.paths.lock_dir(),
            manifest: Arc::new(Default::default()),
        }),
        ..Default::default()
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let rx = build::spawn(
        f.guard.clone(),
        Arc::new(tc),
        f.layout.clone(),
        f.program(&["A.BAS"]),
        req,
        cancel,
    );
    let mut phases = Vec::new();
    let mut outcome = None;
    while let Ok(ev) = rx.recv_timeout(Duration::from_secs(60)) {
        match ev {
            BuildEvent::Phase(p) => phases.push(p),
            BuildEvent::Finished(o) => {
                outcome = Some(o);
                break;
            }
            BuildEvent::Diagnostics(_) => {}
        }
    }
    let outcome = outcome.expect("the build never reported completion");
    assert!(outcome.success, "{outcome:?}");
    assert_eq!(
        phases,
        [
            BuildPhase::Preflight,
            BuildPhase::Preparing,
            BuildPhase::Translating,
            BuildPhase::Compiling
        ]
    );

    // The copy is under our own ASCII directory, and the installation it was
    // made from is still there, untouched.
    let copies = hash_tree(&f.paths.tool_root());
    assert!(
        copies
            .iter()
            .any(|(p, _)| p.to_string_lossy().contains("fake_fbc")),
        "no copy of the compiler was made: {copies:?}"
    );
    assert!(installed.is_file());
}

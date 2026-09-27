//! The build state machine.
//!
//! One program, one pass: read and check the user's files, translate them from
//! Turbo Basic into what FreeBASIC's `-lang qb` accepts, and have `fbc` turn
//! that into a program.
//!
//! Everything the translator can find is found before the compiler runs and
//! reported together. Not because the compiler stops at the first error — `fbc`
//! reports as many as it can — but because what it reports is about the
//! *translated* copy. A Turbo Basic construct this application refuses would
//! otherwise reach the user as a compiler message about something they never
//! wrote.

pub mod diagnostics;
pub mod exec;
pub mod fbargs;
pub mod sourcefmt;
pub mod stage;

use crate::error::{EtbError, Result};
use crate::fs_guard::FsGuard;
use crate::paths::WorkLayout;
use crate::project::Program;
use crate::toolchain::Toolchain;
use crate::translate::{Finding, Severity as FindingSeverity, TestMode, TranslateOptions};
use crossbeam_channel::Sender;
use diagnostics::Diagnostic;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A runaway error cascade can emit hundreds of megabytes.
pub const COMPILER_OUTPUT_CAP: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildPhase {
    Preflight,
    /// Making the compiler usable: see `Toolchain::prepare`. Only the first
    /// build after an installation or an upgrade spends any time here.
    Preparing,
    /// Reading the user's files and translating them.
    Translating,
    /// `fbc` at work: on x86-64 its own translation to C, then GCC, then the
    /// assembler and linker it drives.
    Compiling,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailedAt {
    Preflight,
    Translate,
    Compile,
}

#[derive(Debug, Clone)]
pub struct BuildOutcome {
    pub success: bool,
    pub exe: Option<PathBuf>,
    pub diagnostics: Vec<Diagnostic>,
    /// What the translator had to say. Warnings travel with a successful build
    /// too: it built, but something will not behave as it did.
    pub findings: Vec<Finding>,
    /// Everything the compiler printed, for the technical-details pane.
    pub raw: String,
    pub errors: usize,
    pub warnings: usize,
    pub failed_at: Option<FailedAt>,
    pub cancelled: bool,
    /// Set when the failure was ours rather than the user's code's.
    pub internal_error: Option<String>,
    /// Set when the build stopped because one of the user's files cannot be used.
    ///
    /// Kept as structured data rather than folded into `internal_error`, because
    /// the whole value of checking a file ourselves is being able to explain it
    /// in the user's own language — and a formatted English string cannot be
    /// translated after the fact.
    pub file_problem: Option<FileProblem>,
}

pub use crate::error::{FileProblem, ProblemArg};

impl BuildOutcome {
    fn failure(at: FailedAt, diagnostics: Vec<Diagnostic>, raw: String) -> Self {
        let errors = diagnostics::count_errors(&diagnostics);
        let warnings = diagnostics::count_warnings(&diagnostics);
        Self {
            success: false,
            exe: None,
            diagnostics,
            findings: Vec::new(),
            raw,
            errors,
            warnings,
            failed_at: Some(at),
            cancelled: false,
            internal_error: None,
            file_problem: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum BuildEvent {
    Phase(BuildPhase),
    Diagnostics(Vec<Diagnostic>),
    Finished(Box<BuildOutcome>),
}

fn emit(tx: Option<&Sender<BuildEvent>>, ev: BuildEvent) {
    if let Some(tx) = tx {
        let _ = tx.send(ev);
    }
}

/// How this build is to be made. Only tests change `test_mode` from the
/// default.
#[derive(Debug, Clone, Default)]
pub struct BuildRequest {
    pub test_mode: Option<TestMode>,
    /// Where a compiler that cannot be run where it is installed may be copied
    /// to, and what the copy is stamped with. Without it the compiler is used
    /// where it is — which is all a compiler at a plain path ever needs, and
    /// what the tests that supply their own compiler want.
    pub prepare: Option<Preparation>,
}

/// What `Toolchain::prepare` needs, carried through the build so the window
/// can say what is happening while it runs.
#[derive(Debug, Clone)]
pub struct Preparation {
    /// An ASCII directory of ours: `AppPaths::tool_root`.
    pub tool_root: PathBuf,
    /// `AppPaths::lock_dir`, so two applications do not copy at once.
    pub lock_dir: PathBuf,
    pub manifest: Arc<crate::toolchain::manifest::Manifest>,
}

/// Build one program. Blocking; callers run it on a worker thread.
pub fn build(
    guard: &FsGuard,
    toolchain: &Toolchain,
    layout: &WorkLayout,
    program: &Program,
    tx: Option<&Sender<BuildEvent>>,
    cancel: &AtomicBool,
) -> BuildOutcome {
    build_with(
        guard,
        toolchain,
        layout,
        program,
        &BuildRequest::default(),
        tx,
        cancel,
    )
}

pub fn build_with(
    guard: &FsGuard,
    toolchain: &Toolchain,
    layout: &WorkLayout,
    program: &Program,
    req: &BuildRequest,
    tx: Option<&Sender<BuildEvent>>,
    cancel: &AtomicBool,
) -> BuildOutcome {
    match build_inner(guard, toolchain, layout, program, req, tx, cancel) {
        Ok(outcome) => {
            emit(tx, BuildEvent::Finished(Box::new(outcome.clone())));
            outcome
        }
        Err(e) => {
            let mut o = BuildOutcome::failure(FailedAt::Preflight, Vec::new(), String::new());
            if let EtbError::UnusableFile { problem, .. } = &e {
                o.file_problem = Some((**problem).clone());
            }
            o.internal_error = Some(e.to_string());
            emit(tx, BuildEvent::Finished(Box::new(o.clone())));
            o
        }
    }
}

fn build_inner(
    guard: &FsGuard,
    toolchain: &Toolchain,
    layout: &WorkLayout,
    program: &Program,
    req: &BuildRequest,
    tx: Option<&Sender<BuildEvent>>,
    cancel: &AtomicBool,
) -> Result<BuildOutcome> {
    emit(tx, BuildEvent::Phase(BuildPhase::Preflight));
    if program.sources.is_empty() {
        return Err(EtbError::NoSources);
    }
    // Every file must still be there. The config is not a source of truth about
    // what exists on disk.
    for s in &program.sources {
        if !s.path.exists() {
            return Err(EtbError::io(
                &s.path,
                std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"),
            ));
        }
    }

    // The compiler as it can actually be run. Usually itself; a copy of it
    // when its own path is one it cannot be given (a Vietnamese user name, on
    // Windows). Made before anything is read, so a cancel here costs nothing.
    let prepared;
    let toolchain = match &req.prepare {
        Some(p) if !toolchain.usable_in_place() => {
            emit(tx, BuildEvent::Phase(BuildPhase::Preparing));
            match toolchain.prepare(guard, &p.tool_root, &p.lock_dir, &p.manifest, cancel)? {
                Some(tc) => {
                    prepared = tc;
                    &prepared
                }
                None => return Ok(cancelled(Vec::new(), String::new(), Vec::new())),
            }
        }
        _ => toolchain,
    };

    emit(tx, BuildEvent::Phase(BuildPhase::Translating));
    let opts = TranslateOptions {
        keep_window_open: program.options.keep_window_open,
        test_mode: req.test_mode,
        ..Default::default()
    };
    let staging = stage::stage(guard, layout, program, &opts)?;
    let findings = staging.translation.findings.clone();
    if staging.translation.refused() {
        let mut o = BuildOutcome::failure(FailedAt::Translate, Vec::new(), String::new());
        o.findings = findings;
        return Ok(o);
    }
    if cancel.load(Ordering::Relaxed) {
        return Ok(cancelled(Vec::new(), String::new(), findings));
    }

    emit(tx, BuildEvent::Phase(BuildPhase::Compiling));
    // No lock here. QB64-PE wrote its intermediate C++ inside its own
    // directory, in the same place for every program, so two builds at once
    // could swap programs (`docs/verification.md`, F8); `fbc` writes only
    // where it is told, which `cargo test -p etb-testkit --test mutable`
    // measures. The lock is still taken while a copy of the compiler is made
    // — see `Toolchain::prepare` — because that is a whole tree appearing.
    let exe = layout.exe_with_suffix(toolchain.exe_suffix());
    let mut cmd = toolchain.command(layout);
    cmd.args(fbargs::compile_args(&staging.main, &staging.prelude, &exe));
    let (code, raw) = exec::run_capture(cmd, COMPILER_OUTPUT_CAP, cancel)?;
    // Stop pressed while the compiler was working. The killed process's non-zero exit
    // is ours to interpret, not a failure to report.
    if cancel.load(Ordering::Relaxed) {
        return Ok(cancelled(Vec::new(), raw, findings));
    }

    let mut diags = diagnostics::parse(&raw);
    diagnostics::remap(
        &mut diags,
        &staging.translation.map,
        &staging.translation.main().name,
        &|file, line| staging.user_line(file, line),
    );
    if !diags.is_empty() {
        emit(tx, BuildEvent::Diagnostics(diags.clone()));
    }

    if code != Some(0) || !exe.exists() {
        let mut o = BuildOutcome::failure(FailedAt::Compile, diags, raw);
        o.findings = findings;
        // The compiler failed and said nothing we could read, or said something
        // about what we generated. Either way the user's code is not the one to
        // blame, and the message should not suggest it is.
        if o.diagnostics.is_empty() || o.diagnostics.iter().any(|d| d.ours) {
            o.internal_error = Some(
                "the compiler could not build the translated program; the details are below".into(),
            );
        }
        return Ok(o);
    }

    let errors = diagnostics::count_errors(&diags);
    let warnings = diagnostics::count_warnings(&diags)
        + findings
            .iter()
            .filter(|f| f.severity == FindingSeverity::Warn)
            .count();
    Ok(BuildOutcome {
        success: true,
        exe: Some(exe),
        diagnostics: diags,
        findings,
        raw,
        errors,
        warnings,
        failed_at: None,
        cancelled: false,
        internal_error: None,
        file_problem: None,
    })
}

fn cancelled(diagnostics: Vec<Diagnostic>, raw: String, findings: Vec<Finding>) -> BuildOutcome {
    BuildOutcome {
        success: false,
        exe: None,
        errors: diagnostics::count_errors(&diagnostics),
        warnings: diagnostics::count_warnings(&diagnostics),
        diagnostics,
        findings,
        raw,
        failed_at: None,
        cancelled: true,
        internal_error: None,
        file_problem: None,
    }
}

/// Run a build on a worker thread, returning the event stream immediately.
pub fn spawn(
    guard: FsGuard,
    toolchain: Arc<Toolchain>,
    layout: WorkLayout,
    program: Program,
    req: BuildRequest,
    cancel: Arc<AtomicBool>,
) -> crossbeam_channel::Receiver<BuildEvent> {
    let (tx, rx) = crossbeam_channel::unbounded();
    std::thread::spawn(move || {
        build_with(
            &guard,
            &toolchain,
            &layout,
            &program,
            &req,
            Some(&tx),
            &cancel,
        );
    });
    rx
}

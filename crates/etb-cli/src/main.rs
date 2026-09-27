//! Headless driver.
//!
//! Two jobs: it is how CI exercises the whole build pipeline without a window
//! server, and it is how you diagnose an installation over the phone
//! ("run this one command and read me what it says").

use anyhow::{bail, Context, Result};
use etb_core::build::{self, diagnostics::Severity, BuildRequest};
use etb_core::config::Store;
use etb_core::fs_guard::{FsGuard, Scratch};
use etb_core::i18n::{self, Lang};
use etb_core::paths::{AppPaths, WorkLayout};
use etb_core::project::Program;
use etb_core::toolchain::manifest::Manifest;
use etb_core::toolchain::{self, Toolchain};
use etb_core::translate::{Finding, TestMode};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

const USAGE: &str = "\
Easy Turbo Basic — headless driver

USAGE:
    etb-cli doctor
    etb-cli build <program.bas> [<included file>...] [--out <path>] [--json]
                  [--no-keep-open]
    etb-cli translate <program.bas> [--file <staged name>] [--map]

OPTIONS:
    --out P          save the built program to P
    --json           machine-readable result on stdout
    --no-keep-open   let the program's window close without waiting for a key
    --file NAME      (translate) print this staged file instead of the program
    --map            (translate) print where each staged line came from
";

struct Opts {
    files: Vec<PathBuf>,
    json: bool,
    out: Option<PathBuf>,
    no_keep_open: bool,
    /// For tests only, and deliberately missing from the usage text.
    test_mode: Option<TestMode>,
}

fn main() {
    if let Err(e) = real_main() {
        eprintln!("error: {e:#}");
        std::process::exit(2);
    }
}

fn real_main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let Some(cmd) = args.next() else {
        print!("{USAGE}");
        std::process::exit(1);
    };
    let rest: Vec<String> = args.collect();

    match cmd.as_str() {
        "doctor" => doctor(),
        "translate" => translate(&rest),
        "build" => {
            let o = parse(&rest)?;
            let (paths, outcome) = do_build(&o)?;
            report_build(&o, &outcome);

            let guard = FsGuard::new(paths.write_roots().to_vec())?;
            if let (Some(dest), Some(built)) = (&o.out, &outcome.exe) {
                guard
                    .export_built_program(built, dest)
                    .with_context(|| format!("saving to {}", dest.display()))?;
                if !o.json {
                    println!("saved: {}", dest.display());
                }
            }

            // Clear up, after the export has taken what it needs out of the
            // tree, and sweep what earlier runs left: a CLI that leaves a
            // build tree behind on every run collects them by the dozen.
            let _ = guard.remove_dir_all(&paths.session_work_dir());
            guard.sweep_stale_sessions(
                &paths.work_root(),
                paths.session_id(),
                std::time::Duration::from_secs(24 * 60 * 60),
            );

            if !outcome.success {
                std::process::exit(1);
            }
            Ok(())
        }
        "-h" | "--help" | "help" => {
            print!("{USAGE}");
            Ok(())
        }
        other => bail!("unknown command `{other}`\n\n{USAGE}"),
    }
}

fn parse(args: &[String]) -> Result<Opts> {
    let mut o = Opts {
        files: Vec::new(),
        json: false,
        out: None,
        no_keep_open: false,
        test_mode: None,
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--json" => o.json = true,
            "--no-keep-open" => o.no_keep_open = true,
            "--out" => o.out = Some(PathBuf::from(it.next().context("--out needs a path")?)),
            "--test-mode" => o.test_mode = Some(test_mode(it.next())?),
            other if other.starts_with("--") => bail!("unknown option `{other}`"),
            other => o.files.push(PathBuf::from(other)),
        }
    }
    if o.files.is_empty() {
        bail!("no program given\n\n{USAGE}");
    }
    Ok(o)
}

fn test_mode(arg: Option<&String>) -> Result<TestMode> {
    match arg.map(String::as_str) {
        Some("console") => Ok(TestMode::Console),
        Some("window") => Ok(TestMode::Window),
        other => bail!("unknown test mode {other:?}"),
    }
}

/// Print what the compiler would be given for a Turbo Basic program: the translated
/// main file by default, any other staged file with `--file`, and with `--map`
/// the line map, so a support call can see exactly where a line went.
fn translate(args: &[String]) -> Result<()> {
    use etb_core::translate::{translate_with, Origin, SourceFile, TranslateOptions};
    use std::io::Write as _;

    let mut path: Option<PathBuf> = None;
    let mut file: Option<String> = None;
    let mut map = false;
    let mut opts = TranslateOptions::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--map" => map = true,
            "--file" => file = Some(it.next().context("--file needs a name")?.clone()),
            "--test-mode" => opts.test_mode = Some(test_mode(it.next())?),
            s if s.starts_with("--") => bail!("unknown option `{s}`"),
            s => path = Some(PathBuf::from(s)),
        }
    }
    let path = path.context("which .BAS file?")?;
    let bytes = FsGuard::read_user_source(&path)?;
    let display_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let main = SourceFile {
        display_name,
        bytes,
    };
    // Includes are found the way a build finds them: beside the program.
    let listed = vec![(path.clone(), main.clone())];
    let dirs: Vec<PathBuf> = path.parent().map(|d| d.to_path_buf()).into_iter().collect();
    let t = translate_with(
        &main,
        &mut |name| etb_core::build::stage::find_include(name, &listed, &dirs),
        &opts,
    );

    let mut out = std::io::stdout().lock();
    if map {
        for staged in &t.map.staged {
            for (i, o) in staged.lines.iter().enumerate() {
                let from = match o {
                    Origin::User { file, line } => {
                        format!("{} line {line}", t.map.source_name(*file).unwrap_or("?"))
                    }
                    Origin::Injected => "added".into(),
                    Origin::Prelude { line } => format!("runtime support line {line}"),
                };
                writeln!(out, "{} {}: {from}", staged.staged, i + 1)?;
            }
        }
    } else {
        let name = file.as_deref().unwrap_or(etb_core::translate::STAGED_MAIN);
        let staged = t
            .files
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(name))
            .with_context(|| format!("no staged file called {name}"))?;
        out.write_all(&staged.bytes)?;
    }
    for f in &t.findings {
        eprintln!("{}", finding_line(f));
    }
    if t.refused() {
        std::process::exit(1);
    }
    Ok(())
}

/// A finding, in English, the way it would read in the window.
fn finding_line(f: &Finding) -> String {
    let args: Vec<(&str, String)> = f
        .args
        .iter()
        .map(|(k, v)| {
            let v = match v {
                etb_core::error::ProblemArg::Text(t) => t.clone(),
                etb_core::error::ProblemArg::Key(k) => i18n::lookup(Lang::En, k),
            };
            (*k, v)
        })
        .collect();
    let args: Vec<(&str, &str)> = args.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let text = i18n::format(Lang::En, f.key, &args);
    match f.line {
        Some(l) => format!("{}:{l}: {:?}: {text} [{}]", f.file, f.severity, f.key),
        None => format!("{}: {:?}: {text} [{}]", f.file, f.severity, f.key),
    }
}

fn find_toolchain() -> Result<Toolchain> {
    let store_override = AppPaths::resolve()
        .ok()
        .and_then(|p| Store::new(p).ok())
        .and_then(|mut s| s.load_settings().ok())
        .and_then(|(s, _note)| s.toolchain_override);
    toolchain::discover(store_override.as_deref()).map_err(|e| match e {
        // Nothing shipped and nothing installed: the hint is the useful part.
        etb_core::error::EtbError::ToolchainMissing => anyhow::Error::new(e).context(
            "no FreeBASIC found (run `cargo xtask fetch-toolchain`, or set ETB_TOOLCHAIN_BUNDLE)",
        ),
        // A bundle is present and will not load. Saying "install one" would send
        // someone to fix the wrong thing -- and installing one is precisely what
        // must not happen, because then the next build would silently use it.
        other => anyhow::Error::new(other),
    })
}

fn program_from(o: &Opts) -> Result<Program> {
    let mut p = Program::new("cli");
    for f in &o.files {
        p.add_source(f)
            .with_context(|| format!("adding {}", f.display()))?;
    }
    if o.no_keep_open {
        p.options.keep_window_open = false;
    }
    Ok(p)
}

fn do_build(o: &Opts) -> Result<(AppPaths, build::BuildOutcome)> {
    let paths = AppPaths::resolve()?;
    let guard = FsGuard::new(paths.write_roots().to_vec())?;
    let tc = find_toolchain()?;

    let program = program_from(o)?;

    // The same check the window makes before its first build, and for the same
    // reason: a compiler that cannot be shown to be the one we shipped is not
    // used at all.
    let report = tc.verify_integrity(&Manifest::parse(TOOLCHAIN_MANIFEST));
    if !report.is_ok() {
        bail!(
            "the bundled compiler failed its integrity check, so it was not used\n{}",
            report.detail()
        );
    }

    let layout = WorkLayout::new(paths.build_dir(1));
    let cancel = AtomicBool::new(false);
    let req = BuildRequest {
        test_mode: o.test_mode,
        prepare: Some(build::Preparation {
            tool_root: paths.tool_root(),
            lock_dir: paths.lock_dir(),
            manifest: Arc::new(Manifest::parse(TOOLCHAIN_MANIFEST)),
        }),
    };
    let outcome = build::build_with(&guard, &tc, &layout, &program, &req, None, &cancel);
    Ok((paths, outcome))
}

fn severity(s: Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Note => "note",
    }
}

fn json_opt<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map(|v| v.to_string()).unwrap_or_else(|| "null".into())
}

fn report_build(o: &Opts, outcome: &build::BuildOutcome) {
    if o.json {
        let mut s = String::from("{\n");
        s.push_str(&format!("  \"success\": {},\n", outcome.success));
        s.push_str(&format!("  \"errors\": {},\n", outcome.errors));
        s.push_str(&format!("  \"warnings\": {},\n", outcome.warnings));
        s.push_str(&format!(
            "  \"exe\": {},\n",
            match &outcome.exe {
                Some(p) => format!("{:?}", p.display().to_string()),
                None => "null".into(),
            }
        ));
        if let Some(ie) = &outcome.internal_error {
            s.push_str(&format!("  \"internal_error\": {:?},\n", ie));
        }
        if let Some(fp) = &outcome.file_problem {
            s.push_str(&format!(
                "  \"file_problem\": {{\"file\": {:?}, \"key\": {:?}}},\n",
                fp.name, fp.reason_key
            ));
        }
        s.push_str("  \"findings\": [\n");
        for (i, f) in outcome.findings.iter().enumerate() {
            s.push_str(&format!(
                "    {{\"severity\": {:?}, \"key\": {:?}, \"file\": {:?}, \"line\": {}}}{}\n",
                format!("{:?}", f.severity).to_lowercase(),
                f.key,
                f.file,
                json_opt(f.line),
                if i + 1 == outcome.findings.len() {
                    ""
                } else {
                    ","
                }
            ));
        }
        s.push_str("  ],\n  \"diagnostics\": [\n");
        for (i, d) in outcome.diagnostics.iter().enumerate() {
            s.push_str(&format!(
                "    {{\"severity\": {:?}, \"file\": {}, \"line\": {}, \"ours\": {}, \"message\": {:?}}}{}\n",
                severity(d.severity),
                d.file.as_ref().map(|f| format!("{f:?}")).unwrap_or_else(|| "null".into()),
                json_opt(d.line),
                d.ours,
                d.message,
                if i + 1 == outcome.diagnostics.len() { "" } else { "," }
            ));
        }
        s.push_str("  ]\n}\n");
        print!("{s}");
        return;
    }

    for f in &outcome.findings {
        println!("{}", finding_line(f));
    }
    for d in &outcome.diagnostics {
        let who = if d.ours { " (ours)" } else { "" };
        match (&d.file, d.line) {
            (Some(f), Some(l)) => println!("{f}:{l}: {}{who}: {}", severity(d.severity), d.message),
            (None, Some(l)) => println!("line {l}: {}{who}: {}", severity(d.severity), d.message),
            _ => println!("{}{who}: {}", severity(d.severity), d.message),
        }
        for s in &d.snippet {
            println!("    {s}");
        }
    }
    if let Some(fp) = &outcome.file_problem {
        println!("{}: {}", fp.name, i18n::lookup(Lang::En, fp.reason_key));
    }
    if let Some(ie) = &outcome.internal_error {
        println!("internal error: {ie}");
    }
    if !outcome.success && outcome.diagnostics.is_empty() && !outcome.raw.is_empty() {
        println!("--- compiler output ---\n{}", outcome.raw.trim_end());
    }
    println!(
        "{}: {} errors, {} warnings",
        if outcome.success { "ok" } else { "FAILED" },
        outcome.errors,
        outcome.warnings
    );
}

/// The same manifest the application embeds, so `doctor` reports what the
/// application would actually do.
const TOOLCHAIN_MANIFEST: &str = include_str!("../../etb-gui/assets/toolchain-manifest.txt");

fn doctor() -> Result<()> {
    let paths = AppPaths::resolve()?;
    println!("config dir : {}", paths.config_dir().display());
    println!("data dir   : {}", paths.data_dir().display());
    println!("work root  : {}", paths.work_root().display());

    let tc = match find_toolchain() {
        Ok(tc) => tc,
        Err(e) => {
            println!("compiler   : NOT FOUND ({e:#})");
            return Ok(());
        }
    };
    println!("compiler   : {}", tc.fbc().display());
    println!("version    : {}", tc.id().display());
    println!("kind       : {:?}", tc.id().kind);
    if let Some(l) = tc.launcher() {
        println!("launcher   : {l}");
    }

    let manifest = Manifest::parse(TOOLCHAIN_MANIFEST);
    let report = tc.verify_integrity(&manifest);
    if manifest.is_empty() {
        println!("integrity  : no manifest embedded (development build)");
    } else if report.is_ok() {
        println!("integrity  : ok ({} files verified)", report.checked);
    } else {
        println!(
            "integrity  : FAILED — {} missing, {} changed, {} unreadable",
            report.missing.len(),
            report.changed.len(),
            report.unreadable.len()
        );
        print!("{}", report.detail());
    }

    // Whether it actually runs: fbc's own pass over a one-line program,
    // stopping before the assembler. A compiler that is present, attested and
    // cannot start is the case this line exists for.
    //
    // Through the copy, where there is one, because that is the compiler a
    // build would use and so the one worth smoke-testing.
    let cancel = AtomicBool::new(false);
    let guard = FsGuard::new(paths.write_roots().to_vec())?;
    let tc = match tc.prepare(
        &guard,
        &paths.tool_root(),
        &paths.lock_dir(),
        &manifest,
        &cancel,
    )? {
        Some(tc) => tc,
        None => return Ok(()),
    };
    if let Some(origin) = tc.origin() {
        println!("installed  : {}", origin.display());
        println!(
            "run from   : {}  (the installed path cannot be given to it)",
            tc.fbc().display()
        );
    }
    let scratch = Scratch::new("etb-doctor")?;
    let src = scratch.write("check.bas", b"PRINT 1\r\n")?;
    let mut cmd = tc.command(&WorkLayout::new(scratch.path().to_path_buf()));
    cmd.args(build::fbargs::check_args(&src));
    match build::exec::run_capture(cmd, 64 * 1024, &cancel) {
        Ok((Some(0), _)) => println!("smoke test : ok"),
        Ok((code, text)) => {
            println!("smoke test : FAILED (exit {code:?})");
            print!("{text}");
        }
        Err(e) => println!("smoke test : FAILED ({e})"),
    }
    Ok(())
}

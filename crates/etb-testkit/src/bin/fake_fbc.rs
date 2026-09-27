//! A stand-in for QB64-PE.
//!
//! This is what lets the whole build state machine — translation, staging,
//! error mapping, output caps, cancellation, saving — be tested on a machine
//! with no QB64 installed.
//!
//! QB64-PE runs in its own directory, which for this fake is shared by every
//! test, so its instructions cannot live there. They live beside the build
//! tree it is handed: the staged program is `<build>/src/prog.bas`, and the
//! markers are read from `<build>/`. Tests stay parallel-safe that way.
//!
//! Markers:
//! - `FAKE_MODE`: `ok` (default), `syntax`, `include_err`, `cpp_fail`,
//!   `flood`, `slow`, `interactive`.
//! - `FAKE_PROGRAM`: for `interactive`, the executable to copy into place as
//!   the built program, so a saved program can be run.
//!
//! In `syntax` mode the error is reported on the first staged line containing
//! `ERROR_HERE`, in exactly the shape QB64-PE 4.6 prints with `-q -m`.

use std::io::Write;
use std::path::{Path, PathBuf};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "--version") {
        println!(
            "FreeBASIC Compiler - Version 1.10.1 (2023-12-24), built for linux-x86_64 (64bit)"
        );
        return;
    }

    // The program, not the runtime support: that arrives as the value of
    // `-include` and is a `.bas` too.
    let included = arg_after(&args, "-include");
    let Some(src) = args
        .iter()
        .filter(|a| Some(a.as_str()) != included.as_deref())
        .find(|a| a.to_ascii_lowercase().ends_with(".bas"))
    else {
        eprintln!("fake fbc: no .bas file given");
        std::process::exit(2);
    };
    let src = PathBuf::from(src);
    let build_root = src
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_default();

    // Record every invocation, so a test can assert exactly which flags
    // reached the compiler.
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(build_root.join("argv.log"))
    {
        let _ = writeln!(f, "{}", args.join(" "));
    }

    // `-c`: compile only, no program to produce.
    if args.iter().any(|a| a == "-c") {
        return;
    }
    let out = arg_after(&args, "-x");
    let mode = read_marker(&build_root, "FAKE_MODE").unwrap_or_else(|| "ok".into());

    match mode.as_str() {
        "syntax" => {
            let text = std::fs::read(&src).unwrap_or_default();
            let text = String::from_utf8_lossy(&text);
            let (n, line) = text
                .lines()
                .enumerate()
                .find(|(_, l)| l.contains("ERROR_HERE"))
                .map(|(i, l)| (i + 1, l.to_string()))
                .unwrap_or((1, String::new()));
            let name = src
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            print!("{name}({n}) error 10: Expected '='\n{line}\n^\n");
            std::process::exit(1);
        }
        "include_err" => {
            print!("etb_prelude.bas(3) error 14: Type mismatch\nX = \"\"\n^\n");
            std::process::exit(1);
        }
        "cpp_fail" => {
            // What a wrongly packaged toolchain looks like: the compiler runs,
            // the linker cannot find what it needs. Never the user's fault.
            println!("/usr/bin/ld.bfd: cannot find -lXext: No such file or directory");
            std::process::exit(1);
        }
        "flood" => {
            let line = "prog.bas(1) error 10: Expected '=' ".repeat(20);
            let mut stdout = std::io::stdout().lock();
            for _ in 0..200_000 {
                let _ = writeln!(stdout, "{line}");
            }
            std::process::exit(1);
        }
        "slow" => {
            std::thread::sleep(std::time::Duration::from_secs(30));
            touch(out.as_deref());
        }
        // Produce something that can actually be executed, so saving and
        // running a built program can be tested with no QB64 at all.
        "interactive" => {
            let program = read_marker(&build_root, "FAKE_PROGRAM")
                .expect("FAKE_PROGRAM marker must name the program");
            let out = out.expect("a build needs -o");
            std::fs::copy(&program, &out).expect("copy fake program into place");
            make_executable(Path::new(&out));
        }
        _ => touch(out.as_deref()),
    }
}

fn read_marker(dir: &Path, name: &str) -> Option<String> {
    std::fs::read_to_string(dir.join(name))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn arg_after(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn touch(path: Option<&str>) {
    if let Some(p) = path {
        let _ = std::fs::write(p, b"fake program\n");
    }
}

fn make_executable(p: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(md) = std::fs::metadata(p) {
            let mut perms = md.permissions();
            perms.set_mode(0o755);
            let _ = std::fs::set_permissions(p, perms);
        }
    }
    #[cfg(not(unix))]
    let _ = p;
}

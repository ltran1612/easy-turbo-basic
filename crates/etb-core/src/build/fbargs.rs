//! The only place a FreeBASIC command line is put together.
//!
//! Every flag here is pinned, and a test says so: a build must not depend on
//! what a compiler configuration file happens to say, or on a default that
//! changes between releases. `fbc` has no settings file of its own, which
//! removes a whole class of that problem — but the dialect does not default to
//! the one this application needs, and forgetting `-lang qb` would mean a
//! program that compiled yesterday failing today for reasons the user cannot
//! act on.

use std::ffi::OsString;
use std::path::Path;

/// What every invocation says, whatever it is for.
///
/// `-lang qb` is FreeBASIC's QuickBASIC dialect: implicit variables, `GOSUB`,
/// `$INCLUDE`, QuickBASIC's `SCREEN` graphics, and the numeric types of that
/// family. It is not Turbo Basic — `translate/` closes that gap — but it is
/// the closest the compiler has, and everything the translator emits is
/// written for it.
///
/// `-w error` is deliberately *not* here: a warning from the compiler about
/// the translated copy is ours to read, not the user's to be stopped by.
const PINNED: &[&str] = &["-lang", "qb"];

/// Compile `src` into the program `out`, with `prelude` compiled ahead of it.
///
/// `-include` is what keeps the promise this application is built on: the
/// runtime support is compiled before the program that calls it — FreeBASIC
/// wants a procedure declared before it is used — while the program itself
/// stays exactly as the translator wrote it, so line 7 of what the compiler
/// reads is line 7 of what the user wrote. Including it from inside the
/// program would cost a line, and every error after it would be off by one.
///
/// `-x` names the output. There is no separate link step to drive: fbc runs
/// the assembler and linker itself.
pub fn compile_args(src: &Path, prelude: &Path, out: &Path) -> Vec<OsString> {
    let mut v: Vec<OsString> = PINNED.iter().map(OsString::from).collect();
    v.push("-include".into());
    v.push(prelude.as_os_str().to_owned());
    v.push(src.as_os_str().to_owned());
    v.push("-x".into());
    v.push(out.as_os_str().to_owned());
    v
}

/// Check `src` without producing a program: parse and compile, stop before
/// the assembler. Fast, so it suits `doctor` and the quick tier of tests.
pub fn check_args(src: &Path) -> Vec<OsString> {
    let mut v: Vec<OsString> = PINNED.iter().map(OsString::from).collect();
    v.push("-c".into());
    v.push(src.as_os_str().to_owned());
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Everything a build is allowed to pass. A new flag has to be added here
    /// on purpose, next to the reason it is safe.
    const ALLOWED: &[&str] = &["-lang", "-x", "-c", "-include"];

    fn flags(v: &[OsString]) -> Vec<String> {
        v.iter()
            .map(|a| a.to_string_lossy().into_owned())
            .filter(|a| a.starts_with('-'))
            .collect()
    }

    #[test]
    fn only_known_flags_are_ever_passed() {
        let src = Path::new("/w/src/prog.bas");
        for args in [
            compile_args(
                src,
                Path::new("/w/src/etb_prelude.bas"),
                Path::new("/w/out/p.exe"),
            ),
            check_args(src),
        ] {
            for f in flags(&args) {
                assert!(ALLOWED.contains(&f.as_str()), "unexpected flag {f}");
            }
        }
    }

    #[test]
    fn the_dialect_is_always_named() {
        // Without it fbc compiles modern FreeBASIC, where a variable must be
        // declared before it is used and `GOSUB` does not exist — so every
        // program this application exists for would fail at once.
        for args in [
            compile_args(
                Path::new("/w/src/prog.bas"),
                Path::new("/w/src/etb_prelude.bas"),
                Path::new("/w/out/p"),
            ),
            check_args(Path::new("/w/src/prog.bas")),
        ] {
            let at = args.iter().position(|a| a == "-lang").expect("-lang");
            assert_eq!(args[at + 1], OsString::from("qb"));
        }
    }

    #[test]
    fn the_source_and_output_are_where_they_should_be() {
        let args = compile_args(
            Path::new("/w/src/prog.bas"),
            Path::new("/w/src/etb_prelude.bas"),
            Path::new("/w/out/program.exe"),
        );
        let n = args.len();
        assert_eq!(args[n - 5], OsString::from("-include"));
        assert_eq!(args[n - 4], OsString::from("/w/src/etb_prelude.bas"));
        assert_eq!(args[n - 3], OsString::from("/w/src/prog.bas"));
        assert_eq!(args[n - 2], OsString::from("-x"));
        assert_eq!(args[n - 1], OsString::from("/w/out/program.exe"));
    }
}

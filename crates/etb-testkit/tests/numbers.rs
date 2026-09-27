//! Tier 2: the numbers this application prints, against the numbers Turbo
//! Basic printed.
//!
//! `tools/tb-oracle/SOSANH.BAS` was run through the real TB 1.1 compiler in
//! DOSBox, and what it wrote is `tb-golden.txt`. The same program is built
//! here and run, and what it writes must be `fbc-golden.txt` — which differs
//! from Turbo Basic in a known, listed way and nowhere else.
//!
//! Why a golden file and not a list of assertions: a wrong number here is not
//! a crash. The program runs, the answer is simply not the one the user has
//! been reading for thirty years, and the only way to notice is to compare
//! every line.

use etb_core::build::{self, BuildRequest};
use etb_core::fs_guard::FsGuard;
use etb_core::paths::{AppPaths, WorkLayout};
use etb_core::project::{Program, SourceRef};
use etb_testkit::corpus::toolchain;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

fn oracle_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tools")
        .join("tb-oracle")
}

/// `N01 .3333333 ` → `("N01", ".3333333")`, keeping the spaces inside.
fn labelled(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter(|l| l.len() >= 3)
        .map(|l| {
            (
                l[..3].to_string(),
                l[3..].trim_end_matches('\r').to_string(),
            )
        })
        .collect()
}

fn read(name: &str) -> BTreeMap<String, String> {
    let path = oracle_dir().join(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    labelled(&String::from_utf8_lossy(&bytes))
}

/// Where this application's arithmetic is not Turbo Basic's, with the reason.
/// Everything else must agree exactly; adding a line here is a decision, not a
/// tidy-up.
const KNOWN: &[(&str, &str)] = &[
    ("N01", ""),
    ("N02", "FreeBASIC writes a nought before the point"),
    ("N03", "Turbo Basic puts a space after a number; FreeBASIC does not"),
    ("N04", "a SINGLE above 1E7: FreeBASIC goes exponential, Turbo Basic does not"),
    ("N05", "Turbo Basic puts a space after a number; FreeBASIC does not"),
    ("N06", "Turbo Basic prints anything below 0.1 in exponential form"),
    ("N07", "as N06, and a lower-case e with two exponent digits"),
    ("N10", "Turbo Basic puts a space after a number; FreeBASIC does not"),
    ("N11", "three exponent digits and a capital E, against two and a small one"),
    ("N12", "Turbo Basic puts a space after a number; FreeBASIC does not"),
    ("N13", "Turbo Basic puts a space after a number; FreeBASIC does not"),
    ("N14", "Turbo Basic puts a space after a number; FreeBASIC does not"),
    ("N15", "the last digit of a 16-digit double, and the nought before the point"),
    ("N16", "Turbo Basic puts a space after a number; FreeBASIC does not"),
    ("R06", "Turbo Basic puts a space after a number; FreeBASIC does not"),
    ("R07", "Turbo Basic puts a space after a number; FreeBASIC does not"),
    ("R08", "Turbo Basic puts a space after a number; FreeBASIC does not"),
    ("R09", "Turbo Basic puts a space after a number; FreeBASIC does not"),
    ("S03", "FreeBASIC writes a nought before the point"),
    ("S07", "Turbo Basic puts a space after a number; FreeBASIC does not"),
    ("S08", "Turbo Basic puts a space after a number; FreeBASIC does not"),
    ("T01", "Turbo Basic puts a space after a number; FreeBASIC does not"),
    ("T02", "Turbo Basic puts a space after a number; FreeBASIC does not"),
    ("T03", "Turbo Basic puts a space after a number; FreeBASIC does not"),
    ("T04", "Turbo Basic puts a space after a number; FreeBASIC does not"),
    ("T05", "Turbo Basic puts a space after a number; FreeBASIC does not"),
    ("T06", "FreeBASIC writes a nought before the point"),
    ("U09", "PRINT USING `^^^^`: Turbo Basic keeps the mantissa the format asks for; FreeBASIC normalises to one digit, losing a significant figure"),
    ("U14", "PRINT USING overflow `%`: Turbo Basic shows the number unformatted"),
    ("U15", "as U14"),
];
#[test]
fn the_numbers_are_what_they_were_measured_to_be() {
    let Some(tc) = toolchain() else { return };
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("SOSANH.BAS");
    std::fs::copy(oracle_dir().join("SOSANH.BAS"), &src).unwrap();

    let paths = AppPaths::under(tmp.path().join("app"));
    let guard = FsGuard::new(paths.write_roots().to_vec()).unwrap();
    let mut program = Program::new("numbers");
    program.sources.push(SourceRef::new(&src));
    program.options.keep_window_open = false;
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

    // A FreeBASIC program writes where it is run from, not beside itself —
    // QB64 changed directory at startup and this does not (docs/verification.md,
    // F2) — so it is started in its own folder, which is where a user
    // double-clicking it would start it too.
    let exe = outcome.exe.unwrap();
    let dir = exe.parent().unwrap().to_path_buf();
    let out = dir.join("OUT.TXT");
    let _ = std::fs::remove_file(&out);
    let status = std::process::Command::new(&exe)
        .current_dir(&dir)
        .stdin(std::process::Stdio::null())
        .status();
    assert!(status.is_ok(), "running {}: {status:?}", exe.display());
    let got = labelled(&String::from_utf8_lossy(
        &std::fs::read(&out).unwrap_or_else(|e| panic!("{}: {e}", out.display())),
    ));

    let want = read("fbc-golden.txt");
    let tb = read("tb-golden.txt");
    assert_eq!(got.len(), want.len(), "a line went missing or appeared");

    let mut changed = Vec::new();
    for (k, v) in &want {
        if got.get(k) != Some(v) {
            changed.push(format!("{k}: was {v:?}, now {:?}", got.get(k)));
        }
    }
    assert!(
        changed.is_empty(),
        "what this application prints has changed:\n  {}\n\nIf that is an \
         improvement, check it against Turbo Basic's own output in \
         tools/tb-oracle/tb-golden.txt and update fbc-golden.txt.",
        changed.join("\n  ")
    );

    // And the list of differences from Turbo Basic is exactly the one written
    // down: no new one has crept in, and none of the listed ones has quietly
    // been fixed without being taken off the list.
    let differs: Vec<&str> = tb
        .iter()
        .filter(|(k, v)| got.get(*k) != Some(*v))
        .map(|(k, _)| k.as_str())
        .collect();
    let known: Vec<&str> = KNOWN.iter().map(|(k, _)| *k).collect();
    assert_eq!(
        differs, known,
        "the differences from Turbo Basic are not the ones listed in this test"
    );
}

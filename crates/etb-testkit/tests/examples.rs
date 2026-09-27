//! The example programs shipped with the application do what their guide,
//! `examples/DOC-TRUOC.txt`, says they do.
//!
//! They are the first thing someone new presses the button on, so a broken
//! one is the first impression. Each is built with the real QB64-PE and, where
//! it runs without a screen, run with the answers the guide tells the user to
//! type.

use etb_core::fs_guard::FsGuard;
use etb_core::paths::AppPaths;
use etb_testkit::corpus::{run_case, toolchain, Case, Expect};
use std::path::{Path, PathBuf};

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
}

#[test]
fn the_guide_names_every_example() {
    let dir = examples_dir();
    let guide = std::fs::read_to_string(dir.join("DOC-TRUOC.txt")).unwrap();
    for e in std::fs::read_dir(&dir).unwrap().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name == "DOC-TRUOC.txt" {
            continue;
        }
        assert!(
            guide.contains(&name),
            "DOC-TRUOC.txt does not mention {name}"
        );
    }
}

fn case(name: &str, sources: &[&str], expect: Expect) -> Case {
    Case {
        name: name.into(),
        expect: Expect {
            description: name.into(),
            sources: sources.iter().map(|s| s.to_string()).collect(),
            ..expect
        },
        dir: examples_dir(),
    }
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn every_example_does_what_its_guide_says() {
    let Some(tc) = toolchain() else { return };
    let cases = [
        case(
            "01",
            &["01-CO-BAN.BAS"],
            Expect {
                run: true,
                stdin: "3.5\n2\n".into(),
                stdout_contains: strings(&["Dien tich :    7.00 m2", "Chu vi    :   11.00 m"]),
                ..Default::default()
            },
        ),
        case(
            "02",
            &["02-VONG-LAP.BAS"],
            Expect {
                run: true,
                stdout_contains: strings(&[
                    "Trung binh: 26.50",
                    "Cao nhat  : 32",
                    "Mot",
                    "Hai",
                    "Nhieu",
                ]),
                ..Default::default()
            },
        ),
        case(
            "03",
            &["03-HAM-VA-THU-TUC.BAS"],
            Expect {
                run: true,
                stdout_contains: strings(&["5 binh phuong = 25", "10! = 3628800", "Lan goi thu 3"]),
                ..Default::default()
            },
        ),
        case(
            "04",
            &["04-DOC-GHI-TEP.BAS"],
            Expect {
                run: true,
                stdout_contains: strings(&["Tong cac binh phuong = 55"]),
                files_contain: vec![
                    ("SOLIEU.TXT".into(), "25".into()),
                    ("MAY-IN-LPT1.TXT".into(), "Tong cac binh phuong = 55".into()),
                ],
                ..Default::default()
            },
        ),
        // Graphics: run in its own window on a virtual screen, and checked
        // from a picture of it — the grey axes, the yellow curve, the red
        // circle — once the key it waits for has been pressed.
        case(
            "05",
            &["05-DO-THI.BAS"],
            Expect {
                run: true,
                window: true,
                screen_size: Some((640, 350)),
                screen_colors: vec![
                    (170, 170, 170, 800),
                    (255, 255, 85, 400),
                    (255, 85, 85, 250),
                ],
                ..Default::default()
            },
        ),
        case(
            "06",
            &["06-NHIEU-TEP/CHINH.BAS", "06-NHIEU-TEP/HANGSO.BAS"],
            Expect {
                run: true,
                stdin: "2\n".into(),
                stdout_contains: strings(&[
                    "Dien tich hinh tron:  12.566 m2",
                    "Do chinh xac: 7 chu so",
                ]),
                ..Default::default()
            },
        ),
        case(
            "07",
            &["07-LOI-CO-Y.BAS"],
            Expect {
                success: false,
                error_in_file: Some("07-LOI-CO-Y.BAS".into()),
                error_line: Some(6),
                ..Default::default()
            },
        ),
    ];

    let tmp = tempfile::tempdir().unwrap();
    let paths = AppPaths::under(tmp.path().join("app"));
    let guard = FsGuard::new(paths.write_roots().to_vec()).unwrap();
    let mut failures = Vec::new();
    for (i, c) in cases.iter().enumerate() {
        if let Err(e) = run_case(
            &tc,
            c,
            &tmp.path().join("sources"),
            &paths,
            &guard,
            i as u64 + 1,
        ) {
            failures.push(format!("[example {}] {e}", c.name));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

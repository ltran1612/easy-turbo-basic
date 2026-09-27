//! Turbo Basic → QB64 Phoenix Edition.
//!
//! No compiler of this century accepts Turbo Basic as written: QB64 reads
//! `PRINT#1,` as a variable name, has never had `DEF FN`, and cannot open a
//! printer port. So the program is translated, into the work tree, before
//! QB64 sees it. The user's file is only read — this module does not even have
//! a way to write; it returns bytes, and staging decides where they go.
//!
//! **Line numbers are kept.** Every rewrite edits within a line, so line 155 of
//! the staged program is line 155 of the user's. QB64's compile errors, and the
//! run-time errors the saved program reports long after we are gone, then point
//! at the line the user can find. The few lines that have to be added go after
//! the user's last line, and [`LineMap`] records them.
//!
//! Pure: no filesystem, no processes, no clock. What goes in is bytes and what
//! comes out is bytes, which is what makes every rule testable on its own.

pub mod conditional;
pub mod deftype;
pub mod findings;
pub mod keywords;
pub mod lexer;
pub mod linemap;
pub mod physical;
pub mod program;
pub mod rewrite;
pub mod stmt;

pub use findings::{Finding, Severity};
pub use linemap::{LineMap, Origin, StagedMap};

use crate::error::ProblemArg;
use lexer::Kind;

/// Name of the translated main program in the work tree.
pub const STAGED_MAIN: &str = "prog.bas";
/// Name of the runtime support file in the work tree.
pub const PRELUDE_NAME: &str = "etb_prelude.bas";

const PRELUDE_TEMPLATE: &str = include_str!("../../assets/prelude/etb_prelude.bas");

/// A source file as read from the user's disk.
#[derive(Debug, Clone)]
pub struct SourceFile {
    /// The name the user knows it by, for messages.
    pub display_name: String,
    pub bytes: Vec<u8>,
}

/// How a test build differs from the build a user gets. Never set by the GUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestMode {
    /// Text goes to standard output and `INPUT` reads standard input, so a
    /// test can drive the program. Costs nothing in the program: a FreeBASIC
    /// console program already reads and writes the streams it was started
    /// with, and only the wait for a key at the end has to go.
    Console,
    /// The program runs in its own window, as the user sees it, on a virtual
    /// screen. At the end, a picture of the screen is saved beside it, as
    /// `etb-screen.png`, for the test to look at; and a wait for a key is
    /// answered with Enter, since there is no one to press one.
    Window,
}

/// The screenshot a window-mode test program leaves beside itself.
pub const TEST_SCREENSHOT: &str = "etb-screen.bmp";

#[derive(Debug, Clone)]
pub struct TranslateOptions {
    /// Wait for a key before the program's window closes.
    pub keep_window_open: bool,
    pub test_mode: Option<TestMode>,
    /// Start of the names of printer files: `<stem>-LPT1.TXT`. ASCII letters,
    /// digits and `-` only; anything else is dropped.
    pub spool_stem: String,
}

impl Default for TranslateOptions {
    fn default() -> Self {
        Self {
            keep_window_open: true,
            test_mode: None,
            // "máy in" is Vietnamese for printer.
            spool_stem: "MAY-IN".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct StagedFile {
    pub name: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Translation {
    /// The main program first, then each included file, the runtime support
    /// file last.
    pub files: Vec<StagedFile>,
    pub map: LineMap,
    pub findings: Vec<Finding>,
    /// Every file of the user's that went into it, in `map.sources` order:
    /// the program, then what it included. For quoting a line in a message.
    pub sources: Vec<SourceFile>,
}

impl Translation {
    /// Whether anything stops the program being built.
    pub fn refused(&self) -> bool {
        self.findings.iter().any(|f| f.severity == Severity::Refuse)
    }

    pub fn main(&self) -> &StagedFile {
        &self.files[0]
    }
}

/// Why an `$INCLUDE` could not be followed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IncludeError {
    /// Not found. Where it was looked for, for the message.
    NotFound { searched: Vec<String> },
    /// Found, but not a program: the i18n key saying what it is.
    NotText { reason_key: &'static str },
}

const CRLF: &[u8] = b"\r\n";

/// Nesting deeper than this is a loop the name check missed, or a program no
/// one wrote by hand.
const MAX_INCLUDE_DEPTH: usize = 16;

/// The name the n-th included file is staged under.
fn include_name(n: usize) -> String {
    format!("etb_inc_{n}.bas")
}

/// Translate a program that includes nothing.
pub fn translate(main: &SourceFile, opts: &TranslateOptions) -> Translation {
    translate_with(
        main,
        &mut |_| {
            Err(IncludeError::NotFound {
                searched: Vec::new(),
            })
        },
        opts,
    )
}

/// One of the user's files, split into lines.
struct File {
    src: SourceFile,
    lines: Vec<(Vec<u8>, Vec<u8>)>,
    text_after_ctrl_z: bool,
}

impl File {
    fn new(src: SourceFile) -> Self {
        let phys = physical::split(&src.bytes);
        let lines = phys
            .lines
            .iter()
            .map(|l| (l.text.to_vec(), l.ending.to_vec()))
            .collect();
        let text_after_ctrl_z = phys.text_after_ctrl_z;
        Self {
            src,
            lines,
            text_after_ctrl_z,
        }
    }
}

/// One line of the program as Turbo Basic compiled it: included files
/// appear at the point they are included.
struct Unit {
    file: usize,
    line: usize,
    /// For an `$INCLUDE` line that was followed: the file it included.
    includes: Option<usize>,
}

/// A unit, the i18n key for what went wrong with its `$INCLUDE`, and the
/// message's arguments.
type IncludeProblem = (usize, &'static str, Vec<(&'static str, ProblemArg)>);

/// Loads the program and everything it includes, in the order Turbo Basic
/// read them.
struct Loader<'l> {
    load: &'l mut dyn FnMut(&str) -> Result<SourceFile, IncludeError>,
    files: Vec<File>,
    units: Vec<Unit>,
    /// Problems, by the unit whose `$INCLUDE` caused them.
    problems: Vec<IncludeProblem>,
}

impl Loader<'_> {
    fn walk(&mut self, file: usize, stack: &mut Vec<String>) {
        for li in 0..self.files[file].lines.len() {
            let unit = self.units.len();
            self.units.push(Unit {
                file,
                line: li,
                includes: None,
            });
            let Some(name) = include_directive(&self.files[file].lines[li].0) else {
                continue;
            };
            let upper = name.to_ascii_uppercase();
            if stack.contains(&upper) || stack.len() >= MAX_INCLUDE_DEPTH {
                self.problems.push((
                    unit,
                    "tr.refuse.include_cycle",
                    vec![("name", ProblemArg::text(&name))],
                ));
                continue;
            }
            match (self.load)(&name) {
                Ok(src) => {
                    let child = self.files.len();
                    self.files.push(File::new(src));
                    self.units[unit].includes = Some(child);
                    stack.push(upper);
                    self.walk(child, stack);
                    stack.pop();
                }
                Err(IncludeError::NotFound { searched }) => self.problems.push((
                    unit,
                    "tr.refuse.include_missing",
                    vec![
                        ("name", ProblemArg::text(&name)),
                        ("searched", ProblemArg::text(searched.join("; "))),
                    ],
                )),
                Err(IncludeError::NotText { reason_key }) => self.problems.push((
                    unit,
                    "tr.refuse.include_not_text",
                    vec![
                        ("name", ProblemArg::text(&name)),
                        ("what", ProblemArg::Key(reason_key)),
                    ],
                )),
            }
        }
    }
}

/// `$INCLUDE "CONST.BAS"`: the file named, with `.BAS` added when the name
/// has no extension, as Turbo Basic did.
fn include_directive(line: &[u8]) -> Option<String> {
    let lexed = lexer::lex(line);
    let meta = lexed.tokens.iter().find(|t| t.kind == Kind::Meta)?;
    let text = String::from_utf8_lossy(meta.text(line)).into_owned();
    let rest = text.get(1..)?.trim_start();
    if !rest.get(..7)?.eq_ignore_ascii_case("INCLUDE") {
        return None;
    }
    let rest = rest[7..].trim();
    let name = match rest.strip_prefix('"') {
        Some(r) => r.split('"').next()?,
        None => rest.split(['\'', ' ', '\t']).next()?,
    }
    .trim();
    if name.is_empty() {
        return None;
    }
    let has_ext = name
        .rsplit(['\\', '/'])
        .next()
        .is_some_and(|n| n.contains('.'));
    Some(if has_ext {
        name.to_string()
    } else {
        format!("{name}.BAS")
    })
}

pub fn translate_with(
    main: &SourceFile,
    load: &mut dyn FnMut(&str) -> Result<SourceFile, IncludeError>,
    opts: &TranslateOptions,
) -> Translation {
    let mut loader = Loader {
        load,
        files: vec![File::new(main.clone())],
        units: Vec::new(),
        problems: Vec::new(),
    };
    loader.walk(0, &mut vec![main.display_name.to_ascii_uppercase()]);
    let Loader {
        files,
        units,
        problems,
        ..
    } = loader;

    let text_of = |u: &Unit| files[u.file].lines[u.line].0.as_slice();
    let texts: Vec<&[u8]> = units.iter().map(text_of).collect();

    // What is compiled at all. A line in a branch not taken defines nothing,
    // so the analysis sees it as empty.
    let cond = conditional::settle(&texts);
    let active_texts: Vec<&[u8]> = texts
        .iter()
        .zip(&cond.active)
        .map(|(t, &a)| if a { *t } else { &b""[..] })
        .collect();

    let prog = program::Program::analyse_with(&active_texts, cond.consts.clone());
    let mut rw = rewrite::Rewriter::new(&prog);
    rw.test_window = opts.test_mode == Some(TestMode::Window);
    for (i, u) in units.iter().enumerate() {
        if let Some(child) = u.includes {
            rw.includes.insert(i, include_name(child));
        }
    }

    let mut findings = Vec::new();
    let finding = |unit: usize, severity, key, args| Finding {
        severity,
        key,
        file: files[units[unit].file].src.display_name.clone(),
        line: Some(line_no(units[unit].line)),
        args,
    };
    for f in &files {
        if f.text_after_ctrl_z {
            findings.push(Finding {
                severity: Severity::Warn,
                key: "tr.warn.text_after_ctrl_z",
                file: f.src.display_name.clone(),
                line: Some(line_no(f.lines.len())),
                args: vec![("file", ProblemArg::text(&f.src.display_name))],
            });
        }
    }
    for (unit, key, args) in problems {
        // An $INCLUDE in a branch that is not compiled was never read.
        if cond.active[unit] {
            findings.push(finding(unit, Severity::Refuse, key, args));
        }
    }
    for &(unit, key) in &cond.problems {
        findings.push(finding(unit, Severity::Refuse, key, Vec::new()));
    }

    // Each file's staged text and line origins.
    let mut out: Vec<Vec<u8>> = vec![Vec::new(); files.len()];
    let mut origins: Vec<Vec<Origin>> = vec![Vec::new(); files.len()];

    for (i, u) in units.iter().enumerate() {
        let (text, ending) = &files[u.file].lines[u.line];
        let buf = &mut out[u.file];
        if cond.active[i] && !cond.directive[i] {
            let edits = rw.line(i);
            if edits.is_empty() {
                buf.extend_from_slice(text);
            } else {
                buf.extend_from_slice(&rewrite::apply(text, &edits));
            }
        } else {
            // Not compiled, or a $IF/$ELSE/$ENDIF: kept, as a comment, so
            // the line is still there to count.
            //
            // `REM .` and not `'`: FreeBASIC reads a `$word` at the start of a
            // comment as one of its own directives, whatever the comment
            // marker, so `$IF` would be parsed and rejected. The full stop is
            // there to make sure the `$` is never first.
            buf.extend_from_slice(b"REM .");
            buf.extend_from_slice(text);
        }
        // A lone CR ended lines on old Macs and in some DOS editors; nothing
        // downstream should have to agree on whether it ends a line. And a
        // last line with no ending gets one, because lines follow it.
        match ending.as_slice() {
            b"\r" | b"" => buf.extend_from_slice(CRLF),
            e => buf.extend_from_slice(e),
        }
        origins[u.file].push(Origin::User {
            file: u.file,
            line: line_no(u.line),
        });
    }

    // Falling off the end finishes the program too.
    out[0].extend_from_slice(b"ETB_FINISH: END");
    out[0].extend_from_slice(CRLF);
    origins[0].push(Origin::Injected);

    // Code that could not stay on its line: after the program, each line
    // mapped to the line it came from, so an error in it is still the user's.
    for m in &rw.moved {
        let u = &units[m.line];
        for t in &m.text {
            out[0].extend_from_slice(t.as_bytes());
            out[0].extend_from_slice(CRLF);
            origins[0].push(Origin::User {
                file: u.file,
                line: line_no(u.line),
            });
        }
    }

    findings.extend(
        rw.findings
            .iter()
            .map(|f| finding(f.line, f.severity, f.key, f.args.clone())),
    );
    findings.sort_by(|a, b| (a.severity, &a.file, a.line).cmp(&(b.severity, &b.file, b.line)));

    let prelude = prelude_text(opts, &rw.declarations, &rw.shared);
    let prelude_lines = prelude.lines().count();

    let mut staged_files = Vec::new();
    let mut staged_maps = Vec::new();
    for (k, (bytes, lines)) in out.into_iter().zip(origins).enumerate() {
        let name = if k == 0 {
            STAGED_MAIN.to_string()
        } else {
            include_name(k)
        };
        staged_files.push(StagedFile {
            name: name.clone(),
            bytes,
        });
        staged_maps.push(StagedMap {
            staged: name,
            lines,
        });
    }
    staged_files.push(StagedFile {
        name: PRELUDE_NAME.into(),
        bytes: prelude.into_bytes(),
    });
    staged_maps.push(StagedMap {
        staged: PRELUDE_NAME.into(),
        lines: (1..=prelude_lines)
            .map(|n| Origin::Prelude {
                line: line_no(n - 1),
            })
            .collect(),
    });

    Translation {
        files: staged_files,
        map: LineMap {
            sources: files.iter().map(|f| f.src.display_name.clone()).collect(),
            staged: staged_maps,
        },
        findings,
        sources: files.into_iter().map(|f| f.src).collect(),
    }
}

fn line_no(i: usize) -> u32 {
    u32::try_from(i + 1).unwrap_or(u32::MAX)
}

/// In window test mode, the keyboard is a test: every key asked for is Enter.
const TEST_KEYS: &str = "' Window test mode only: there is no one to press a key.
FUNCTION ETB_TESTKEYS$ (ETB_N AS LONG)
    ETB_TESTKEYS$ = STRING$(ETB_N, 13)
END FUNCTION

FUNCTION ETB_TESTINKEY$
    ETB_TESTINKEY$ = CHR$(13)
END FUNCTION
";

fn prelude_text(
    opts: &TranslateOptions,
    declarations: &[String],
    shared: &std::collections::BTreeSet<String>,
) -> String {
    let stem: String = opts
        .spool_stem
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    let stem = if stem.is_empty() {
        "PRINTER".into()
    } else {
        stem
    };

    let exit = match opts.test_mode {
        Some(TestMode::Window) => format!("BSAVE \"{TEST_SCREENSHOT}\", 0: SYSTEM"),
        Some(TestMode::Console) => "SYSTEM".into(),
        None if !opts.keep_window_open => "SYSTEM".into(),
        None => "' END follows, and waits for a key.".into(),
    };
    let test_support = if opts.test_mode == Some(TestMode::Window) {
        TEST_KEYS
    } else {
        ""
    };
    let mut declared = String::new();
    if !shared.is_empty() {
        declared.push_str(
            "' Variables the program's functions use without declaring them. In\n             ' Turbo Basic they belong to the main program; FreeBASIC has no\n             ' SHARED inside a procedure, so they are the module's, here.\n",
        );
        for name in shared {
            if let Some(base) = name.strip_suffix("()") {
                // `(10)` and not `()`: Turbo Basic gives an array nobody
                // dimensioned the bounds 0 to 10, and FreeBASIC does not
                // bounds-check, so an empty one is a program that writes past
                // the end of nothing and dies.
                declared.push_str(&format!("REDIM SHARED {base}(10)\n"));
            } else {
                declared.push_str(&format!("DIM SHARED {name}\n"));
            }
        }
        declared.push('\n');
    }
    let declared_fns = if declarations.is_empty() {
        String::new()
    } else {
        format!(
            "' What the program defines further down. FreeBASIC will not call a\n             ' procedure it has not seen declared, and these programs call\n             ' functions defined below the call.\n{}\n",
            declarations.join("\n")
        )
    };
    declared.push_str(&declared_fns);
    PRELUDE_TEMPLATE
        .replace("{{DECLARATIONS}}", &declared)
        .replace("{{SPOOL_STEM}}", &stem)
        .replace("{{FINISH_EXIT}}", &exit)
        .replace("{{TEST_SUPPORT}}", test_support)
        .replace("{{PRINT_FILE}}", "' Kept as a file beside the program.")
        .replace('\n', "\r\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src(text: &[u8]) -> SourceFile {
        SourceFile {
            display_name: "CALC.BAS".into(),
            bytes: text.to_vec(),
        }
    }

    fn lines(t: &Translation) -> Vec<String> {
        String::from_utf8(t.main().bytes.clone())
            .unwrap()
            .split("\r\n")
            .map(str::to_string)
            .collect()
    }

    /// The shape of legacy code this exists for: a leading blank line, a
    /// numbered menu, `?` for PRINT, a glued `print#`, results written to a
    /// file and to a printer port named in a variable, CRLF, and Ctrl-Z.
    const LEGACY: &[u8] = b"\r\ncolor 14,1\r\n2 cls\r\ninput \"Ten \";t$\r\n?\r\n\
input \"Luu (c/k) \";k$\r\nif k$ =\"k\" then 35\r\ninput \"file\";f$\r\n\
open f$ for output as 4\r\nprint# 4,space$(15);\"KET QUA\"\r\nprint# 4,using \"##.#\";x;\r\n\
35 ?\r\np$=\"lpt1\"\r\nopen p$ for output as 3\r\nprint# 3,\"x\"\r\nclose\r\n\
input \"Again (c/k) \";a$\r\nif a$ =\"c\" then 2\r\nend\r\n\r\n\x1a";

    #[test]
    fn a_translated_line_stays_on_its_own_line_number() {
        let t = translate(&src(LEGACY), &TranslateOptions::default());
        let out = lines(&t);
        let input: Vec<&str> = std::str::from_utf8(&LEGACY[..LEGACY.len() - 1])
            .unwrap()
            .split("\r\n")
            .collect();
        // Line 10 is the glued print# — the kind of line QB64 stops on.
        assert_eq!(input[9], "print# 4,space$(15);\"KET QUA\"");
        assert_eq!(out[9], "print # 4,space$(15);\"KET QUA\"");
        assert_eq!(
            t.map.main_origin(10),
            Some(Origin::User { file: 0, line: 10 })
        );
        // Every user line maps to itself.
        for n in 1..=20u32 {
            assert_eq!(
                t.map.main_origin(n),
                Some(Origin::User { file: 0, line: n })
            );
        }
    }

    #[test]
    fn the_legacy_program_translates_as_expected() {
        let t = translate(&src(LEGACY), &TranslateOptions::default());
        let out = lines(&t);
        assert_eq!(out[0], "");
        assert_eq!(out[4], "PRINT");
        assert_eq!(out[8], "open ETB_DEV$((f$), (4)) for output as 4");
        assert_eq!(out[11], "35 PRINT");
        assert_eq!(out[13], "open ETB_DEV$((p$), (3)) for output as 3");
        assert_eq!(out[15], "close: ETB_CLOSED -1");
        assert_eq!(out[18], "ETB_FINISH: END");
        assert_eq!(out[20], "ETB_FINISH: END");
        assert_eq!(
            out.len(),
            22,
            "nothing follows: the runtime support is a separate file, given to \
             the compiler with -include"
        );
        assert!(t.findings.is_empty());
        assert!(!t.refused());
    }

    /// Two statements on one line that both have to go. Turbo Basic programs
    /// declare constants this way — `%DUNG = -1 : %SAI = 0` — and a pair of
    /// single-line DEF FNs sharing a line is just as ordinary.
    ///
    /// Each removal takes the colon beside it with it, so two of them used to
    /// ask to remove the same colon; the second edit was dropped and half a
    /// definition stayed in the program, to be reported later as an error on a
    /// line the user had written correctly.
    #[test]
    fn two_statements_on_one_line_that_both_go_both_go() {
        let t = translate(
            &src(b"%TRUE = -1 : %FALSE = 0\r\nPRINT %TRUE, %FALSE\r\n"),
            &TranslateOptions::default(),
        );
        let out = lines(&t);
        assert_eq!(out[0].trim(), "", "nothing of either definition is left");
        assert_eq!(out[1], "PRINT (-1), (0)");
        assert!(t.findings.is_empty(), "{:?}", t.findings);

        let t = translate(
            &src(b"%A = 1 : %B = 2 : %C = 3\r\nPRINT %A, %B, %C\r\n"),
            &TranslateOptions::default(),
        );
        let out = lines(&t);
        assert_eq!(out[0].trim(), "");
        assert_eq!(out[1], "PRINT (1), (2), (3)");

        let t = translate(
            &src(b"DEF FNA(X) = X * 2 : DEF FNB(Y) = Y + 1\r\nPRINT FNA(1), FNB(2)\r\n"),
            &TranslateOptions::default(),
        );
        let out = lines(&t);
        assert_eq!(out[0].trim(), "");
        assert_eq!(out[1], "PRINT ETB_FNA!(1), ETB_FNB!(2)");
        assert!(
            out.iter().filter(|l| l.starts_with("FUNCTION ")).count() == 2,
            "both definitions were written out: {out:?}"
        );
    }

    #[test]
    fn test_mode_costs_no_line_at_all_and_every_line_is_still_the_users() {
        let opts = TranslateOptions {
            test_mode: Some(TestMode::Console),
            ..Default::default()
        };
        let t = translate(&src(LEGACY), &opts);
        assert_eq!(
            t.map.main_origin(1),
            Some(Origin::User { file: 0, line: 1 }),
            "line 1 of the staged program is line 1 of theirs"
        );
        assert_eq!(
            t.map.main_origin(10),
            Some(Origin::User { file: 0, line: 10 })
        );
    }

    #[test]
    fn the_prelude_has_every_placeholder_filled() {
        for opts in [
            TranslateOptions::default(),
            TranslateOptions {
                keep_window_open: false,
                test_mode: Some(TestMode::Console),
                spool_stem: "máy in!".into(),
            },
            TranslateOptions {
                test_mode: Some(TestMode::Window),
                ..Default::default()
            },
        ] {
            let p = prelude_text(&opts, &[], &Default::default());
            assert!(!p.contains("{{"), "unfilled placeholder in:\n{p}");
            assert!(p.is_ascii(), "the prelude must stay ASCII");
        }
    }

    #[test]
    fn the_window_waits_unless_told_otherwise() {
        let waits = prelude_text(&TranslateOptions::default(), &[], &Default::default());
        assert!(!waits.contains("    SYSTEM"));
        let goes = prelude_text(
            &TranslateOptions {
                keep_window_open: false,
                ..Default::default()
            },
            &[],
            &Default::default(),
        );
        assert!(goes.contains("    SYSTEM"));
    }

    #[test]
    fn text_after_ctrl_z_is_worth_a_warning() {
        let t = translate(&src(b"end\r\n\x1aold\r\n"), &TranslateOptions::default());
        assert_eq!(t.findings.len(), 1);
        assert_eq!(t.findings[0].key, "tr.warn.text_after_ctrl_z");
    }

    #[test]
    fn a_last_line_without_an_ending_still_gets_one() {
        let t = translate(&src(b"print 1"), &TranslateOptions::default());
        assert_eq!(lines(&t)[..2], ["print 1", "ETB_FINISH: END"]);
    }

    #[test]
    fn lines_that_need_nothing_are_copied_byte_for_byte() {
        // Including bytes that are not UTF-8 at all: a DOS code page's
        // Vietnamese inside a string must come out exactly as it went in.
        let text = b"print \"\xE2\xEA\xF4 \xB0\xB1\"\nx = 1\n";
        let t = translate(&src(text), &TranslateOptions::default());
        assert!(t.main().bytes.starts_with(text));
    }

    #[test]
    fn nothing_ever_panics_on_arbitrary_bytes() {
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        for _ in 0..500 {
            let len = (state % 300) as usize;
            let bytes: Vec<u8> = (0..len)
                .map(|_| {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    (state >> 24) as u8
                })
                .collect();
            let t = translate(&src(&bytes), &TranslateOptions::default());
            let user_lines = physical::split(&bytes).lines.len();
            // One line is added: the call that finishes the program.
            assert_eq!(t.map.staged[0].lines.len(), user_lines + 1);
        }
    }
}

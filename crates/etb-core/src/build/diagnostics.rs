//! Reading what FreeBASIC says, and saying it about the user's file.
//!
//! Deliberately a pure `&str -> Vec<Diagnostic>` parse plus a separate remap,
//! with no I/O: that is what makes it testable from captured output on a
//! machine with no compiler installed.
//!
//! `fbc` reports like this, on stdout, and carries on to find more:
//!
//! ```text
//! prog.bas(155) error 10: Expected '='
//! print# 4,space$(15);"CHUONG TRINH"
//!        ^
//! ```
//!
//! and stops with `error 133: Too many errors, exiting`, which has no line of
//! its own. Warnings have the same shape with a second number:
//! `prog.bas(12) warning 3(1): Suspicious pointer assignment`.
//!
//! The file and line are of the *staged* program. `remap` turns them back into
//! the user's file and line, and puts the user's own text beside them — not
//! the translated line, which is not what they will find when they open the
//! file.
//!
//! Anything the linker says is ours: the user's program cannot cause a missing
//! library, and telling them to fix it would send them nowhere.

use crate::translate::{LineMap, Origin};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

impl Severity {
    pub fn is_error(self) -> bool {
        matches!(self, Severity::Error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: Severity,
    /// The file: a staged name as QB64 reported it, until `remap` runs; then
    /// the user's file.
    pub file: Option<String>,
    pub line: Option<u32>,
    /// QB64 does not report columns. Kept so every consumer has one shape.
    pub col: Option<u32>,
    /// QB64's message, in English.
    pub message: String,
    /// The line in question: the user's own text after `remap`.
    pub snippet: Vec<String>,
    /// The problem is in something we generated — a line we added, or our
    /// runtime support — or in the C++ stage behind QB64. Not the user's to fix,
    /// and the interface says so.
    pub ours: bool,
}

impl Diagnostic {
    fn error(message: String) -> Self {
        Self {
            severity: Severity::Error,
            file: None,
            line: None,
            col: None,
            message,
            snippet: Vec::new(),
            ours: false,
        }
    }
}

/// What the linker says when this application has been packaged wrongly. The
/// user's program cannot cause it, so it is never reported as theirs.
const LINKER_MARKS: &[&str] = &["ld:", "ld.bfd:", "ld returned", "undefined reference"];

pub fn parse(output: &str) -> Vec<Diagnostic> {
    let lines: Vec<&str> = output.lines().map(|l| l.trim_end_matches('\r')).collect();
    let mut out = Vec::new();

    for (i, l) in lines.iter().enumerate() {
        if let Some(mut d) = located(l) {
            // The line after a located message is fbc's copy of the source,
            // and the one after that is its caret. The copy is a stand-in
            // until `remap` puts the user's own line there.
            if let Some(text) = lines.get(i + 1).filter(|t| !t.trim().is_empty()) {
                if !is_caret(text) && located(text).is_none() {
                    d.snippet = vec![text.to_string()];
                }
            }
            out.push(d);
            continue;
        }
        let t = l.trim();
        if let Some(rest) = t.strip_prefix("error ") {
            // `error 133: Too many errors, exiting` — real, and unplaced.
            if let Some((_, msg)) = rest.split_once(':') {
                out.push(Diagnostic::error(msg.trim().to_string()));
            }
            continue;
        }
        if LINKER_MARKS.iter().any(|m| t.contains(m)) {
            let mut d = Diagnostic::error(t.to_string());
            d.ours = true;
            out.push(d);
        }
    }
    out
}

fn is_caret(text: &str) -> bool {
    let t = text.trim();
    !t.is_empty() && t.chars().all(|c| c == '^')
}

/// `prog.bas(155) error 10: Expected '='` — and the same shape for a warning,
/// whose number may carry a level in brackets.
fn located(line: &str) -> Option<Diagnostic> {
    // Left to right, because a warning carries a second number in brackets —
    // `warning 3(1):` — and the rightmost bracket is that one, not the line.
    // A candidate counts only if what follows it says error or warning.
    let (open, close, n, severity, rest) = line.char_indices().find_map(|(open, c)| {
        if c != '(' {
            return None;
        }
        let close = line[open..].find(')')? + open;
        let n: u32 = line[open + 1..close].parse().ok()?;
        let after = line[close + 1..].trim_start();
        let (severity, rest) = match () {
            _ if after.starts_with("error ") => (Severity::Error, &after[6..]),
            _ if after.starts_with("warning ") => (Severity::Warning, &after[8..]),
            _ => return None,
        };
        Some((open, close, n, severity, rest))
    })?;
    let _ = close;
    let file = line[..open].trim();
    if file.is_empty() {
        return None;
    }
    let message = rest.split_once(':').map(|(_, m)| m.trim()).unwrap_or(rest);
    Some(Diagnostic {
        severity,
        file: Some(file.to_string()),
        line: Some(n),
        col: None,
        message: message.to_string(),
        snippet: Vec::new(),
        ours: false,
    })
}

/// Put each diagnostic on the user's file and line, with the user's own text.
///
/// `main_staged` is the staged name of the main program, which QB64 does not
/// print (it names only included files). `user_line` gives the text of a line
/// of one of the user's files, as the translator read it.
pub fn remap(
    diags: &mut [Diagnostic],
    map: &LineMap,
    main_staged: &str,
    user_line: &dyn Fn(usize, u32) -> Option<String>,
) {
    for d in diags.iter_mut() {
        let Some(line) = d.line else { continue };
        // fbc names the file it was given, which is an absolute path into the
        // work tree; the line map knows staged files by name.
        let staged = d
            .file
            .as_deref()
            .map(|f| {
                std::path::Path::new(f)
                    .file_name()
                    .map_or_else(|| f.to_string(), |n| n.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| main_staged.to_string());
        match map.origin(&staged, line) {
            Some(Origin::User { file, line }) => {
                d.file = map.source_name(file).map(str::to_string);
                d.line = Some(line);
                if let Some(text) = user_line(file, line) {
                    d.snippet = vec![text];
                }
            }
            Some(Origin::Injected) | Some(Origin::Prelude { .. }) => {
                d.ours = true;
            }
            None => {}
        }
    }
}

pub fn count_errors(diags: &[Diagnostic]) -> usize {
    diags.iter().filter(|d| d.severity.is_error()).count()
}

pub fn count_warnings(diags: &[Diagnostic]) -> usize {
    diags
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::translate::StagedMap;

    /// Captured from FreeBASIC 1.10.1 (`fbc -lang qb`).
    const SYNTAX: &str = "prog.bas(6) error 10: Expected '='\nprint# 4,space$(3);\"x\"\n       ^\n";

    #[test]
    fn a_compile_error_is_read_with_its_line_and_text() {
        let d = parse(SYNTAX);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].message, "Expected '='");
        assert_eq!(d[0].line, Some(6));
        assert_eq!(d[0].file.as_deref(), Some("prog.bas"));
        assert_eq!(d[0].snippet, ["print# 4,space$(3);\"x\""]);
        assert!(!d[0].ours);
    }

    #[test]
    fn an_error_in_an_included_file_is_placed_in_that_file() {
        // fbc names the file the error is in, whichever it is, so an include
        // needs no special reading.
        let out = "etb_inc_1.bas(3) error 4: Duplicated definition, KM\nKM = 2\n^\n";
        let d = parse(out);
        assert_eq!(d[0].message, "Duplicated definition, KM");
        assert_eq!(d[0].file.as_deref(), Some("etb_inc_1.bas"));
        assert_eq!(d[0].line, Some(3));
    }

    #[test]
    fn an_absolute_path_is_reduced_to_the_staged_name() {
        let mut d = parse("/home/x/work/build-1/src/prog.bas(3) error 10: Expected '='\n");
        assert_eq!(d[0].line, Some(3));
        remap(&mut d, &map(), "prog.bas", &|_, line| {
            Some(format!("line {line} as the user wrote it"))
        });
        assert_eq!(
            d[0].file.as_deref(),
            Some("CALC.BAS"),
            "the path fbc echoes back is not a name the line map knows"
        );
        assert_eq!(d[0].snippet, ["line 2 as the user wrote it"]);
    }

    #[test]
    fn a_warning_is_a_warning_and_not_an_error() {
        let d = parse("prog.bas(12) warning 3(1): Suspicious pointer assignment\n");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].severity, Severity::Warning);
        assert_eq!(d[0].message, "Suspicious pointer assignment");
        assert_eq!(count_errors(&d), 0);
        assert_eq!(count_warnings(&d), 1);
    }

    #[test]
    fn the_message_that_stops_the_compiler_is_kept_although_it_has_no_line() {
        let d = parse("error 133: Too many errors, exiting\n");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].message, "Too many errors, exiting");
        assert_eq!(d[0].line, None);
    }

    #[test]
    fn a_failure_of_the_linker_is_ours() {
        let d = parse("/usr/bin/ld.bfd: cannot find -lXext: No such file or directory\n");
        assert_eq!(d.len(), 1);
        assert!(
            d[0].ours,
            "the user's program cannot cause a missing library"
        );
        assert_eq!(d[0].line, None);
    }

    #[test]
    fn progress_and_banners_are_not_diagnostics() {
        let out = "FreeBASIC Compiler - Version 1.10.1 (2023-12-24)\n                   target: linux-x86_64 (64bit)\ncompiling: prog.bas\nlinking: program\n";
        assert!(parse(out).is_empty());
    }

    fn map() -> LineMap {
        LineMap {
            sources: vec!["CALC.BAS".into()],
            staged: vec![
                StagedMap {
                    staged: "prog.bas".into(),
                    lines: vec![
                        Origin::Injected,
                        Origin::User { file: 0, line: 1 },
                        Origin::User { file: 0, line: 2 },
                    ],
                },
                StagedMap {
                    staged: "etb_prelude.bas".into(),
                    lines: vec![Origin::Prelude { line: 1 }],
                },
            ],
        }
    }

    #[test]
    fn a_staged_line_becomes_the_users_line_with_the_users_text() {
        let mut d = parse("prog.bas(3) error 10: Expected '='\nprint # 1,x\n");
        remap(&mut d, &map(), "prog.bas", &|file, line| {
            assert_eq!((file, line), (0, 2));
            Some("  print# 1,x   ' as they typed it".into())
        });
        assert_eq!(d[0].file.as_deref(), Some("CALC.BAS"));
        assert_eq!(d[0].line, Some(2));
        assert_eq!(d[0].snippet, ["  print# 1,x   ' as they typed it"]);
        assert!(!d[0].ours);
    }

    #[test]
    fn an_error_on_a_line_we_added_is_ours() {
        let mut d = parse("prog.bas(1) error 10: Expected '='\n");
        remap(&mut d, &map(), "prog.bas", &|_, _| None);
        assert!(d[0].ours, "line 1 of the staged program is one we added");

        let mut d = parse("etb_prelude.bas(1) error 14: Type mismatch\n");
        remap(&mut d, &map(), "prog.bas", &|_, _| None);
        assert!(d[0].ours, "an error in our runtime support is our fault");
    }
}

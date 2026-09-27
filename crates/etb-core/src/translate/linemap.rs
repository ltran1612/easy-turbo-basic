//! Where each line of a staged file came from.
//!
//! QB64 names a line of *our* file; the user needs a line of *theirs*. Almost
//! every staged line is the user's line of the same number, because rewrites
//! edit within a line. The map exists for the few that are not: lines added to
//! make the program work, and the whole of the runtime support file.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Origin {
    /// Line `line` (1-based) of the user's file `file`, an index into
    /// [`LineMap::sources`].
    User { file: usize, line: u32 },
    /// A line we added to one of the user's files.
    Injected,
    /// Line `line` of our runtime support file. An error here is our fault.
    Prelude { line: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StagedMap {
    /// The staged file's name, as QB64 will report it.
    pub staged: String,
    /// Origin of each staged line; index 0 is line 1.
    pub lines: Vec<Origin>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct LineMap {
    /// The user's files, by the name the user knows them by.
    pub sources: Vec<String>,
    pub staged: Vec<StagedMap>,
}

impl LineMap {
    /// Where line `line` (1-based) of the staged file `staged` came from.
    ///
    /// `staged` may be a bare name or a path: QB64 prints included files by the
    /// name they were included under, and on Windows compares names without
    /// regard to case.
    pub fn origin(&self, staged: &str, line: u32) -> Option<Origin> {
        let name = file_name(staged);
        let m = self
            .staged
            .iter()
            .find(|m| m.staged.eq_ignore_ascii_case(name))?;
        let idx = usize::try_from(line).ok()?.checked_sub(1)?;
        m.lines.get(idx).copied()
    }

    /// The origin of a line of the main program.
    pub fn main_origin(&self, line: u32) -> Option<Origin> {
        let main = self.staged.first()?;
        self.origin(&main.staged, line)
    }

    pub fn source_name(&self, file: usize) -> Option<&str> {
        self.sources.get(file).map(String::as_str)
    }
}

fn file_name(p: &str) -> &str {
    p.rsplit(['/', '\\']).next().unwrap_or(p)
}

#[cfg(test)]
mod tests {
    use super::*;

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
                    staged: "etb_prelude.bm".into(),
                    lines: vec![Origin::Prelude { line: 1 }],
                },
            ],
        }
    }

    #[test]
    fn a_staged_line_maps_back_to_the_users_line() {
        let m = map();
        assert_eq!(m.main_origin(3), Some(Origin::User { file: 0, line: 2 }));
        assert_eq!(m.main_origin(1), Some(Origin::Injected));
        assert_eq!(m.main_origin(4), None);
        assert_eq!(m.main_origin(0), None);
    }

    #[test]
    fn included_files_are_found_by_name_whatever_the_path_or_case() {
        let m = map();
        assert_eq!(
            m.origin(r"C:\work\src\ETB_PRELUDE.BM", 1),
            Some(Origin::Prelude { line: 1 })
        );
    }
}

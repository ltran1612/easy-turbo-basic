//! Reading the user's files and putting a translated copy in the work tree.
//!
//! This is what makes the promise keepable: every file of the user's is read
//! once, through `FsGuard`, and from then on QB64 only ever sees our copy.
//! The copy is also the translation — Turbo Basic in, QB64 out — so the
//! staged names are ours too: plain ASCII, whatever the user's files are
//! called, which keeps a Vietnamese file name out of QB64's command line.

use crate::build::sourcefmt;
use crate::error::{EtbError, Result};
use crate::fs_guard::{FsGuard, MAX_TOTAL_SOURCE_BYTES};
use crate::paths::WorkLayout;
use crate::project::Program;
use crate::text;
use crate::translate::{self, IncludeError, SourceFile, TranslateOptions, Translation};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Staging {
    pub translation: Translation,
    /// The staged main program, as the compiler is to be given it.
    pub main: PathBuf,
    /// The runtime support file. It is not included from inside the program:
    /// it is passed with `-include`, so it is compiled before the program
    /// that calls it and no line of the user's file moves.
    pub prelude: PathBuf,
}

impl Staging {
    /// Text of line `line` (1-based) of the user's file `file` — in the
    /// translation's numbering, which includes what the program includes —
    /// for quoting in a message. Decoded leniently: this is for showing,
    /// never for building.
    pub fn user_line(&self, file: usize, line: u32) -> Option<String> {
        let src = self.translation.sources.get(file)?;
        let phys = translate::physical::split(&src.bytes);
        let l = phys
            .lines
            .get(usize::try_from(line).ok()?.checked_sub(1)?)?;
        Some(String::from_utf8_lossy(l.text).into_owned())
    }
}

pub fn stage(
    guard: &FsGuard,
    layout: &WorkLayout,
    program: &Program,
    opts: &TranslateOptions,
) -> Result<Staging> {
    if program.sources.is_empty() {
        return Err(EtbError::NoSources);
    }
    for dir in layout.all_dirs() {
        guard.create_dir_all(&dir)?;
    }

    // Every listed file is read and checked, not only the program: a PDF in
    // the list is worth saying so about whether or not anything includes it.
    let mut originals = Vec::with_capacity(program.sources.len());
    let mut total: u64 = 0;
    for src in &program.sources {
        crate::project::validate_user_path(&src.path)?;
        let bytes = FsGuard::read_user_source(&src.path)?;
        let display_name = text::display_file_name(&src.path);

        if let Err(problem) = sourcefmt::check(&bytes) {
            return Err(EtbError::UnusableFile {
                problem: Box::new(problem.problem(display_name)),
                english: problem.english(),
            });
        }

        total += bytes.len() as u64;
        if total > MAX_TOTAL_SOURCE_BYTES {
            return Err(EtbError::SourceTooLarge {
                len: total,
                limit: MAX_TOTAL_SOURCE_BYTES,
            });
        }
        originals.push(SourceFile {
            display_name,
            bytes,
        });
    }

    let listed: Vec<(PathBuf, SourceFile)> = program
        .sources
        .iter()
        .map(|s| s.path.clone())
        .zip(originals.iter().cloned())
        .collect();
    let dirs = program.include_dirs();
    let mut load = |name: &str| find_include(name, &listed, &dirs);
    let translation = translate::translate_with(&originals[0], &mut load, opts);
    for f in &translation.files {
        guard.write_file(&layout.src().join(&f.name), &f.bytes)?;
    }
    Ok(Staging {
        main: layout.src().join(&translation.main().name),
        prelude: layout.src().join(crate::translate::PRELUDE_NAME),
        translation,
    })
}

/// Find the file an `$INCLUDE` names.
///
/// Among the files the user listed first — the list is the user's own
/// statement of what the program is made of — then in the folders those files
/// are in. Names are matched without regard to case: DOS never had any, and
/// the program was written for it. A DOS path, `C:\TB\INC\CONST.BAS`, that does
/// not exist here is looked for by its file name.
pub fn find_include(
    name: &str,
    listed: &[(PathBuf, SourceFile)],
    dirs: &[PathBuf],
) -> Result<SourceFile, IncludeError> {
    // `..` is dropped, not honoured. `$INCLUDE` names a file of the user's own
    // program, and a relative path that climbs out of the folder it was found
    // in is either a mistake or a file somebody else chose — either way not
    // something to go and read because a line of BASIC asked.
    let parts: Vec<&str> = name
        .split(['\\', '/'])
        .filter(|p| !p.is_empty() && !p.ends_with(':') && *p != "." && *p != "..")
        .collect();
    let file_name = parts.last().copied().unwrap_or(name);

    if let Some((_, src)) = listed.iter().find(|(p, _)| {
        p.file_name()
            .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(file_name))
    }) {
        return Ok(src.clone());
    }

    let mut searched = Vec::new();
    for dir in dirs {
        searched.push(text::display_path(dir));
        let found =
            find_ignoring_case(dir, &parts).or_else(|| find_ignoring_case(dir, &[file_name]));
        if let Some(path) = found {
            let bytes = FsGuard::read_user_source(&path).map_err(|_| IncludeError::NotFound {
                searched: searched.clone(),
            })?;
            if let Err(problem) = sourcefmt::check(&bytes) {
                return Err(IncludeError::NotText {
                    reason_key: problem.key(),
                });
            }
            return Ok(SourceFile {
                display_name: text::display_file_name(&path),
                bytes,
            });
        }
    }
    Err(IncludeError::NotFound { searched })
}

/// `dir/a/b`, matching each part without regard to case.
fn find_ignoring_case(dir: &Path, parts: &[&str]) -> Option<PathBuf> {
    let mut at = dir.to_path_buf();
    for part in parts {
        let exact = at.join(part);
        if exact.exists() {
            at = exact;
            continue;
        }
        let entry = std::fs::read_dir(&at)
            .ok()?
            .flatten()
            .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))?;
        at = entry.path();
    }
    at.is_file().then_some(at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::SourceRef;

    #[test]
    fn an_include_cannot_climb_out_of_the_folder_it_was_found_in() {
        let td = tempfile::tempdir().unwrap();
        let outside = td.path().join("SECRET.BAS");
        std::fs::write(&outside, b"' not the program's\r\n").unwrap();
        let dir = td.path().join("program");
        std::fs::create_dir_all(&dir).unwrap();

        // The climb is dropped, so this looks for SECRET.BAS beside the
        // program — where there is none.
        assert!(find_include("..\\SECRET.BAS", &[], std::slice::from_ref(&dir)).is_err());
        assert!(find_include("../SECRET.BAS", &[], std::slice::from_ref(&dir)).is_err());

        // And the same name beside the program is found, as it should be.
        std::fs::write(dir.join("SECRET.BAS"), b"' the program's own\r\n").unwrap();
        let got = find_include("..\\SECRET.BAS", &[], &[dir]).unwrap();
        assert_eq!(got.bytes, b"' the program's own\r\n");
    }

    fn setup() -> (tempfile::TempDir, FsGuard, WorkLayout) {
        let td = tempfile::tempdir().unwrap();
        let work = td.path().join("work");
        let guard = FsGuard::new(vec![work.clone()]).unwrap();
        (td, guard, WorkLayout::new(work))
    }

    #[test]
    fn the_translation_is_staged_under_our_own_names() {
        let (td, guard, layout) = setup();
        let user = td.path().join("Tính toán.BAS");
        std::fs::write(&user, b"print# 1, x\r\nend\r\n").unwrap();
        let mut p = Program::new("t");
        p.sources.push(SourceRef::new(&user));

        let s = stage(&guard, &layout, &p, &TranslateOptions::default()).unwrap();
        assert_eq!(s.main, layout.src().join("prog.bas"));
        let staged = std::fs::read(&s.main).unwrap();
        assert!(staged.starts_with(b"print # 1, x\r\n"));
        assert!(layout.src().join("etb_prelude.bas").is_file());
        assert_eq!(
            std::fs::read(&user).unwrap(),
            b"print# 1, x\r\nend\r\n",
            "the user's file is only read"
        );
    }

    #[test]
    fn the_users_own_line_is_available_for_messages() {
        let (td, guard, layout) = setup();
        let user = td.path().join("CALC.BAS");
        std::fs::write(&user, b"\r\n  print# 1, x   ' here\r\n").unwrap();
        let mut p = Program::new("t");
        p.sources.push(SourceRef::new(&user));
        let s = stage(&guard, &layout, &p, &TranslateOptions::default()).unwrap();
        assert_eq!(s.user_line(0, 2).as_deref(), Some("  print# 1, x   ' here"));
        assert_eq!(s.user_line(0, 9), None);
        assert_eq!(s.user_line(3, 1), None);
    }

    #[test]
    fn a_file_that_is_not_a_program_is_refused_by_name() {
        let (td, guard, layout) = setup();
        let main = td.path().join("MAIN.BAS");
        let doc = td.path().join("NOTES.BAS");
        std::fs::write(&main, b"end\r\n").unwrap();
        std::fs::write(&doc, b"%PDF-1.4\n").unwrap();
        let mut p = Program::new("t");
        p.sources = vec![SourceRef::new(&main), SourceRef::new(&doc)];

        match stage(&guard, &layout, &p, &TranslateOptions::default()) {
            Err(EtbError::UnusableFile { problem, .. }) => assert_eq!(problem.name, "NOTES.BAS"),
            other => panic!("expected the PDF to be refused, got {other:?}"),
        }
    }

    #[test]
    fn an_include_is_found_beside_the_program_whatever_its_case() {
        let (td, guard, layout) = setup();
        let main = td.path().join("MAIN.BAS");
        std::fs::write(&main, b"$INCLUDE \"const\"\r\nPRINT pi\r\n").unwrap();
        std::fs::write(td.path().join("CONST.BAS"), b"pi = 3.14\r\n").unwrap();
        let mut p = Program::new("t");
        p.sources.push(SourceRef::new(&main));

        let s = stage(&guard, &layout, &p, &TranslateOptions::default()).unwrap();
        assert!(
            s.translation.findings.is_empty(),
            "{:?}",
            s.translation.findings
        );
        let staged = std::fs::read(&s.main).unwrap();
        assert!(staged.starts_with(b"'$INCLUDE:'etb_inc_1.bas'\r\n"));
        assert_eq!(
            std::fs::read(layout.src().join("etb_inc_1.bas")).unwrap(),
            b"pi = 3.14#\r\n"
        );
        assert_eq!(s.user_line(1, 1).as_deref(), Some("pi = 3.14"));
    }

    #[test]
    fn a_missing_include_is_refused_saying_where_it_was_looked_for() {
        let (td, guard, layout) = setup();
        let main = td.path().join("MAIN.BAS");
        std::fs::write(&main, b"$INCLUDE \"GONE.BAS\"\r\n").unwrap();
        let mut p = Program::new("t");
        p.sources.push(SourceRef::new(&main));
        let s = stage(&guard, &layout, &p, &TranslateOptions::default()).unwrap();
        assert!(s.translation.refused());
        assert_eq!(s.translation.findings[0].key, "tr.refuse.include_missing");
    }

    #[test]
    fn a_program_with_no_files_says_so() {
        let (_td, guard, layout) = setup();
        let p = Program::new("t");
        assert!(matches!(
            stage(&guard, &layout, &p, &TranslateOptions::default()),
            Err(EtbError::NoSources)
        ));
    }
}

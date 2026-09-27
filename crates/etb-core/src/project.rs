//! The user's data model: a "program" is a named list of Turbo Basic files.
//!
//! The first file is the program itself. Any others are files it pulls in with
//! `$INCLUDE`; listing them keeps every file the build reads in front of the
//! user, and lets an include be found even when it lives in another folder.

use crate::text;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use time::OffsetDateTime;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BuildOptions {
    /// Wait for a key before the program's window closes.
    ///
    /// On by default. Double-clicking the program in Explorer is what a person
    /// who has never used a terminal will do, and a window that closes the
    /// moment the results are printed looks like a program that did nothing.
    /// QB64 already shows "Press any key to continue" at `END`; turning this
    /// off makes the program close straight away instead.
    pub keep_window_open: bool,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            keep_window_open: true,
        }
    }
}

pub trait FileEntry {
    fn path(&self) -> &Path;
    fn from_path(path: PathBuf) -> Self;
}

/// Add a file to a list, refusing names the build cannot use and ignoring
/// duplicates. Returns whether the list changed.
fn add_file<T: FileEntry>(list: &mut Vec<T>, path: &Path) -> Result<bool, crate::error::EtbError> {
    let path = canonical_user_path(path);
    validate_user_path(&path)?;
    if list.iter().any(|e| e.path() == path) {
        return Ok(false);
    }
    list.push(T::from_path(path));
    Ok(true)
}

/// Point an existing entry at a new path, with the same checks as adding one.
fn relocate_file<T: FileEntry>(
    list: &mut [T],
    index: usize,
    path: &Path,
) -> Result<bool, crate::error::EtbError> {
    let path = canonical_user_path(path);
    validate_user_path(&path)?;
    match list.get_mut(index) {
        Some(e) => {
            *e = T::from_path(path);
            Ok(true)
        }
        None => Ok(false),
    }
}

fn remove_file<T>(list: &mut Vec<T>, index: usize) -> bool {
    if index < list.len() {
        list.remove(index);
        return true;
    }
    false
}

fn move_file<T>(list: &mut [T], index: usize, delta: isize) -> bool {
    let target = index as isize + delta;
    if index < list.len() && target >= 0 && (target as usize) < list.len() {
        list.swap(index, target as usize);
        return true;
    }
    false
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRef {
    pub path: PathBuf,
}

impl FileEntry for SourceRef {
    fn path(&self) -> &Path {
        &self.path
    }
    fn from_path(path: PathBuf) -> Self {
        Self { path }
    }
}

impl SourceRef {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
    pub fn display_name(&self) -> String {
        text::display_file_name(&self.path)
    }
    pub fn display_path(&self) -> String {
        text::display_path(&self.path)
    }
}

/// Transient, never serialized. The config is not a source of truth about what
/// exists on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileStatus {
    Present { len: u64 },
    Missing,
    Unreadable(String),
}

impl FileStatus {
    pub fn is_usable(&self) -> bool {
        matches!(self, FileStatus::Present { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Program {
    pub id: String,
    pub name: String,
    pub created: String,
    pub modified: String,
    /// The program first, then any files it includes.
    #[serde(rename = "source")]
    pub sources: Vec<SourceRef>,
    pub options: BuildOptions,
}

impl Default for Program {
    fn default() -> Self {
        let now = now_rfc3339();
        Self {
            id: uuid::Uuid::new_v4().simple().to_string(),
            name: String::new(),
            created: now.clone(),
            modified: now,
            sources: Vec::new(),
            options: BuildOptions::default(),
        }
    }
}

impl Program {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: text::nfc(&name.into()),
            ..Default::default()
        }
    }

    pub fn touch(&mut self) {
        self.modified = now_rfc3339();
    }

    /// The file that is the program, as opposed to the files it includes.
    pub fn main(&self) -> Option<&SourceRef> {
        self.sources.first()
    }

    /// Add a source file. Duplicates are ignored; unusable names are refused.
    pub fn add_source(&mut self, path: impl AsRef<Path>) -> Result<(), crate::error::EtbError> {
        if add_file(&mut self.sources, path.as_ref())? {
            self.touch();
        }
        Ok(())
    }

    pub fn remove_source(&mut self, index: usize) {
        if remove_file(&mut self.sources, index) {
            self.touch();
        }
    }

    /// Moving a file to the top makes it the program, so this is a real edit.
    pub fn move_source(&mut self, index: usize, delta: isize) {
        if move_file(&mut self.sources, index, delta) {
            self.touch();
        }
    }

    pub fn relocate_source(
        &mut self,
        index: usize,
        path: impl AsRef<Path>,
    ) -> Result<(), crate::error::EtbError> {
        if relocate_file(&mut self.sources, index, path.as_ref())? {
            self.touch();
        }
        Ok(())
    }

    /// Every distinct directory holding one of the user's files, in first-seen
    /// order: where an `$INCLUDE` is looked for when it is not in the list.
    /// Read-only access to directories the user already chose.
    pub fn include_dirs(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = Vec::new();
        for s in &self.sources {
            if let Some(dir) = s.path.parent() {
                if !out.iter().any(|d| d == dir) {
                    out.push(dir.to_path_buf());
                }
            }
        }
        out
    }
}

/// Names that could be mistaken for something other than a file name are
/// refused when they are added, rather than escaped later.
///
/// QB64 takes the source file as an argument, and a name starting with `-` is
/// read as one of its options.
pub fn validate_user_path(path: &Path) -> Result<(), crate::error::EtbError> {
    use crate::error::EtbError;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if name.is_empty() {
        return Err(EtbError::UnsafeSourceName {
            reason: "the path has no file name".into(),
        });
    }
    if name.starts_with('-') {
        return Err(EtbError::UnsafeSourceName {
            reason: format!("`{name}` starts with a dash, which a compiler reads as an option"),
        });
    }
    let s = path.to_string_lossy();
    if s.contains('\0') || s.contains('\n') || s.contains('\r') {
        return Err(EtbError::UnsafeSourceName {
            reason: "the path contains a control character".into(),
        });
    }
    Ok(())
}

/// Absolute, and simplified so no `\\?\` verbatim prefix ever reaches argv or the UI.
pub fn canonical_user_path(p: &Path) -> PathBuf {
    match dunce::canonicalize(p) {
        Ok(c) => c,
        Err(_) => {
            // The file may not exist (yet). Make it absolute without resolving.
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                std::env::current_dir().unwrap_or_default().join(p)
            }
        }
    }
}

/// A file name to offer in the save dialog for a program called `name`.
///
/// Keeps the user's own words, including Vietnamese, and only removes the
/// characters a filesystem will not accept. The platform executable suffix is
/// appended so the saved file is runnable on Windows.
pub fn suggested_program_file_name(name: &str) -> String {
    const ILLEGAL: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
    let cleaned: String = text::nfc(name)
        .chars()
        .map(|c| {
            if ILLEGAL.contains(&c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').trim();
    let stem = if cleaned.is_empty() {
        "program"
    } else {
        cleaned
    };
    format!("{stem}{}", std::env::consts::EXE_SUFFIX)
}

pub fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| String::from("1970-01-01T00:00:00Z"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dash_prefixed_names_are_refused() {
        let err = validate_user_path(Path::new("/tmp/-x.bas")).unwrap_err();
        assert!(matches!(
            err,
            crate::error::EtbError::UnsafeSourceName { .. }
        ));
    }

    #[test]
    fn ordinary_and_vietnamese_names_are_accepted() {
        validate_user_path(Path::new("/tmp/CALC.BAS")).unwrap();
        validate_user_path(Path::new("/tmp/Chương trình.bas")).unwrap();
    }

    #[test]
    fn the_first_file_is_the_program() {
        let mut p = Program::new("t");
        assert!(p.main().is_none());
        p.sources = vec![SourceRef::new("/a/MAIN.BAS"), SourceRef::new("/a/LIB.INC")];
        assert_eq!(p.main().unwrap().path, PathBuf::from("/a/MAIN.BAS"));
    }

    #[test]
    fn include_dirs_are_deduped_in_order() {
        let mut p = Program::new("t");
        p.sources = vec![
            SourceRef::new("/a/one.bas"),
            SourceRef::new("/b/two.inc"),
            SourceRef::new("/a/three.inc"),
        ];
        assert_eq!(
            p.include_dirs(),
            vec![PathBuf::from("/a"), PathBuf::from("/b")]
        );
    }

    #[test]
    fn relocating_a_file_is_validated_like_adding_one() {
        // "Locate file…" is the second way into the list. A name the build would
        // refuse must not be able to enter through it.
        let mut p = Program::new("t");
        p.sources = vec![SourceRef::new("/a/1.bas")];

        assert!(p.relocate_source(0, "/tmp/-x.bas").is_err());
        assert_eq!(
            p.sources[0].path,
            PathBuf::from("/a/1.bas"),
            "left unchanged"
        );

        p.relocate_source(0, "/tmp/CALC.BAS").unwrap();
        assert!(p.sources[0].path.ends_with("CALC.BAS"));
    }

    #[test]
    fn an_out_of_range_relocate_changes_nothing() {
        let mut p = Program::new("t");
        p.relocate_source(7, "/tmp/x.bas").unwrap();
        assert!(p.sources.is_empty());
    }

    #[test]
    fn source_order_is_preserved_and_movable() {
        let mut p = Program::new("t");
        p.sources = vec![SourceRef::new("/a/1.bas"), SourceRef::new("/a/2.bas")];
        p.move_source(0, 1);
        assert_eq!(p.sources[0].path, PathBuf::from("/a/2.bas"));
        p.move_source(0, -1); // out of range: no-op
        assert_eq!(p.sources[0].path, PathBuf::from("/a/2.bas"));
    }

    #[test]
    fn a_suggested_file_name_keeps_the_users_words_but_drops_illegal_characters() {
        let n = suggested_program_file_name("Tính dầm bê tông");
        assert!(n.starts_with("Tính dầm bê tông"), "got {n}");
        assert!(n.ends_with(std::env::consts::EXE_SUFFIX));

        let n = suggested_program_file_name("a/b:c*d?");
        assert!(!n.contains('/') && !n.contains(':') && !n.contains('*') && !n.contains('?'));
    }

    #[test]
    fn an_unnamed_program_still_gets_a_usable_file_name() {
        for input in ["", "   ", "...", "///"] {
            let n = suggested_program_file_name(input);
            assert!(!n.is_empty());
            assert!(
                n.starts_with("program") || n.starts_with('_'),
                "{input:?} -> {n:?}"
            );
            assert!(n.ends_with(std::env::consts::EXE_SUFFIX));
        }
    }

    #[test]
    fn the_window_waits_by_default() {
        assert!(BuildOptions::default().keep_window_open);
    }

    #[test]
    fn an_older_config_with_options_we_no_longer_have_still_loads() {
        // A program saved with options that have since gone must still open,
        // with the options we do have at their defaults.
        let p: Program = toml::from_str(
            "name = \"x\"\n[options]\nline_length = \"col72\"\nkeep_window_open = false\n",
        )
        .unwrap();
        assert!(!p.options.keep_window_open);
    }
}

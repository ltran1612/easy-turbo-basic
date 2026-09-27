//! Persistence. TOML, because one day I will be on the phone saying "open this file
//! in Notepad and read it to me".
//!
//! Settings and programs live in separate files so that a corrupt settings file can
//! never cost the user the user's program list.

use crate::error::{EtbError, Result};
use crate::fs_guard::{backup_path, with_suffix, FsGuard};
use crate::i18n::Lang;
use crate::paths::AppPaths;
use crate::project::Program;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const PROGRAMS_SCHEMA: u32 = 2;
pub const SETTINGS_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProgramsFile {
    pub schema_version: u32,
    #[serde(rename = "program")]
    pub programs: Vec<Program>,
}

impl Default for ProgramsFile {
    fn default() -> Self {
        Self {
            schema_version: PROGRAMS_SCHEMA,
            programs: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub schema_version: u32,
    pub language: Lang,
    pub zoom: f32,
    pub check_updates: bool,
    pub last_check_utc: Option<String>,
    pub toolchain_override: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA,
            language: Lang::from_locale(sys_locale_best().as_deref()),
            zoom: 1.15,
            check_updates: true,
            last_check_utc: None,
            toolchain_override: None,
        }
    }
}

fn sys_locale_best() -> Option<String> {
    // etb-core does not depend on sys-locale; the GUI passes the locale in when it
    // constructs Settings for the first time. Default here is Vietnamese.
    None
}

/// What happened while loading, so the UI can tell the user plainly rather than
/// silently starting empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadNote {
    /// No file yet — first run.
    Fresh,
    Loaded,
    /// The file would not parse; it was moved aside and the backup was used.
    RecoveredFromBackup {
        corrupt_saved_to: PathBuf,
    },
    /// The file would not parse and there was no usable backup.
    StartedEmpty {
        corrupt_saved_to: PathBuf,
    },
    /// Written by a newer version. We load nothing and refuse to overwrite it.
    TooNew {
        found: u32,
    },
}

pub struct Store {
    paths: AppPaths,
    guard: FsGuard,
    /// Set when the on-disk file came from a newer version: we must never clobber it.
    programs_readonly: bool,
    settings_readonly: bool,
}

impl Store {
    pub fn new(paths: AppPaths) -> Result<Self> {
        let guard = FsGuard::new(paths.write_roots().to_vec())?;
        Ok(Self {
            paths,
            guard,
            programs_readonly: false,
            settings_readonly: false,
        })
    }

    pub fn paths(&self) -> &AppPaths {
        &self.paths
    }
    pub fn guard(&self) -> &FsGuard {
        &self.guard
    }
    pub fn programs_readonly(&self) -> bool {
        self.programs_readonly
    }
    pub fn settings_readonly(&self) -> bool {
        self.settings_readonly
    }

    // ------------------------------------------------------------- programs

    pub fn load_programs(&mut self) -> Result<(Vec<Program>, LoadNote)> {
        let path = self.paths.programs_file();
        let (file, note) = self.load_with_recovery(&path, |b, p| self.parse_programs(b, p))?;
        self.programs_readonly = matches!(note, LoadNote::TooNew { .. });
        Ok((file.map(|f| f.programs).unwrap_or_default(), note))
    }

    /// Read a versioned config file, and survive it being unreadable.
    ///
    /// The same four outcomes for both files, which is the point of writing it
    /// once: there is no file yet; it parses; it was written by a newer version
    /// and must be left alone rather than overwritten; or it will not parse, in
    /// which case it is moved aside — never deleted, the user may want it back —
    /// and the backup `write_file_atomic` left behind is tried in its place.
    ///
    /// `None` means nothing was loaded and the caller should start from its own
    /// defaults. The `LoadNote` says which of the four happened, so the
    /// interface can tell the user rather than leaving them to notice.
    fn load_with_recovery<T>(
        &self,
        path: &Path,
        parse: impl Fn(&[u8], &Path) -> Result<T>,
    ) -> Result<(Option<T>, LoadNote)> {
        let Some(bytes) = self.guard.read_app_file(path)? else {
            return Ok((None, LoadNote::Fresh));
        };
        match parse(&bytes, path) {
            Ok(v) => Ok((Some(v), LoadNote::Loaded)),
            Err(EtbError::ConfigTooNew { found, .. }) => Ok((None, LoadNote::TooNew { found })),
            Err(_) => {
                let stamped = with_stamp(path, "corrupt");
                let _ = self.guard.rename_within_root(path, &stamped);
                let bak = backup_path(path);
                if let Some(bbytes) = self.guard.read_app_file(&bak)? {
                    if let Ok(v) = parse(&bbytes, &bak) {
                        return Ok((
                            Some(v),
                            LoadNote::RecoveredFromBackup {
                                corrupt_saved_to: stamped,
                            },
                        ));
                    }
                }
                Ok((
                    None,
                    LoadNote::StartedEmpty {
                        corrupt_saved_to: stamped,
                    },
                ))
            }
        }
    }

    fn parse_programs(&self, bytes: &[u8], path: &Path) -> Result<ProgramsFile> {
        let text = String::from_utf8_lossy(bytes);
        // Read the version before anything else, so a newer schema never reaches serde.
        let raw: toml::Value = toml::from_str(&text).map_err(|e| EtbError::ConfigParse {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;
        let found = raw
            .get("schema_version")
            .and_then(|v| v.as_integer())
            .unwrap_or(PROGRAMS_SCHEMA as i64) as u32;
        if found > PROGRAMS_SCHEMA {
            return Err(EtbError::ConfigTooNew {
                path: path.to_path_buf(),
                found,
                supported: PROGRAMS_SCHEMA,
            });
        }
        let file: ProgramsFile = raw.try_into().map_err(|e| EtbError::ConfigParse {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;
        Ok(migrate_programs(file, found))
    }

    pub fn save_programs(&self, programs: &[Program]) -> Result<()> {
        if self.programs_readonly {
            return Ok(()); // refuse to overwrite a newer-schema file
        }
        let file = ProgramsFile {
            schema_version: PROGRAMS_SCHEMA,
            programs: programs.to_vec(),
        };
        let text = toml::to_string_pretty(&file)?;
        self.guard
            .write_file_atomic(&self.paths.programs_file(), text.as_bytes())
    }

    // ------------------------------------------------------------- settings

    /// Load the settings, saying what happened rather than quietly starting over.
    ///
    /// This used to collapse every failure into `Settings::default()`. That is a
    /// worse outcome than it sounds: it does not lose one setting, it resets the
    /// interface language and the text size together, on the machine of someone
    /// who set both deliberately and has no way to know why they changed back.
    pub fn load_settings(&mut self) -> Result<(Settings, LoadNote)> {
        let path = self.paths.settings_file();
        let (settings, note) = self.load_with_recovery(&path, Self::parse_settings)?;
        self.settings_readonly = matches!(note, LoadNote::TooNew { .. });
        Ok((settings.unwrap_or_default(), note))
    }

    fn parse_settings(bytes: &[u8], path: &Path) -> Result<Settings> {
        let text = String::from_utf8_lossy(bytes);
        // The version first, so a newer file never reaches serde. `SETTINGS_SCHEMA`
        // was written into every settings file from the start and never read back,
        // which meant the field promised a compatibility check nothing performed.
        let raw: toml::Value = toml::from_str(&text).map_err(|e| EtbError::ConfigParse {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;
        let found = raw
            .get("schema_version")
            .and_then(|v| v.as_integer())
            .unwrap_or(SETTINGS_SCHEMA as i64) as u32;
        if found > SETTINGS_SCHEMA {
            return Err(EtbError::ConfigTooNew {
                path: path.to_path_buf(),
                found,
                supported: SETTINGS_SCHEMA,
            });
        }
        let s: Settings = raw.try_into().map_err(|e| EtbError::ConfigParse {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;
        Ok(migrate_settings(s, found))
    }

    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        if self.settings_readonly {
            return Ok(()); // refuse to overwrite a newer-schema file
        }
        let text = toml::to_string_pretty(s)?;
        self.guard
            .write_file_atomic(&self.paths.settings_file(), text.as_bytes())
    }
}

fn migrate_programs(mut f: ProgramsFile, from: u32) -> ProgramsFile {
    // 1 -> 2: `keep_window_open` became true by default.
    //
    // The field serialises unconditionally, so every program saved by 0.1.4 or
    // 0.1.5 carries an explicit `false` that the new default cannot reach. Those
    // two releases are the only ones that ever wrote it, and in both it was off
    // with no way to have chosen otherwise before saving -- so a stored `false`
    // from schema 1 records the old default, not a decision, and turning it on
    // overrides nobody.
    //
    // That reasoning expires here. A later migration must not assume the same:
    // from now on a stored value is a choice, and choices are kept.
    if from < 2 {
        for p in &mut f.programs {
            p.options.keep_window_open = true;
        }
    }
    ProgramsFile {
        schema_version: PROGRAMS_SCHEMA,
        ..f
    }
}

fn migrate_settings(s: Settings, _from: u32) -> Settings {
    // Only schema 1 exists so far. Future migrations chain here, the way
    // `migrate_programs` does — the point of reading the version at all.
    Settings {
        schema_version: SETTINGS_SCHEMA,
        ..s
    }
}

fn with_stamp(p: &Path, what: &str) -> PathBuf {
    let stamp = crate::project::now_rfc3339().replace([':', '-'], "");
    with_suffix(p, &format!(".{what}-{stamp}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{BuildOptions, SourceRef};

    fn store() -> (tempfile::TempDir, Store) {
        let td = tempfile::tempdir().unwrap();
        let s = Store::new(AppPaths::under(td.path())).unwrap();
        (td, s)
    }

    #[test]
    fn first_run_is_fresh_and_empty() {
        let (_td, mut s) = store();
        let (p, note) = s.load_programs().unwrap();
        assert!(p.is_empty());
        assert_eq!(note, LoadNote::Fresh);
    }

    #[test]
    fn programs_round_trip_with_vietnamese_names_and_paths() {
        let (_td, mut s) = store();
        let mut p = Program::new("Tính dầm bê tông");
        p.sources = vec![SourceRef::new("/home/Nguyễn Văn A/BASIC/TINHCOC.BAS")];
        p.options.keep_window_open = false;
        s.save_programs(&[p.clone()]).unwrap();

        let (back, note) = s.load_programs().unwrap();
        assert_eq!(note, LoadNote::Loaded);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].name, "Tính dầm bê tông");
        assert_eq!(back[0].sources[0].path, p.sources[0].path);
        assert!(!back[0].options.keep_window_open);
    }

    #[test]
    fn unknown_fields_are_tolerated_for_forward_compatibility() {
        let (td, mut s) = store();
        let path = AppPaths::under(td.path()).programs_file();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "schema_version = 1\n\n[[program]]\nid = \"abc\"\nname = \"X\"\nfuture_field = 42\n",
        )
        .unwrap();
        let (back, note) = s.load_programs().unwrap();
        assert_eq!(note, LoadNote::Loaded);
        assert_eq!(back[0].name, "X");
    }

    #[test]
    fn schema_1_gets_the_window_kept_open_but_a_later_choice_is_kept() {
        // The 1 -> 2 migration. A program saved by 0.1.4 or 0.1.5 carries an
        // explicit `keep_window_open = false` that is the old default rather
        // than a decision, so it is turned on.
        let (td, mut s) = store();
        let path = AppPaths::under(td.path()).programs_file();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let old = "schema_version = 1\n\n[[program]]\nid = \"abc\"\nname = \"X\"\n\n\
                   [program.options]\nkeep_window_open = false\n";
        std::fs::write(&path, old).unwrap();
        let (back, _) = s.load_programs().unwrap();
        assert!(
            back[0].options.keep_window_open,
            "a schema 1 program should come back with the window kept open"
        );

        // And the migration stops there: once the file says schema 2, `false`
        // means someone turned it off, and loading must not undo that.
        let mine = old.replace("schema_version = 1", "schema_version = 2");
        std::fs::write(&path, mine).unwrap();
        let (back, _) = s.load_programs().unwrap();
        assert!(
            !back[0].options.keep_window_open,
            "a choice recorded under schema 2 must survive being loaded"
        );
    }

    #[test]
    fn unreadable_settings_fall_back_to_the_backup_and_say_so() {
        // Previously any parse failure here returned `Settings::default()` with
        // nothing said, which resets the language and the text size together.
        let (_td, mut s) = store();
        s.save_settings(&Settings {
            language: Lang::En,
            zoom: 1.5,
            ..Default::default()
        })
        .unwrap();
        // A second save is what rotates the first copy to `.bak`.
        s.save_settings(&Settings {
            language: Lang::En,
            zoom: 1.75,
            ..Default::default()
        })
        .unwrap();

        std::fs::write(s.paths().settings_file(), "not toml {{{").unwrap();
        let (back, note) = s.load_settings().unwrap();
        match note {
            LoadNote::RecoveredFromBackup { corrupt_saved_to } => {
                assert!(
                    corrupt_saved_to.exists(),
                    "the unreadable file must be kept"
                );
                assert_eq!(back.language, Lang::En, "the language must come back");
                assert!((back.zoom - 1.5).abs() < f32::EPSILON, "got {}", back.zoom);
            }
            other => panic!("expected recovery from backup, got {other:?}"),
        }
    }

    #[test]
    fn settings_from_a_newer_version_are_reported_and_left_alone() {
        // The version field was written into every settings file from the start
        // and never read back. Now it means something.
        let (td, mut s) = store();
        let path = AppPaths::under(td.path()).settings_file();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let written = "schema_version = 99\nlanguage = \"en\"\n";
        std::fs::write(&path, written).unwrap();

        let (back, note) = s.load_settings().unwrap();
        assert!(
            matches!(note, LoadNote::TooNew { found: 99 }),
            "got {note:?}"
        );
        assert_eq!(back.language, Lang::default(), "nothing should be loaded");

        // And saving must not overwrite it.
        s.save_settings(&Settings::default()).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            written,
            "a newer settings file must survive a save"
        );
    }

    #[test]
    fn a_newer_schema_is_never_overwritten() {
        let (td, mut s) = store();
        let path = AppPaths::under(td.path()).programs_file();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "schema_version = 99\n").unwrap();

        let (back, note) = s.load_programs().unwrap();
        assert!(back.is_empty());
        assert_eq!(note, LoadNote::TooNew { found: 99 });
        assert!(s.programs_readonly());

        s.save_programs(&[Program::new("nope")]).unwrap();
        let on_disk = std::fs::read_to_string(&path).unwrap();
        assert!(on_disk.contains("99"), "the newer file was clobbered");
    }

    #[test]
    fn a_corrupt_file_is_moved_aside_and_the_backup_is_used() {
        let (_td, mut s) = store();
        s.save_programs(&[Program::new("good")]).unwrap();
        s.save_programs(&[Program::new("newer good")]).unwrap(); // creates .bak

        let path = s.paths().programs_file();
        std::fs::write(&path, "this is not toml {{{").unwrap();

        let (back, note) = s.load_programs().unwrap();
        match note {
            LoadNote::RecoveredFromBackup { corrupt_saved_to } => {
                assert!(corrupt_saved_to.exists(), "the corrupt file must be kept");
                assert_eq!(back.len(), 1);
                assert_eq!(back[0].name, "good");
            }
            other => panic!("expected recovery from backup, got {other:?}"),
        }
    }

    #[test]
    fn a_setting_that_no_longer_exists_does_not_cost_the_user_the_others() {
        // `create_launcher` was removed once the program learned to wait for a
        // key by itself. Every settings file written before that still names it.
        //
        // This matters more than it sounds: `load_settings` falls back to
        // `Settings::default()` on any parse failure, so a field serde refused
        // to ignore would not just drop that one setting -- it would silently
        // reset the language back to the locale guess and the zoom back to
        // 1.15, on a machine where someone had deliberately set both.
        let (td, mut s) = store();
        let path = AppPaths::under(td.path()).settings_file();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "schema_version = 1\nlanguage = \"en\"\nzoom = 1.5\ncreate_launcher = true\n",
        )
        .unwrap();

        let (back, _) = s.load_settings().unwrap();
        assert_eq!(back.language, Lang::En, "the language must survive");
        assert!(
            (back.zoom - 1.5).abs() < f32::EPSILON,
            "the zoom must survive, got {}",
            back.zoom
        );
    }

    #[test]
    fn settings_round_trip() {
        let (_td, mut s) = store();
        let st = Settings {
            language: Lang::En,
            zoom: 1.5,
            ..Default::default()
        };
        s.save_settings(&st).unwrap();
        let (back, _) = s.load_settings().unwrap();
        assert_eq!(back.language, Lang::En);
        assert!((back.zoom - 1.5).abs() < f32::EPSILON);
    }

    #[test]
    fn build_options_defaults_survive_an_empty_table() {
        let f: ProgramsFile =
            toml::from_str("schema_version = 1\n[[program]]\nname = \"x\"\n").unwrap();
        assert_eq!(f.programs[0].options, BuildOptions::default());
    }
}

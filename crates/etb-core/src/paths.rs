//! Every path in the application derives from `AppPaths`, resolved once at startup.

use crate::error::{EtbError, Result};
use directories::ProjectDirs;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Reverse-DNS qualifier. Changing this moves every user's saved data, so it is
/// fixed for the life of the product: changing it strands every saved setting
/// and program list in a directory the application no longer looks in.
pub const QUALIFIER: &str = "io.github";
pub const ORGANIZATION: &str = "easy-turbo-basic";
pub const APPLICATION: &str = "Easy Turbo Basic";

#[derive(Debug, Clone)]
pub struct AppPaths {
    config_dir: PathBuf,
    data_dir: PathBuf,
    cache_dir: PathBuf,
    /// Scratch space for builds. Usually under `data_dir`, but not always — see
    /// `choose_ascii_root`.
    work_root: PathBuf,
    /// Where a copy of the compiler goes when its own path cannot be given to
    /// it. Same reasoning as `work_root`.
    tool_root: PathBuf,
    /// Root of everything we are allowed to write. All the dirs above live under
    /// their platform locations, so the write root is checked per-directory.
    write_roots: Vec<PathBuf>,
    session_id: String,
}

impl AppPaths {
    pub fn discover() -> Result<Self> {
        let pd = ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION)
            .ok_or(EtbError::NoProjectDirs)?;
        Ok(Self::from_dirs(
            pd.config_dir().to_path_buf(),
            pd.data_dir().to_path_buf(),
            pd.cache_dir().to_path_buf(),
        ))
    }

    /// Used by tests and by `ETB_HOME` so an integration test can point the whole
    /// application at a scratch directory.
    pub fn under(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref();
        Self::from_dirs(root.join("config"), root.join("data"), root.join("cache"))
    }

    /// Honours `ETB_HOME` when set; otherwise the platform directories.
    pub fn resolve() -> Result<Self> {
        match std::env::var_os("ETB_HOME") {
            Some(h) if !h.is_empty() => Ok(Self::under(PathBuf::from(h))),
            _ => Self::discover(),
        }
    }

    fn from_dirs(config_dir: PathBuf, data_dir: PathBuf, cache_dir: PathBuf) -> Self {
        let base = system_scratch_base();
        // Only needed when the scratch has to leave the user's own folder; the
        // fallback keeps the old name rather than failing, because a worse
        // directory name is not worth refusing to start over.
        let secret = crate::fs_guard::stable_secret(&config_dir).unwrap_or_else(|| {
            format!(
                "{:x}",
                Sha256::digest(data_dir.to_string_lossy().as_bytes())
            )[..32]
                .to_string()
        });
        let work_root = choose_ascii_root(&data_dir, base.as_deref(), &secret, "work");
        let tool_root = choose_ascii_root(&data_dir, base.as_deref(), &secret, "tools");
        let write_roots = vec![
            config_dir.clone(),
            data_dir.clone(),
            cache_dir.clone(),
            work_root.clone(),
            tool_root.clone(),
        ];
        Self {
            config_dir,
            data_dir,
            cache_dir,
            work_root,
            tool_root,
            write_roots,
            session_id: uuid::Uuid::new_v4().simple().to_string(),
        }
    }

    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }
    pub fn write_roots(&self) -> &[PathBuf] {
        &self.write_roots
    }

    pub fn programs_file(&self) -> PathBuf {
        self.config_dir.join("programs.toml")
    }
    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join("settings.toml")
    }
    pub fn log_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }
    pub fn work_root(&self) -> PathBuf {
        self.work_root.clone()
    }
    /// Where a usable copy of the compiler is kept, when one is needed.
    pub fn tool_root(&self) -> PathBuf {
        self.tool_root.clone()
    }
    /// Where the file locks live: ours, in the user's own directory. Never a
    /// shared temp directory — see `fs_guard::lock_toolchain`.
    pub fn lock_dir(&self) -> PathBuf {
        self.data_dir.join("locks")
    }
    pub fn session_work_dir(&self) -> PathBuf {
        self.work_root().join(&self.session_id)
    }
    pub fn build_dir(&self, n: u64) -> PathBuf {
        self.session_work_dir().join(format!("build-{n}"))
    }
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
}

/// Where a build's scratch tree — or a copy of the compiler — goes.
///
/// Normally under the application's own data directory. On Windows that sits in
/// the user's profile, and MinGW's driver hands paths to `as.exe` through the
/// system ANSI codepage — so a profile named `Nguyễn Văn A` arrives at the
/// assembler as `Nguy?n Van A` and it reports `Invalid argument` trying to open
/// its own temp file. Nothing about the build is wrong; the path cannot survive
/// the trip.
///
/// So when the natural location is not pure ASCII, it moves somewhere that is.
/// Only what a compiler has to open by name: the program list, the user's
/// sources and the saved program are read and written by us rather than by the
/// compiler, and Rust handles Windows paths as UTF-16 throughout, so those keep
/// their real locations and their real names.
///
/// Taking `base` as an argument rather than reading the environment keeps this a
/// pure function, so both branches are testable on a machine that is neither.
fn choose_ascii_root(data_dir: &Path, base: Option<&Path>, secret: &str, what: &str) -> PathBuf {
    let natural = data_dir.join(what);
    if natural.to_string_lossy().is_ascii() {
        return natural;
    }
    let Some(base) = base else {
        // Nowhere better to go. On Unix this is the normal answer: paths are
        // bytes there and the toolchain passes them through unharmed.
        return natural;
    };
    // `%ProgramData%` is shared: every account on the machine can create
    // directories under it, and whoever creates one owns it and decides what
    // may go in it. So the name is the user's own secret and not a hash of
    // their profile path — which anyone who knows the account name could work
    // out, create first, and then own. See `fs_guard::stable_secret`.
    //
    // Hex, so the name is ASCII by construction, which is the whole reason
    // for coming here.
    base.join(format!("EasyTurboBasic-{secret}")).join(what)
}

/// A machine-wide location whose path is ASCII, or `None` where the problem does
/// not arise.
fn system_scratch_base() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        // %ProgramData% is ASCII on every Windows install and writable by a
        // standard user for directories it creates itself, so this needs no
        // elevation.
        std::env::var_os("ProgramData")
            .map(PathBuf::from)
            .filter(|p| p.to_string_lossy().is_ascii())
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Layout of one build's working tree. Nothing outside this is ever written.
#[derive(Debug, Clone)]
pub struct WorkLayout {
    pub root: PathBuf,
}

impl WorkLayout {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
    pub fn src(&self) -> PathBuf {
        self.root.join("src")
    }
    pub fn tmp(&self) -> PathBuf {
        self.root.join("tmp")
    }
    pub fn out(&self) -> PathBuf {
        self.root.join("out")
    }
    /// The built program, for a target whose executables carry `suffix`.
    ///
    /// The suffix comes from the toolchain's bundle, not from the host: a
    /// Windows bundle produces `program.exe` wherever it is driven from.
    pub fn exe_with_suffix(&self, suffix: &str) -> PathBuf {
        self.out().join(format!("program{suffix}"))
    }

    /// The built program for a toolchain targeting this machine.
    pub fn exe(&self) -> PathBuf {
        self.exe_with_suffix(std::env::consts::EXE_SUFFIX)
    }
    pub fn all_dirs(&self) -> Vec<PathBuf> {
        vec![self.src(), self.tmp(), self.out()]
    }
}

#[cfg(test)]
mod work_root_tests {
    use super::*;

    const ASCII_BASE: &str = "/ProgramData";
    /// Stands in for `fs_guard::stable_secret`: this user's, and nobody's guess.
    const SECRET: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn an_ascii_data_directory_keeps_its_work_tree_where_it_is() {
        let d = PathBuf::from("/home/user/.local/share/easyturbobasic");
        assert_eq!(
            choose_ascii_root(&d, Some(Path::new(ASCII_BASE)), SECRET, "work"),
            d.join("work")
        );
    }

    #[test]
    fn a_non_ascii_data_directory_moves_its_work_tree_somewhere_ascii() {
        // The real case: a Windows profile the compiler's ANSI round-trip mangles.
        let d = PathBuf::from(r"C:\Users\Nguyễn Văn A\AppData\Local\easyturbobasic");
        let w = choose_ascii_root(&d, Some(Path::new(ASCII_BASE)), SECRET, "work");

        assert!(
            w.to_string_lossy().is_ascii(),
            "the whole scratch path must survive the codepage, got {}",
            w.display()
        );
        assert!(!w.starts_with(&d), "it has to leave the profile entirely");
        assert!(w.starts_with(ASCII_BASE));
    }

    #[test]
    fn the_name_in_a_shared_place_is_the_secret_and_not_the_profile() {
        // %ProgramData% is shared, and whoever creates a directory there owns
        // it. A name worked out from the account would let somebody else
        // create it first; this one cannot be worked out at all.
        let d = Path::new(r"C:\Users\Nguyễn Văn A\AppData\Local\TenRieng");
        let w = choose_ascii_root(d, Some(Path::new(ASCII_BASE)), SECRET, "work");
        let shown = w.to_string_lossy();
        assert!(shown.contains(SECRET), "{shown}");
        assert!(
            !shown.contains("Nguy") && !shown.contains("TenRieng"),
            "nothing about the account may be readable from it: {shown}"
        );
    }

    #[test]
    fn two_accounts_on_one_machine_do_not_share_a_scratch_tree() {
        let base = Some(Path::new(ASCII_BASE));
        let d = Path::new(r"C:\Users\Nguyễn Văn A\AppData\Local\ef");
        let a = choose_ascii_root(d, base, SECRET, "work");
        let b = choose_ascii_root(d, base, "ffffffffffffffffffffffffffffffff", "work");
        assert_ne!(a, b, "different users, different secrets, different trees");
    }

    #[test]
    fn the_same_account_gets_the_same_scratch_tree_every_time() {
        // Otherwise every launch would strand the last one's build directories.
        let d = Path::new(r"C:\Users\Nguyễn Văn A\AppData\Local\ef");
        let base = Some(Path::new(ASCII_BASE));
        assert_eq!(
            choose_ascii_root(d, base, SECRET, "work"),
            choose_ascii_root(d, base, SECRET, "work")
        );
    }

    #[test]
    fn work_and_tools_are_different_trees() {
        let d = Path::new(r"C:\Users\Nguyễn Văn A\AppData\Local\ef");
        let base = Some(Path::new(ASCII_BASE));
        assert_ne!(
            choose_ascii_root(d, base, SECRET, "work"),
            choose_ascii_root(d, base, SECRET, "tools")
        );
    }
}

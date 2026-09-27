//! Locating and describing QB64 Phoenix Edition.
//!
//! One code path serves both cases: on Windows a pruned QB64-PE, with the C++
//! compiler it drives, ships beside the executable; on Linux (development and
//! testing only) QB64-PE is built from its pinned source and drives the
//! system's C++ compiler. The difference is discovery and verification, not
//! behaviour.
//!
//! QB64-PE is itself a translator: it turns BASIC into C++ and then runs
//! `make` and a C++ compiler over that. So the environment it runs in matters
//! twice over, and it is built from nothing rather than inherited.

pub mod bundle;
pub mod manifest;

use crate::error::{EtbError, Result};
use crate::fs_guard::FsGuard;
use crate::paths::WorkLayout;
use bundle::Bundle;
use manifest::Manifest;
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolchainKind {
    /// Shipped inside our own installation directory.
    Bundled,
    /// Found on PATH. Development and testing.
    System,
    /// Pointed at explicitly by settings or `ETB_TOOLCHAIN`.
    UserSpecified,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolchainId {
    pub kind: ToolchainKind,
    pub version: String,
    pub path: PathBuf,
}

impl ToolchainId {
    /// A short label for the UI: "FreeBASIC 1.10.1".
    pub fn display(&self) -> String {
        if self.version.is_empty() {
            crate::text::display_path(&self.path)
        } else {
            format!("FreeBASIC {}", self.version)
        }
    }
}

#[derive(Debug, Clone)]
pub struct Toolchain {
    id: ToolchainId,
    fbc: PathBuf,
    /// Present only for a bundled toolchain.
    bundle: Option<Bundle>,
    /// Set when this is a copy made to be usable: where the original is, which
    /// is the one we are answerable for and the one integrity is about.
    origin: Option<PathBuf>,
    /// Found beside our own executable, rather than pointed at by a setting or
    /// an environment variable.
    ///
    /// This is what distinguishes the compiler we shipped — which we are
    /// answerable for and must be able to attest to — from one a developer or a
    /// test aimed us at, which we can say nothing about and do not pretend to.
    shipped: bool,
}

impl Toolchain {
    pub fn id(&self) -> &ToolchainId {
        &self.id
    }
    pub fn fbc(&self) -> &Path {
        &self.fbc
    }

    /// The directory QB64-PE runs in: its own. It finds its `internal` folder,
    /// and writes its temporary C++ there, relative to it.
    pub fn home(&self) -> &Path {
        self.fbc.parent().unwrap_or(Path::new("."))
    }

    /// The suffix executables get for the bundle's *target*.
    ///
    /// The host's suffix is only right when the bundle targets the host. A
    /// Windows bundle driven from Linux still produces `program.exe`, and looking
    /// for `program` would report a failure on a build that worked.
    pub fn exe_suffix(&self) -> &str {
        self.bundle
            .as_ref()
            .and_then(|b| b.exe_suffix.as_deref())
            .unwrap_or(std::env::consts::EXE_SUFFIX)
    }

    /// Program that runs the bundle's binaries, when they are not native.
    pub fn launcher(&self) -> Option<&str> {
        self.bundle.as_ref().and_then(|b| b.launcher.as_deref())
    }

    /// A command that runs one of the bundle's binaries, through the launcher
    /// when there is one.
    pub fn run_binary(&self, exe: &Path) -> Command {
        match self.launcher() {
            Some(l) => {
                let mut c = Command::new(l);
                c.arg(exe);
                c
            }
            None => Command::new(exe),
        }
    }

    /// A command for QB64-PE, with a scrubbed environment, running in its own
    /// directory.
    pub fn command(&self, layout: &WorkLayout) -> Command {
        let mut cmd = self.run_binary(&self.fbc);
        cmd.env_clear();
        for (k, v) in self.compile_env(layout) {
            cmd.env(k, v);
        }
        cmd.current_dir(self.home());
        cmd
    }

    /// The whole installation: the bundle's root, or the directory the
    /// compiler sits in when there is no descriptor.
    pub fn root(&self) -> &Path {
        self.bundle
            .as_ref()
            .map_or_else(|| self.home(), |b| &b.root)
    }

    /// Can QB64-PE be run where it is?
    ///
    /// On Windows it receives its arguments — including the path to its own
    /// `internal` folder and to the program it is building — through the ANSI
    /// code page, which has no Vietnamese. Installed under
    /// `C:\Users\Nguyễn Văn A\…`, it is handed `Nguy?n Van A` and cannot find
    /// its own files. Nothing is wrong with the installation; the path cannot
    /// survive the trip. See `docs/verification.md`, F3.
    pub fn usable_in_place(&self) -> bool {
        self.root().to_string_lossy().is_ascii()
    }

    /// The compiler as it can actually be run: itself, or a copy of it at a
    /// path it can be given.
    ///
    /// The copy is made once, into `tool_root` (an ASCII path — see
    /// `paths::AppPaths::tool_root`), and reused from then on: it is stamped
    /// with where it came from, that compiler's version and date, and the
    /// manifest it was made against, and remade when any of those differ. The
    /// original is left alone, still attested; `verify_integrity` goes on
    /// checking it, because that is the one we shipped.
    ///
    /// `Ok(None)` means `cancel` was set while copying.
    pub fn prepare(
        &self,
        guard: &FsGuard,
        tool_root: &Path,
        lock_dir: &Path,
        manifest: &Manifest,
        cancel: &AtomicBool,
    ) -> Result<Option<Self>> {
        if self.usable_in_place() {
            return Ok(Some(self.clone()));
        }
        let from = self.root().to_path_buf();
        let key = format!("{:x}", Sha256::digest(from.to_string_lossy().as_bytes()));
        let dir = tool_root.join(&key[..16]);
        let stamp = dir.join(".etb-copy");
        let want = self.copy_stamp(manifest);

        let rel = self.fbc.strip_prefix(&from).unwrap_or(&self.fbc);
        let copied_driver = dir.join(rel);

        // The same lock a build takes, and on the same directory, so the tree
        // is never remade under a build already running from it, and two
        // applications starting at once make the copy once between them.
        let home = copied_driver.parent().unwrap_or(&dir).to_path_buf();
        let Some(_lock) = crate::fs_guard::lock_toolchain(lock_dir, &home, cancel)? else {
            return Ok(None);
        };

        // Reused only if it is still the compiler we copied — by its hashes,
        // not by a note next to it.
        //
        // This directory can be a shared one (`%ProgramData%`, when the user's
        // own folder is at a path the compiler cannot be given). The stamp
        // says where the copy came from and what it was, all of which another
        // account on the machine could work out from a public release and
        // write down; the manifest is the thing they cannot forge, because it
        // is inside this application and lists what every file must hash to.
        //
        // With no manifest — a development build — there is nothing to check
        // against and the stamp is all there is.
        let reusable = copied_driver.is_file()
            && guard
                .read_app_file(&stamp)?
                .is_some_and(|got| got == want.as_bytes())
            && (manifest.is_empty() || manifest.verify(&dir).is_ok());
        if !reusable {
            if dir.exists() {
                guard.remove_dir_all(&dir)?;
            }
            if guard.materialise_tree(&from, &dir, cancel)?.is_none() {
                return Ok(None);
            }
            guard.write_file(&stamp, want.as_bytes())?;
        }

        let bundle = self.bundle.as_ref().map(|b| b.rebased(&dir));
        Ok(Some(Self {
            id: ToolchainId {
                path: copied_driver.clone(),
                ..self.id.clone()
            },
            fbc: copied_driver,
            bundle,
            shipped: self.shipped,
            origin: Some(from),
        }))
    }

    /// What a copy is stamped with: where it came from, which compiler it was,
    /// and the manifest it was made against.
    fn copy_stamp(&self, manifest: &Manifest) -> String {
        let (len, when) = std::fs::metadata(&self.fbc)
            .map(|m| {
                let secs = m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs());
                (m.len(), secs)
            })
            .unwrap_or((0, 0));
        format!(
            "from: {}\nversion: {}\ndriver: {len}:{when}\nmanifest: {}\n",
            self.root().display(),
            self.id.version,
            manifest.digest()
        )
    }

    /// Where this copy came from, if it is one.
    pub fn origin(&self) -> Option<&Path> {
        self.origin.as_deref()
    }

    /// Check the bundled files against the manifest embedded in the application.
    ///
    /// A system toolchain is not ours and has nothing to attest to, so it always
    /// verifies; so does a bundle built before a manifest was generated, which is
    /// what a development checkout has.
    pub fn verify_integrity(&self, manifest: &manifest::Manifest) -> manifest::VerifyReport {
        // Always the installation, never a copy of it: the installation is
        // what was shipped, and what antivirus takes bites out of.
        let attested = self
            .origin
            .clone()
            .unwrap_or_else(|| self.root().to_path_buf());
        match (&self.bundle, manifest.is_empty()) {
            (Some(_), false) => manifest.verify(&attested),
            // A compiler we shipped, and nothing to check it against. That is
            // not a pass: the manifest is written by the toolchain fetch and
            // baked in at compile time, so an empty one here means this binary
            // was built before its own compiler was fetched and cannot tell
            // whether the compiler beside it is the one we meant to ship.
            (Some(_), true) if self.shipped => manifest::VerifyReport {
                unattested: true,
                ..Default::default()
            },
            // A bundle someone pointed us at, or no bundle at all. Neither is
            // ours to vouch for.
            _ => manifest::VerifyReport::default(),
        }
    }

    /// Environment for QB64-PE and everything it runs.
    pub fn compile_env(&self, layout: &WorkLayout) -> Vec<(OsString, OsString)> {
        scrubbed_env(self.bundle.as_ref(), &layout.tmp())
    }

    /// Construct directly from a path to `fbc`. Used by discovery and by
    /// tests that inject a fake compiler.
    pub fn from_path(path: PathBuf, kind: ToolchainKind) -> Result<Self> {
        if !path.exists() {
            return Err(EtbError::ToolchainInvalid {
                path: path.clone(),
                reason: "the file does not exist".into(),
            });
        }
        let version = query_version(&path, None).unwrap_or_default();
        Ok(Self {
            id: ToolchainId {
                kind,
                version,
                path: path.clone(),
            },
            fbc: path,
            shipped: false,
            origin: None,
            bundle: None,
        })
    }

    /// Load a bundled toolchain from its root directory, honouring its
    /// `bundle.toml` if it ships one.
    pub fn from_bundle(root: &Path) -> Result<Self> {
        let b = match Bundle::load(root)? {
            Some(b) => b,
            // No descriptor is fine: a plain QB64-PE tree needs none.
            None => Bundle::resolve(root, bundle::BundleDescriptor::default())?,
        };
        if !b.fbc.is_file() {
            return Err(EtbError::ToolchainInvalid {
                path: b.fbc.clone(),
                reason: "the bundled compiler is missing (antivirus may have removed it)".into(),
            });
        }
        let version = if b.version.is_empty() {
            query_version(&b.fbc, b.launcher.as_deref()).unwrap_or_default()
        } else {
            b.version.clone()
        };
        Ok(Self {
            id: ToolchainId {
                kind: ToolchainKind::Bundled,
                version,
                path: b.fbc.clone(),
            },
            fbc: b.fbc.clone(),
            shipped: false,
            origin: None,
            bundle: Some(b),
        })
    }
}

/// Variables that must never reach QB64-PE or what it runs, because they
/// inject search paths or settings into a build. They are absent by
/// construction after `env_clear`, and a bundle descriptor is not allowed to
/// put them back.
///
/// The second half is `make`'s: it reads every environment variable as the
/// default for the Makefile variable of the same name, so `CXXFLAGS`, `OS` or
/// `MAKEFLAGS` left in the environment would silently change how the user's
/// program is compiled. Windows sets `OS` in every environment by default.
pub const BANNED_ENV: &[&str] = &[
    "LIBRARY_PATH",
    "CPATH",
    "C_INCLUDE_PATH",
    "CPLUS_INCLUDE_PATH",
    "OBJC_INCLUDE_PATH",
    "LD_LIBRARY_PATH",
    "LD_PRELOAD",
    "GCC_EXEC_PREFIX",
    "COMPILER_PATH",
    "CC",
    "CXX",
    "AR",
    "CFLAGS",
    "CXXFLAGS",
    "CPPFLAGS",
    "LDFLAGS",
    "MAKEFLAGS",
    "MFLAGS",
    "GNUMAKEFLAGS",
    "MAKEFILES",
    "MAKELEVEL",
    "OS",
    "BUILD_QB64",
];

/// Build the scrubbed environment.
fn scrubbed_env(bundle: Option<&Bundle>, tmp: &Path) -> Vec<(OsString, OsString)> {
    let mut env: Vec<(OsString, OsString)> = Vec::new();

    let mut path = OsString::new();
    let push_dir = |p: &Path, path: &mut OsString| {
        path.push(p);
        path.push(PATH_SEP);
    };
    if let Some(b) = bundle {
        for dir in &b.path_dirs {
            push_dir(dir, &mut path);
        }
    }
    #[cfg(windows)]
    {
        let sysroot =
            std::env::var_os("SystemRoot").unwrap_or_else(|| OsString::from(r"C:\Windows"));
        let mut s32 = PathBuf::from(&sysroot);
        s32.push("System32");
        path.push(&s32);
        env.push(("SystemRoot".into(), sysroot.clone()));
        env.push(("windir".into(), sysroot));
        // QB64-PE runs `make` through the command interpreter, and finds it
        // by this name. Passed through from our own environment, as the
        // system root is; Windows always sets it.
        if let Some(interp) = std::env::var_os("ComSpec") {
            env.push(("ComSpec".into(), interp));
        }
        env.push(("TMP".into(), tmp.as_os_str().to_os_string()));
        env.push(("TEMP".into(), tmp.as_os_str().to_os_string()));
    }
    #[cfg(not(windows))]
    {
        // A Linux QB64-PE drives the system's make and C++ compiler.
        path.push("/usr/local/bin:/usr/bin:/bin");
        env.push(("LANG".into(), OsString::from("C.UTF-8")));
        env.push(("LC_ALL".into(), OsString::from("C.UTF-8")));
        if let Some(home) = std::env::var_os("HOME") {
            env.push(("HOME".into(), home));
        }
    }
    env.push(("PATH".into(), path));
    env.push(("TMPDIR".into(), tmp.as_os_str().to_os_string()));

    if let Some(b) = bundle {
        // Everything set above is ours. A bundle may add variables; it may never
        // override them, or it could undo the PATH scrubbing or the
        // temp-directory containment.
        let ours: Vec<String> = env
            .iter()
            .map(|(k, _)| k.to_string_lossy().to_string())
            .collect();
        for (k, v) in &b.env {
            let name = k.to_string_lossy().to_string();
            let banned = BANNED_ENV.iter().any(|b| name.eq_ignore_ascii_case(b));
            let ours_already = ours.iter().any(|o| o.eq_ignore_ascii_case(&name));
            if banned || ours_already {
                tracing::warn!("ignoring `{name}` from bundle.toml: it is not a bundle's to set");
                continue;
            }
            env.push((k.clone(), v.clone()));
        }
    }
    env
}

#[cfg(windows)]
const PATH_SEP: &str = ";";
#[cfg(not(windows))]
const PATH_SEP: &str = ":";

/// `fbc -v` prints `QB64-PE Compiler V4.6.0`.
fn query_version(fbc: &Path, launcher: Option<&str>) -> Option<String> {
    let mut cmd = match launcher {
        Some(l) => {
            let mut c = Command::new(l);
            c.arg(fbc);
            c
        }
        None => Command::new(fbc),
    };
    if let Some(dir) = fbc.parent() {
        cmd.current_dir(dir);
    }
    let out = cmd.arg("--version").output().ok()?;
    parse_version(&String::from_utf8_lossy(&out.stdout))
}

/// `FreeBASIC Compiler - Version 1.10.1 (2023-12-24), built for linux-x86_64`
/// → `1.10.1`. The line also arrives with a `Version` that has no number after
/// it in some builds, so the digits are checked rather than assumed.
fn parse_version(text: &str) -> Option<String> {
    let line = text.lines().find(|l| l.contains("FreeBASIC"))?;
    let after = line.split("Version").nth(1)?.trim_start();
    let v: String = after
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let v = v.trim_end_matches('.');
    if v.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        Some(v.to_string())
    } else {
        None
    }
}

/// Discovery order, first success wins.
///
/// 1. an explicit override from settings
/// 2. `ETB_TOOLCHAIN_BUNDLE` — a bundle root, for exercising the bundled path
/// 3. `ETB_TOOLCHAIN` — a bare `fbc`, used by tests and by `cargo run`
/// 4. the bundled toolchain beside our own executable
/// 5. `fbc` on PATH — but only if step 4 found no bundle at all
///
/// Step 5 is deliberately not a fallback for a *broken* bundle. See below.
pub fn discover(override_path: Option<&Path>) -> Result<Toolchain> {
    if let Some(p) = override_path {
        return Toolchain::from_path(p.to_path_buf(), ToolchainKind::UserSpecified);
    }
    // A bundle root, for testing the bundled path without installing anything.
    if let Some(root) = std::env::var_os("ETB_TOOLCHAIN_BUNDLE") {
        if !root.is_empty() {
            return Toolchain::from_bundle(Path::new(&root));
        }
    }
    if let Some(p) = std::env::var_os("ETB_TOOLCHAIN") {
        if !p.is_empty() {
            return Toolchain::from_path(PathBuf::from(p), ToolchainKind::UserSpecified);
        }
    }
    if let Some(found) = bundled_toolchain(&bundled_roots()) {
        return found;
    }
    if let Ok(p) = which::which("fbc") {
        return Toolchain::from_path(p, ToolchainKind::System);
    }
    Err(EtbError::ToolchainMissing)
}

/// Step 4 of `discover`, and the reason step 5 is not a fallback for it.
///
/// `None` means no bundle was shipped -- the only case in which building with
/// whatever `fbc` is on PATH is the right answer. `Some(Err)` means a bundle
/// is there and will not load, which is a damaged installation and is reported
/// as one: the application already has the words for it, in both languages
/// (`toolchain.missing.reinstall`, and the antivirus topic beside it).
///
/// The distinction is the whole point. A program built by a compiler nobody
/// chose and no recipe pinned is a program that may behave differently than it
/// did yesterday, with nothing on screen to say so. "Reinstall the
/// application" is a far better failure.
///
/// Separated from `discover` so that policy can be tested without arranging for
/// `current_exe()` to sit next to a chosen directory.
fn bundled_toolchain(roots: &[PathBuf]) -> Option<Result<Toolchain>> {
    let mut damaged: Option<EtbError> = None;
    for root in roots {
        if looks_like_a_bundle(root) {
            match Toolchain::from_bundle(root) {
                Ok(mut tc) => {
                    // Found beside our own executable: this is the compiler we
                    // shipped, so this build is answerable for it.
                    tc.shipped = true;
                    return Some(Ok(tc));
                }
                Err(e) => {
                    tracing::warn!("bundle at {} will not load: {e}", root.display());
                    damaged.get_or_insert(e);
                }
            }
        }
    }
    damaged.map(Err)
}

/// Was a toolchain bundle shipped beside this executable?
///
/// This, not the compile profile, is the question that decides what to tell the
/// user when no compiler is found: a build that shipped a bundle and cannot find
/// one is damaged, while a build that never had one simply needs a compiler
/// installed. `cfg!(debug_assertions)` answers neither.
pub fn bundle_was_shipped() -> bool {
    bundled_roots().iter().any(|r| looks_like_a_bundle(r))
}

/// A directory is a bundle only if it actually holds a toolchain.
///
/// Merely being called `toolchain` is not enough: the repository has a
/// `toolchain/` directory of *recipes* sitting exactly where the development
/// layout looks for a bundle, and treating that as one would make a developer
/// build report a damaged installation instead of a missing compiler.
fn looks_like_a_bundle(root: &Path) -> bool {
    root.is_dir()
        && (root.join(bundle::BUNDLE_FILE).is_file()
            || root.join("fbc").is_file()
            || root.join("fbc.exe").is_file())
}

/// Where a bundled toolchain may live, most specific first.
fn bundled_roots() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            out.push(dir.join("toolchain"));
            // development layout: target/debug/easy-turbo-basic -> ../../toolchain
            out.push(dir.join("..").join("..").join("toolchain"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy() -> Toolchain {
        Toolchain {
            id: ToolchainId {
                kind: ToolchainKind::System,
                version: "4.6.0".into(),
                path: "/opt/fbc/fbc".into(),
            },
            fbc: "/opt/fbc/fbc".into(),
            shipped: false,
            origin: None,
            bundle: None,
        }
    }

    fn bundled(desc: bundle::BundleDescriptor, root: &Path) -> Toolchain {
        let b = Bundle::resolve(root, desc).unwrap();
        Toolchain {
            id: ToolchainId {
                kind: ToolchainKind::Bundled,
                version: b.version.clone(),
                path: b.fbc.clone(),
            },
            fbc: b.fbc.clone(),
            shipped: false,
            origin: None,
            bundle: Some(b),
        }
    }

    #[test]
    fn the_version_is_read_from_what_fbc_prints() {
        assert_eq!(
            parse_version("FreeBASIC Compiler - Version 1.10.1 (2023-12-24), built for linux-x86_64 (64bit)\n").as_deref(),
            Some("1.10.1")
        );
        assert_eq!(parse_version("something else"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn the_child_environment_contains_nothing_that_changes_a_build() {
        let layout = WorkLayout::new(PathBuf::from("/tmp/work"));
        let env = dummy().compile_env(&layout);
        for (k, _) in &env {
            let k = k.to_string_lossy().to_string();
            assert!(
                !BANNED_ENV.contains(&k.as_str()),
                "`{k}` must not be passed to the compiler"
            );
        }
    }

    #[test]
    fn the_child_environment_redirects_temporary_files_into_the_work_tree() {
        let layout = WorkLayout::new(PathBuf::from("/tmp/work"));
        let env = dummy().compile_env(&layout);
        let tmpdir = env
            .iter()
            .find(|(k, _)| k == "TMPDIR")
            .map(|(_, v)| PathBuf::from(v));
        assert_eq!(tmpdir, Some(layout.tmp()));
    }

    #[test]
    fn fbc_runs_in_its_own_directory() {
        let layout = WorkLayout::new(PathBuf::from("/tmp/work"));
        let cmd = dummy().command(&layout);
        assert_eq!(cmd.get_current_dir(), Some(Path::new("/opt/fbc")));
    }

    #[test]
    fn a_bundle_contributes_its_path_directories() {
        // Absolute on this platform: a bare "/opt/..." is relative on Windows.
        let root = if cfg!(windows) {
            PathBuf::from("C:\\opt\\etb\\toolchain")
        } else {
            PathBuf::from("/opt/etb/toolchain")
        };
        let tc = bundled(
            bundle::BundleDescriptor {
                id: "fbc-4.6.0-win-x64".into(),
                path_dirs: vec!["internal/c/c_compiler/bin".into()],
                ..Default::default()
            },
            &root,
        );
        let layout = WorkLayout::new(PathBuf::from("/tmp/work"));
        let env = tc.compile_env(&layout);
        let path = env
            .iter()
            .find(|(k, _)| k == "PATH")
            .unwrap()
            .1
            .to_string_lossy()
            .to_string();
        let want = root.join("internal/c/c_compiler/bin").display().to_string();
        assert!(
            path.starts_with(&want),
            "PATH {path:?} should start with {want:?}"
        );
    }

    /// A bundle at `root`, with a `fbc` and a directory of its own.
    fn lay_out_bundle(root: &Path) {
        std::fs::create_dir_all(root.join("internal/c/c_compiler/bin")).unwrap();
        std::fs::write(root.join("fbc"), b"driver").unwrap();
        std::fs::write(root.join("internal/c/libqb.o"), b"prebuilt").unwrap();
        std::fs::write(
            root.join(bundle::BUNDLE_FILE),
            "id = \"x\"\nversion = \"4.6.0\"\nfbc = \"fbc\"\n\
             path_dirs = [\"internal/c/c_compiler/bin\"]\n",
        )
        .unwrap();
    }

    #[test]
    fn a_compiler_it_can_be_given_the_path_of_is_used_where_it_is() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path().join("Programs/EasyTurboBasic/toolchain");
        lay_out_bundle(&root);
        let tc = Toolchain::from_bundle(&root).unwrap();
        assert!(tc.usable_in_place());

        let tools = td.path().join("tools");
        let locks = td.path().join("locks");
        let guard = FsGuard::new(vec![tools.clone(), locks.clone()]).unwrap();
        let ready = tc
            .prepare(
                &guard,
                &tools,
                &locks,
                &Manifest::default(),
                &AtomicBool::new(false),
            )
            .unwrap()
            .unwrap();
        assert_eq!(ready.fbc(), tc.fbc(), "no copy, nothing to copy for");
        assert!(ready.origin().is_none());
        assert_eq!(
            std::fs::read_dir(&tools).map(|d| d.count()).unwrap_or(0),
            0,
            "nothing was copied"
        );
    }

    #[test]
    fn a_compiler_under_a_vietnamese_path_is_run_from_a_copy_somewhere_ascii() {
        // The case this exists for: on Windows QB64-PE is handed its own path
        // through the ANSI code page, and `Nguyễn Văn A` does not survive it.
        let td = tempfile::tempdir().unwrap();
        let root = td
            .path()
            .join("Nguyễn Văn A/AppData/Local/Programs/ETB/toolchain");
        lay_out_bundle(&root);
        let tc = Toolchain::from_bundle(&root).unwrap();
        assert!(!tc.usable_in_place());

        let tools = td.path().join("tools");
        let locks = td.path().join("locks");
        let guard = FsGuard::new(vec![tools.clone(), locks.clone()]).unwrap();
        let manifest = Manifest::parse(&format!("{:x}  fbc\n", sha2::Sha256::digest(b"driver")));
        let ready = tc
            .prepare(&guard, &tools, &locks, &manifest, &AtomicBool::new(false))
            .unwrap()
            .unwrap();

        assert!(ready.fbc().starts_with(&tools));
        assert!(ready.fbc().is_file());
        assert!(
            ready.fbc().to_string_lossy().is_ascii(),
            "the copy is at a path QB64-PE can be given: {}",
            ready.fbc().display()
        );
        assert_eq!(ready.origin(), Some(tc.root()));
        // What it needs on PATH comes with it.
        let env = ready.compile_env(&WorkLayout::new(td.path().join("w")));
        let path = env
            .iter()
            .find(|(k, _)| k == "PATH")
            .unwrap()
            .1
            .to_string_lossy()
            .to_string();
        assert!(
            path.contains(&tools.to_string_lossy().to_string()),
            "{path}"
        );

        // Integrity is about the installation, not the copy.
        assert!(ready.verify_integrity(&manifest).is_ok());
        std::fs::write(ready.fbc(), b"scribbled on").unwrap();
        assert!(
            ready.verify_integrity(&manifest).is_ok(),
            "the copy is not what was shipped"
        );
        std::fs::write(root.join("fbc"), b"tampered").unwrap();
        assert!(
            !ready.verify_integrity(&manifest).is_ok(),
            "the installation is"
        );
    }

    #[test]
    fn the_copy_is_made_once_and_remade_when_the_compiler_changes() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path().join("Nguyễn Văn A/toolchain");
        lay_out_bundle(&root);
        let tools = td.path().join("tools");
        let locks = td.path().join("locks");
        let guard = FsGuard::new(vec![tools.clone(), locks.clone()]).unwrap();
        let go = || {
            Toolchain::from_bundle(&root)
                .unwrap()
                .prepare(
                    &guard,
                    &tools,
                    &locks,
                    &Manifest::default(),
                    &AtomicBool::new(false),
                )
                .unwrap()
                .unwrap()
        };
        let first = go();
        // Something left beside the compiler between builds: it must survive,
        // or every build starts by copying 176 MB again.
        let built = first.root().join("internal/c/qbx.o");
        std::fs::write(&built, b"compiled here").unwrap();
        let again = go();
        assert_eq!(again.fbc(), first.fbc());
        assert!(built.is_file(), "the copy was made again for no reason");

        // A new version installed over the old one: the copy is remade.
        std::fs::write(root.join("fbc"), b"a different driver").unwrap();
        let after = go();
        assert_eq!(after.fbc(), first.fbc());
        assert!(
            !built.exists(),
            "a stale copy would build with the old compiler"
        );
    }

    /// The copy can live in a shared directory — `%ProgramData%`, when the
    /// user's own folder is at a path the compiler cannot be given — and
    /// anyone with an account on the machine can create directories there.
    /// So the copy is trusted for what it hashes to, not for the note beside
    /// it, which they could write themselves.
    #[test]
    fn a_copy_that_does_not_hash_to_the_shipped_compiler_is_made_again() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path().join("Nguyễn Văn A/toolchain");
        lay_out_bundle(&root);
        let tools = td.path().join("tools");
        let locks = td.path().join("locks");
        let guard = FsGuard::new(vec![tools.clone(), locks.clone()]).unwrap();
        let manifest = Manifest::parse(&format!(
            "{:x}  fbc\n",
            sha2::Sha256::digest(std::fs::read(root.join("fbc")).unwrap())
        ));
        let go = || {
            Toolchain::from_bundle(&root)
                .unwrap()
                .prepare(&guard, &tools, &locks, &manifest, &AtomicBool::new(false))
                .unwrap()
                .unwrap()
        };

        let ready = go();
        let copied = ready.fbc().to_path_buf();
        assert!(copied.is_file());

        // Somebody else got there first and left their own program behind,
        // with the stamp that says it is ours.
        std::fs::write(&copied, b"#!/bin/sh\nrm -rf ~\n").unwrap();
        let again = go();
        assert_eq!(again.fbc(), copied, "same place");
        assert_eq!(
            std::fs::read(&copied).unwrap(),
            std::fs::read(root.join("fbc")).unwrap(),
            "what is there now must be the compiler we shipped, not what was planted"
        );
    }

    #[test]
    fn a_copy_that_is_stopped_part_way_is_not_taken_for_finished() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path().join("Nguyễn Văn A/toolchain");
        lay_out_bundle(&root);
        let tools = td.path().join("tools");
        let locks = td.path().join("locks");
        let guard = FsGuard::new(vec![tools.clone(), locks.clone()]).unwrap();
        let tc = Toolchain::from_bundle(&root).unwrap();
        assert!(tc
            .prepare(
                &guard,
                &tools,
                &locks,
                &Manifest::default(),
                &AtomicBool::new(true)
            )
            .unwrap()
            .is_none());
        // Nothing was stamped, so the next attempt starts over rather than
        // running a half-copied compiler.
        let ready = tc
            .prepare(
                &guard,
                &tools,
                &locks,
                &Manifest::default(),
                &AtomicBool::new(false),
            )
            .unwrap()
            .unwrap();
        assert!(ready.fbc().is_file());
    }

    #[test]
    fn a_system_toolchain_has_nothing_to_verify() {
        let m = manifest::Manifest::parse(
            "0000000000000000000000000000000000000000000000000000000000000000  fbc\n",
        );
        assert!(dummy().verify_integrity(&m).is_ok());
    }

    #[test]
    fn a_bundle_is_verified_against_the_manifest() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path().join("toolchain");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("fbc"), b"driver").unwrap();
        let hash = format!("{:x}", sha2::Sha256::digest(b"driver"));

        let tc = bundled(bundle::BundleDescriptor::default(), &root);
        assert!(tc
            .verify_integrity(&manifest::Manifest::parse(&format!("{hash}  fbc\n")))
            .is_ok());

        let bad = manifest::Manifest::parse(
            "0000000000000000000000000000000000000000000000000000000000000000  fbc\n",
        );
        let r = tc.verify_integrity(&bad);
        assert!(!r.is_ok());
        assert_eq!(r.changed, vec!["fbc".to_string()]);
    }

    #[test]
    fn an_empty_manifest_verifies_because_there_is_nothing_to_check() {
        let td = tempfile::tempdir().unwrap();
        let tc = bundled(bundle::BundleDescriptor::default(), td.path());
        assert!(tc
            .verify_integrity(&manifest::Manifest::parse("# none\n"))
            .is_ok());
    }

    #[test]
    fn a_bundle_cannot_reintroduce_a_banned_variable() {
        // A bundle descriptor is ours, but it is data, and data should not be able
        // to undo the environment scrubbing.
        let mut env_map = std::collections::BTreeMap::new();
        env_map.insert("CXXFLAGS".to_string(), "-O3 -ffast-math".to_string());
        env_map.insert("WINEPREFIX".to_string(), "${ROOT}/wine".to_string());
        let tc = bundled(
            bundle::BundleDescriptor {
                env: env_map,
                ..Default::default()
            },
            Path::new("/opt/t"),
        );
        let layout = WorkLayout::new(PathBuf::from("/tmp/work"));
        let env = tc.compile_env(&layout);
        let names: Vec<String> = env
            .iter()
            .map(|(k, _)| k.to_string_lossy().to_string())
            .collect();
        assert!(
            !names.contains(&"CXXFLAGS".to_string()),
            "a bundle must not be able to set a banned variable; got {names:?}"
        );
        assert!(names.contains(&"WINEPREFIX".to_string()));
    }

    #[test]
    fn a_directory_named_toolchain_is_not_by_itself_a_bundle() {
        let td = tempfile::tempdir().unwrap();
        let recipes = td.path().join("toolchain");
        std::fs::create_dir_all(&recipes).unwrap();
        std::fs::write(recipes.join("linux-x86_64.toml"), "target = \"x\"\n").unwrap();
        assert!(
            !looks_like_a_bundle(&recipes),
            "a folder of recipes must not be mistaken for a shipped toolchain"
        );

        std::fs::write(recipes.join("fbc.exe"), b"").unwrap();
        assert!(looks_like_a_bundle(&recipes), "a fbc makes it one");
    }

    #[test]
    fn a_bundle_is_recognised_by_its_descriptor_alone() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join(bundle::BUNDLE_FILE), "id = \"x\"\n").unwrap();
        assert!(looks_like_a_bundle(td.path()));
    }

    #[test]
    fn a_shipped_compiler_with_nothing_to_check_it_against_does_not_pass() {
        // The quiet failure this exists to stop: a release built before its own
        // compiler was fetched embeds the placeholder manifest, so there is
        // nothing to verify against. Reporting that as "verified" would look
        // exactly like success while checking nothing at all.
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("fbc"), b"#!/bin/sh\n").unwrap();
        let mut tc = bundled(bundle::BundleDescriptor::default(), td.path());

        let empty = manifest::Manifest::parse("# placeholder, no entries\n");
        assert!(empty.is_empty(), "the placeholder must parse to nothing");

        // Pointed at by a test or a developer: not ours to vouch for.
        tc.shipped = false;
        assert!(tc.verify_integrity(&empty).is_ok());

        // Shipped beside our own executable: we are answerable for it.
        tc.shipped = true;
        let report = tc.verify_integrity(&empty);
        assert!(!report.is_ok(), "a shipped compiler must be attested");
        assert!(report.unattested);
        assert_eq!(
            report.explanation_key(),
            Some("toolchain.integrity.unattested")
        );
    }

    #[test]
    fn a_damaged_bundle_is_reported_rather_than_replaced_by_whatever_is_on_path() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(
            td.path().join(bundle::BUNDLE_FILE),
            "id = \"x\"\nversion = \"4.6.0\"\nfbc = \"gone.exe\"\n",
        )
        .unwrap();

        match bundled_toolchain(&[td.path().to_path_buf()]) {
            Some(Err(EtbError::ToolchainInvalid { .. })) => {}
            Some(Ok(_)) => panic!("a bundle with no compiler in it must not load"),
            Some(Err(e)) => panic!("expected ToolchainInvalid, got {e:?}"),
            None => panic!(
                "a damaged bundle reported as `no bundle` is exactly the bug: \
                 discover would fall through to whatever is on PATH"
            ),
        }
    }

    #[test]
    fn no_bundle_at_all_still_defers_to_the_system_compiler() {
        let td = tempfile::tempdir().unwrap();
        assert!(
            bundled_toolchain(&[td.path().to_path_buf()]).is_none(),
            "an empty directory is not a bundle, damaged or otherwise"
        );
    }

    #[test]
    fn a_missing_compiler_is_reported_not_panicked() {
        let err =
            Toolchain::from_path("/nonexistent/fbc".into(), ToolchainKind::System).unwrap_err();
        assert!(matches!(err, EtbError::ToolchainInvalid { .. }));
    }
}

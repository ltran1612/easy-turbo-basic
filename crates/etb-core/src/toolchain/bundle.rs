//! How to drive a bundled QB64 Phoenix Edition.
//!
//! Each bundle ships a `bundle.toml` saying where its `fbc` is, which of its
//! directories the C++ compiler behind it needs on PATH, and — when the bundle
//! is for another platform, as the Windows bundle is when it is exercised from
//! Linux — what runs its binaries. The application code is the same for every
//! bundle; the differences are data.

use crate::error::{EtbError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

/// The file a bundle ships at its root.
pub const BUNDLE_FILE: &str = "bundle.toml";

/// Substituted with the bundle's absolute root at load time, so the bundle can be
/// installed anywhere — next to the executable, in Program Files, or unpacked into
/// a temporary directory by a test.
const ROOT_TOKEN: &str = "${ROOT}";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BundleDescriptor {
    /// Stable identity of this toolchain, e.g. `fbc-4.6.0-win-x64`.
    pub id: String,
    pub version: String,
    /// Path to `fbc`, relative to the bundle root.
    pub fbc: String,
    /// Extra environment for the child, merged over the scrubbed defaults.
    pub env: BTreeMap<String, String>,
    /// Directories prepended to the child's PATH, relative to the bundle root.
    /// On Windows, the bundled C++ compiler's `bin`.
    pub path_dirs: Vec<String>,
    /// Suffix the *bundle's target* gives executables, e.g. `.exe`.
    ///
    /// Not the host's: a Windows bundle driven from Linux writes `program.exe`
    /// while the host suffix is empty. Defaults to the host's, which is right
    /// whenever the bundle targets the machine it runs on.
    pub exe_suffix: Option<String>,
    /// Program used to run the bundle's binaries, e.g. `wine`.
    ///
    /// Absent in a shipped bundle. Set when driving a bundle built for another
    /// platform, which is how the Windows bundle is exercised from Linux.
    pub launcher: Option<String>,
}

impl Default for BundleDescriptor {
    fn default() -> Self {
        Self {
            id: String::new(),
            version: String::new(),
            fbc: default_fbc_path(),
            env: BTreeMap::new(),
            path_dirs: Vec::new(),
            exe_suffix: None,
            launcher: None,
        }
    }
}

fn default_fbc_path() -> String {
    format!("fbc{}", std::env::consts::EXE_SUFFIX)
}

/// A descriptor with every `${ROOT}` resolved against a real directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bundle {
    pub root: PathBuf,
    pub id: String,
    pub version: String,
    pub fbc: PathBuf,
    pub env: Vec<(OsString, OsString)>,
    pub path_dirs: Vec<PathBuf>,
    pub exe_suffix: Option<String>,
    pub launcher: Option<String>,
}

impl Bundle {
    /// The same bundle, seen at another root: a copy of it made somewhere the
    /// compiler can be given its own path.
    pub fn rebased(&self, root: &Path) -> Self {
        let move_in = |p: &Path| root.join(p.strip_prefix(&self.root).unwrap_or(p));
        Self {
            root: root.to_path_buf(),
            id: self.id.clone(),
            version: self.version.clone(),
            fbc: move_in(&self.fbc),
            env: self.env.clone(),
            path_dirs: self.path_dirs.iter().map(|p| move_in(p)).collect(),
            exe_suffix: self.exe_suffix.clone(),
            launcher: self.launcher.clone(),
        }
    }

    /// Load `bundle.toml` from a bundle root. `Ok(None)` means there is no
    /// descriptor, which is fine: a plain QB64-PE tree needs no instructions.
    pub fn load(root: &Path) -> Result<Option<Self>> {
        let file = root.join(BUNDLE_FILE);
        let text = match std::fs::read_to_string(&file) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(EtbError::io(&file, e)),
        };
        let desc: BundleDescriptor =
            toml::from_str(&text).map_err(|e| EtbError::ToolchainInvalid {
                path: file.clone(),
                reason: format!("{BUNDLE_FILE} is malformed: {e}"),
            })?;
        Ok(Some(Self::resolve(root, desc)?))
    }

    pub fn resolve(root: &Path, d: BundleDescriptor) -> Result<Self> {
        // The root must be absolute: paths are handed to a child whose working
        // directory is not ours, so a relative root would resolve against the
        // wrong place.
        let root = absolutize(root);
        let sub = |s: &str| -> String { s.replace(ROOT_TOKEN, &root.to_string_lossy()) };

        let fbc = join_inside(&root, &sub(&d.fbc), "fbc")?;
        let mut path_dirs = Vec::new();
        for p in &d.path_dirs {
            path_dirs.push(join_inside(&root, &sub(p), "path_dirs")?);
        }

        Ok(Self {
            id: d.id,
            version: d.version,
            fbc,
            env: d
                .env
                .iter()
                .map(|(k, v)| (OsString::from(k), OsString::from(sub(v))))
                .collect(),
            path_dirs,
            exe_suffix: d.exe_suffix,
            // Resolved to an absolute path now, because the child's PATH is
            // scrubbed down to the bundle's own directories: a bare `wine` would
            // not be found there, and every build would fail for a reason that
            // looks nothing like the cause.
            launcher: d.launcher.map(|l| resolve_launcher(&l)),
            root,
        })
    }
}

/// Find a launcher on the *parent's* PATH, keeping the name if it cannot be
/// found so the failure names the program the descriptor asked for.
fn resolve_launcher(name: &str) -> String {
    if Path::new(name).is_absolute() {
        return name.to_string();
    }
    which::which(name)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| name.to_string())
}

/// Make a path absolute without requiring it to exist, resolving symlinks where
/// the path is real.
fn absolutize(p: &Path) -> PathBuf {
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(p)
    };
    dunce::canonicalize(&abs).unwrap_or(abs)
}

/// Resolve a descriptor-supplied path against the bundle root, refusing anything
/// that ends up outside it.
///
/// The check is on the *resolved* path, not on how it was written: `${ROOT}/bin`
/// is fine even though it substitutes to an absolute path, while `/usr/bin` and
/// `../../usr/bin` are not. A descriptor is our own data, but it is still data,
/// and it must not be able to aim the "bundled" toolchain at one on the host —
/// the single outcome this whole mechanism exists to rule out.
fn join_inside(root: &Path, resolved: &str, field: &str) -> Result<PathBuf> {
    let bad = |reason: String| EtbError::ToolchainInvalid {
        path: root.to_path_buf(),
        reason: format!("{BUNDLE_FILE}: `{field}` {reason}"),
    };
    if resolved.trim().is_empty() {
        return Err(bad("is empty".into()));
    }
    let p = Path::new(resolved);
    let joined = if p.is_absolute() {
        p.to_path_buf()
    } else {
        root.join(p)
    };
    let norm = lexical_normalize(&joined);
    if !norm.starts_with(root) {
        return Err(bad(format!(
            "must stay inside the bundle, got `{resolved}`"
        )));
    }
    Ok(norm)
}

/// Resolve `.` and `..` without touching the filesystem, so the containment check
/// works for paths that do not exist yet.
fn lexical_normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_descriptor_is_not_an_error() {
        let td = tempfile::tempdir().unwrap();
        assert_eq!(Bundle::load(td.path()).unwrap(), None);
    }

    #[test]
    fn root_is_substituted_everywhere_it_appears() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path();
        std::fs::write(
            root.join(BUNDLE_FILE),
            r#"
id = "fbc-4.6.0-win-x64"
version = "4.6.0"
fbc = "fbc.exe"
path_dirs = ["${ROOT}/internal/c/c_compiler/bin"]

[env]
EXAMPLE = "${ROOT}/internal"
"#,
        )
        .unwrap();

        let b = Bundle::load(root).unwrap().unwrap();
        let root = &b.root; // canonicalized
        assert_eq!(b.id, "fbc-4.6.0-win-x64");
        assert_eq!(b.fbc, root.join("fbc.exe"));
        assert_eq!(
            b.env[0].1,
            OsString::from(format!("{}/internal", root.display()))
        );
        assert_eq!(b.path_dirs, vec![root.join("internal/c/c_compiler/bin")]);
        assert!(!b.path_dirs[0].to_string_lossy().contains(ROOT_TOKEN));
    }

    #[test]
    fn a_plain_tree_needs_nothing_but_an_id() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join(BUNDLE_FILE), "id = \"x\"\n").unwrap();
        let b = Bundle::load(td.path()).unwrap().unwrap();
        assert!(b.path_dirs.is_empty());
        assert!(b.fbc.ends_with(default_fbc_path()));
        assert!(b.exe_suffix.is_none());
        assert!(b.launcher.is_none());
    }

    #[test]
    fn a_descriptor_cannot_point_outside_the_bundle() {
        // Otherwise a bundle could aim the "bundled" toolchain at one on the
        // host, which is exactly what bundling exists to prevent.
        let td = tempfile::tempdir().unwrap();
        for bad in [
            "fbc = \"/usr/bin/fbc\"",
            "fbc = \"../../../usr/bin/fbc\"",
            "path_dirs = [\"/usr/bin\"]",
            "path_dirs = [\"../../usr/bin\"]",
            "fbc = \"\"",
        ] {
            std::fs::write(td.path().join(BUNDLE_FILE), format!("id = \"x\"\n{bad}\n")).unwrap();
            let err = Bundle::load(td.path()).unwrap_err();
            assert!(
                matches!(err, EtbError::ToolchainInvalid { .. }),
                "{bad} should have been refused"
            );
        }
    }

    #[test]
    fn a_relative_root_is_made_absolute() {
        let b = Bundle::resolve(
            Path::new("some/relative/toolchain"),
            BundleDescriptor::default(),
        )
        .unwrap();
        assert!(b.root.is_absolute(), "root was {:?}", b.root);
        assert!(b.fbc.is_absolute());
    }

    #[test]
    fn a_misspelled_key_is_an_error_rather_than_silently_ignored() {
        // `path_dir` (singular) would otherwise leave the compiler's directory
        // off PATH, and every build would fail with a message about something
        // else entirely.
        let td = tempfile::tempdir().unwrap();
        std::fs::write(
            td.path().join(BUNDLE_FILE),
            "id = \"x\"\npath_dir = [\"internal/c/c_compiler/bin\"]\n",
        )
        .unwrap();
        let err = Bundle::load(td.path()).unwrap_err();
        assert!(matches!(err, EtbError::ToolchainInvalid { .. }));
    }

    #[test]
    fn a_bundle_may_declare_its_targets_suffix_and_a_launcher() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(
            td.path().join(BUNDLE_FILE),
            "id = \"x\"\nexe_suffix = \".exe\"\nlauncher = \"wine\"\n",
        )
        .unwrap();
        let b = Bundle::load(td.path()).unwrap().unwrap();
        assert_eq!(b.exe_suffix.as_deref(), Some(".exe"));
        // Resolved against the parent's PATH, because the child's is scrubbed.
        let l = b.launcher.as_deref().unwrap();
        assert!(
            l == "wine" || Path::new(l).is_absolute(),
            "launcher should be absolute when findable, got {l:?}"
        );
        assert!(l.ends_with("wine"));
    }

    #[test]
    fn an_unfindable_launcher_keeps_its_name_so_the_error_says_what_was_wanted() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(
            td.path().join(BUNDLE_FILE),
            "id = \"x\"\nlauncher = \"definitely-not-installed-xyz\"\n",
        )
        .unwrap();
        let b = Bundle::load(td.path()).unwrap().unwrap();
        assert_eq!(b.launcher.as_deref(), Some("definitely-not-installed-xyz"));
    }

    #[test]
    fn a_malformed_descriptor_is_reported_not_ignored() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join(BUNDLE_FILE), "id = [this is not toml").unwrap();
        let err = Bundle::load(td.path()).unwrap_err();
        assert!(matches!(err, EtbError::ToolchainInvalid { .. }));
    }
}

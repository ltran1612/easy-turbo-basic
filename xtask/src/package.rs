//! `cargo xtask package` — assemble a distributable.
//!
//! The compiler is never in git. It is built into `target/` from the pinned
//! recipe by `fetch-toolchain`, and this gathers it with the application and the
//! licences into something to attach to a release. CI runs both, so the release
//! artefact is reproducible from the repository plus the recipe, and the
//! repository stays small.

use crate::fetch;
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub fn run(args: &[String]) -> Result<()> {
    let mut target: Option<String> = None;
    let mut profile = "release".to_string();
    let mut archive = true;

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--target" => target = Some(it.next().context("--target needs a value")?.clone()),
            "--profile" => profile = it.next().context("--profile needs a value")?.clone(),
            "--no-archive" => archive = false,
            other => bail!(
                "unknown option `{other}`\n\n\
                 USAGE: cargo xtask package [--target <name>] [--profile <p>] [--no-archive]"
            ),
        }
    }
    let target = target.unwrap_or_else(default_target);
    let windows = target.starts_with("windows");

    let root = crate::repo_root();
    let version = workspace_version(&root)?;
    let bundle = root.join("target").join("toolchain").join(&target);
    if !bundle.join("bundle.toml").is_file() {
        bail!(
            "no toolchain bundle at {}.\n\nRun:  cargo xtask fetch-toolchain --target {target}",
            bundle.display()
        );
    }

    // `toolchain/verify-under-wine.sh` appends a `launcher` to the fetched
    // bundle so it can be driven from Linux, and packaging copies bundle.toml
    // verbatim. Shipping that would produce an installer whose compiler tries to
    // launch wine from a scratch directory on the user's machine -- and it would
    // fail at the user, not here.
    refuse_cross_test_bundle(&fs::read_to_string(bundle.join("bundle.toml"))?)
        .with_context(|| format!("{}", bundle.join("bundle.toml").display()))?;

    let exe_name = if windows {
        "easy-turbo-basic.exe"
    } else {
        "easy-turbo-basic"
    };
    let app = root.join("target").join(&profile).join(exe_name);
    if !app.is_file() {
        bail!(
            "no application binary at {}.\n\nRun:  cargo build --{profile}",
            app.display()
        );
    }

    let stem = format!("EasyTurboBasic-{version}-{target}");
    let dist = root.join("target").join("dist");
    let out = dist.join(&stem);
    fs::create_dir_all(&dist)?;
    if out.exists() {
        fs::remove_dir_all(&out)?;
    }
    fs::create_dir_all(&out)?;

    // The application, then the compiler beside it, which is where discovery
    // looks: current_exe()/../toolchain.
    fs::copy(&app, out.join(exe_name))?;
    copy_tree(&bundle, &out.join("toolchain"))?;
    licences(&root, &out, &target, &version)?;
    examples(&root, &out)?;
    readme(&out, windows)?;

    let bytes = dir_size(&out)?;
    println!("staged   {} ({:.1} MB)", out.display(), bytes as f64 / 1e6);

    if archive {
        let path = if windows {
            zip_dir(&out, &dist.join(format!("{stem}.zip")))?
        } else {
            targz_dir(&out, &dist.join(format!("{stem}.tar.gz")))?
        };
        let size = fs::metadata(&path)?.len();
        println!("archive  {} ({:.1} MB)", path.display(), size as f64 / 1e6);
    }
    if windows {
        println!();
        println!("For the installer, CI runs makensis over packaging/windows/installer.nsi.");
    }
    Ok(())
}

/// A bundle meant for shipping runs natively. A `launcher` means this one was
/// set up to be driven from another platform, and must not leave the machine.
fn refuse_cross_test_bundle(bundle_toml: &str) -> Result<()> {
    let has_launcher = bundle_toml
        .lines()
        .map(str::trim)
        .any(|l| l.starts_with("launcher") && l.contains('='));
    if has_launcher {
        bail!(
            "this bundle carries a `launcher`, so it was set up for cross-testing \
             (verify-under-wine.sh appends one) and would ship a compiler that tries \
             to run under wine on the user's machine.\n\n\
             Rebuild it first:  cargo xtask fetch-toolchain --target <target>"
        );
    }
    Ok(())
}

fn default_target() -> String {
    if cfg!(windows) {
        "windows-x86_64".into()
    } else {
        "linux-x86_64".into()
    }
}

/// The workspace version, and the only place it is read from.
///
/// The release workflow names the installer after this, and `package` names the
/// archive after it. Two parsers would eventually disagree and produce a release
/// whose files contradict its own tag, so the workflow asks for this one via
/// `cargo xtask version`.
pub fn workspace_version(root: &Path) -> Result<String> {
    let text = fs::read_to_string(root.join("Cargo.toml"))?;
    for line in text.lines() {
        if let Some(v) = line.trim().strip_prefix("version = ") {
            return Ok(v.trim().trim_matches('"').to_string());
        }
    }
    bail!("could not read the workspace version from Cargo.toml")
}

/// Ship the example programs with the application.
///
/// They exist so that someone opening this for the first time has something to
/// press the button on, and that only works if they arrive with it. In the
/// repository alone they serve whoever reads the repository, which is not the
/// person this is for.
fn examples(root: &Path, out: &Path) -> Result<()> {
    let from = root.join("examples");
    if !from.is_dir() {
        bail!("no examples/ directory at {}", from.display());
    }
    copy_tree(&from, &out.join("examples"))?;
    Ok(())
}

// ------------------------------------------------------------------ licences

/// What we are obliged to ship beside GPL binaries, and what a reader needs to
/// find the source.
fn licences(root: &Path, out: &Path, target: &str, version: &str) -> Result<()> {
    let dir = out.join("LICENSES");
    fs::create_dir_all(&dir)?;
    for name in ["LICENSE-MIT", "LICENSE-APACHE", "gpl-3.0.txt"] {
        let from = if name.starts_with("LICENSE-") {
            root.join(name)
        } else {
            root.join("LICENSES").join(name)
        };
        fs::copy(&from, dir.join(name)).with_context(|| format!("copying {}", from.display()))?;
    }

    let recipe = fetch::read_recipe(root, target)?;
    let mut md = String::new();
    md.push_str(&format!(
        "# Third-party software in Easy Turbo Basic {version}\n\n"
    ));
    md.push_str(
        "Easy Turbo Basic itself is licensed MIT OR Apache-2.0 (`LICENSE-MIT`,\n\
         `LICENSE-APACHE`).\n\n\
         ## The bundled compiler\n\n\
         The `toolchain/` directory is QB64 Phoenix Edition and the C++ toolchain it\n\
         runs. Each component is listed below with its licence; their own licence\n\
         texts are in `toolchain/licenses/` and `toolchain/internal/c/c_compiler/`,\n\
         and the GNU General Public License is `gpl-3.0.txt` here.\n\n\
         Easy Turbo Basic runs QB64-PE as a separate program, which runs the others\n\
         in turn, communicating through command-line arguments and files. That is an\n\
         aggregate, not a combined work.\n\n\
         **Programs you build are yours.** QB64-PE's runtime and the libraries it\n\
         builds into your program are under permissive licences, listed in\n\
         `toolchain/licenses/`. They place no requirement on your own code; if you\n\
         give a built program to other people, include those notices with it.\n\n\
         ## Components\n\n",
    );
    md.push_str("| Component | Version | Licence |\n|---|---|---|\n");
    for s in &recipe.sources {
        md.push_str(&format!(
            "| {} | {} | {} |\n",
            s.component, s.version, s.license
        ));
    }
    md.push_str("\n## Source code\n\n");
    md.push_str(
        "The GNU licences entitle you to the source for the GPL components above. It\n\
         is published alongside this download, on the same page, for as long as this\n\
         release is available. If you cannot find it there, ask and we will send it.\n\n",
    );
    md.push_str("| Component | Where |\n|---|---|\n");
    for s in &recipe.sources {
        md.push_str(&format!("| {} | <{}> |\n", s.component, s.url));
    }
    md.push_str(
        "\n## Fonts\n\nNone are bundled. The interface uses a font already on the \
         machine.\n",
    );
    fs::write(dir.join("THIRD-PARTY.md"), md)?;
    Ok(())
}

fn readme(out: &Path, windows: bool) -> Result<()> {
    let run = if windows {
        "Bấm đúp vào easy-turbo-basic.exe.\n\nDouble-click easy-turbo-basic.exe."
    } else {
        "Chạy ./easy-turbo-basic\n\nRun ./easy-turbo-basic"
    };
    fs::write(
        out.join("README.txt"),
        format!(
            "Easy Turbo Basic\n\
             ===============\n\n\
             {run}\n\n\
             Thư mục toolchain chứa trình biên dịch. Đừng xoá hoặc di chuyển nó:\n\
             ứng dụng kiểm tra và sẽ từ chối chạy nếu thiếu tệp.\n\n\
             The toolchain folder is the compiler. Do not delete or move it: the\n\
             application checks it and refuses to build if anything is missing.\n\n\
             Giấy phép và mã nguồn: xem thư mục LICENSES.\n\
             Licensing and source code: see the LICENSES folder.\n"
        ),
    )?;
    Ok(())
}

// ------------------------------------------------------------------ plumbing

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let p = entry?.path();
        let dest = to.join(p.file_name().unwrap());
        let md = fs::symlink_metadata(&p)?;
        if md.file_type().is_symlink() {
            let target = fs::read_link(&p)?;
            #[cfg(unix)]
            std::os::unix::fs::symlink(&target, &dest)?;
            #[cfg(not(unix))]
            {
                let resolved = p.parent().unwrap().join(&target);
                if resolved.is_file() {
                    fs::copy(&resolved, &dest)?;
                }
            }
        } else if md.is_dir() {
            copy_tree(&p, &dest)?;
        } else {
            fs::copy(&p, &dest)?;
        }
    }
    Ok(())
}

fn walk_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in fs::read_dir(&dir)? {
            let p = e?.path();
            let md = fs::symlink_metadata(&p)?;
            if md.file_type().is_symlink() {
                out.push(p);
            } else if md.is_dir() {
                stack.push(p);
            } else {
                out.push(p);
            }
        }
    }
    out.sort();
    Ok(out)
}

fn dir_size(root: &Path) -> Result<u64> {
    Ok(walk_files(root)?
        .iter()
        .filter_map(|p| fs::symlink_metadata(p).ok().map(|m| m.len()))
        .sum())
}

fn zip_dir(dir: &Path, dest: &Path) -> Result<PathBuf> {
    use std::io::Write;
    let file = fs::File::create(dest)?;
    let mut zip = zip::ZipWriter::new(file);
    let top = dir.file_name().unwrap().to_string_lossy().to_string();
    for p in walk_files(dir)? {
        let rel = p.strip_prefix(dir)?.to_string_lossy().replace('\\', "/");
        let name = format!("{top}/{rel}");
        let opts: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        // Carry the executable bit across, which only Unix has one to carry.
        // Shadowed rather than mutated so neither the binding nor the metadata
        // read exists at all on a platform that cannot use them.
        #[cfg(unix)]
        let opts = {
            use std::os::unix::fs::PermissionsExt;
            opts.unix_permissions(fs::symlink_metadata(&p)?.permissions().mode())
        };
        zip.start_file(name, opts)?;
        zip.write_all(&fs::read(&p)?)?;
    }
    zip.finish()?;
    Ok(dest.to_path_buf())
}

fn targz_dir(dir: &Path, dest: &Path) -> Result<PathBuf> {
    let file = fs::File::create(dest)?;
    let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut tar = tar::Builder::new(enc);
    tar.follow_symlinks(false);
    let top = dir.file_name().unwrap().to_string_lossy().to_string();
    tar.append_dir_all(&top, dir)?;
    tar.into_inner()?.finish()?;
    Ok(dest.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::refuse_cross_test_bundle;

    #[test]
    fn a_shipping_bundle_is_accepted() {
        let toml = "id = \"x\"\nversion = \"4.6.0\"\nfbc = \"fbc.exe\"\n\
                    exe_suffix = \".exe\"\npath_dirs = [\"internal/c/c_compiler/bin\"]\n";
        assert!(refuse_cross_test_bundle(toml).is_ok());
    }

    #[test]
    fn a_cross_testing_bundle_is_refused() {
        // Exactly what verify-under-wine.sh appends.
        let toml = "id = \"x\"\nfbc = \"fbc.exe\"\n\n\
                    launcher = \"wine\"\n\n[env]\nWINEPREFIX = \"/tmp/scratch/wineprefix\"\n";
        let err = refuse_cross_test_bundle(toml).unwrap_err().to_string();
        assert!(err.contains("launcher"), "{err}");
        assert!(
            err.contains("fetch-toolchain"),
            "the error must say how to fix it"
        );
    }

    #[test]
    fn a_commented_out_launcher_is_not_a_launcher() {
        let toml = "id = \"x\"\n# launcher = \"wine\" -- only for cross-testing\n";
        assert!(refuse_cross_test_bundle(toml).is_ok());
    }

    #[test]
    fn an_env_block_alone_is_fine() {
        // A real bundle may legitimately set variables; only a launcher means
        // it was aimed at another platform.
        let toml = "id = \"x\"\n[env]\nEXAMPLE = \"${ROOT}/internal\"\n";
        assert!(refuse_cross_test_bundle(toml).is_ok());
    }
}

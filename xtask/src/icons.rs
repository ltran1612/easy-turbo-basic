//! `cargo xtask gen-icons` — derive the build's icons from `logo.png`.
//!
//! One piece of artwork lives in the repository. Everything the build needs is
//! generated from it, so the window icon, the executable's icon and the
//! installer's icon cannot drift apart or disagree with the README.
//!
//! Two files come out:
//!
//! - `packaging/windows/icon.ico` — embedded into `easy-turbo-basic.exe` as a
//!   resource, and used by NSIS for the installer and uninstaller. The installer
//!   icon is the one the user actually sees first, on the file they download.
//! - `crates/etb-gui/assets/icon-128.png` — the window icon, set at runtime, which
//!   is how Linux gets one at all.

use anyhow::{bail, Context, Result};
use image::codecs::ico::{IcoEncoder, IcoFrame};
use image::imageops::FilterType;
use image::ImageEncoder;
use std::fs;
use std::path::{Path, PathBuf};

/// Sizes Windows actually asks for. 256 is the largest an `.ico` can hold, and
/// the small ones are worth generating rather than letting the shell downscale:
/// a 16-pixel icon resampled from 512 comes out mush.
const ICO_SIZES: &[u32] = &[16, 24, 32, 48, 64, 128, 256];

/// The window icon. 128 is comfortably more than any desktop draws in a title
/// bar or a task switcher.
const WINDOW_ICON: u32 = 128;

pub fn run(args: &[String]) -> Result<()> {
    if let Some(a) = args.first() {
        bail!("unknown option `{a}`\n\nUSAGE: cargo xtask gen-icons");
    }
    let root = crate::repo_root();
    let g = generate(&root)?;

    let ico = ico_path(&root);
    fs::create_dir_all(ico.parent().unwrap())?;
    fs::write(&ico, &g.ico)?;
    println!(
        "ico      {} ({} sizes, {:.0} KB)",
        ico.display(),
        ICO_SIZES.len(),
        g.ico.len() as f64 / 1e3
    );

    let png = window_png_path(&root);
    fs::create_dir_all(png.parent().unwrap())?;
    fs::write(&png, &g.window_png)?;
    println!(
        "png      {} ({:.0} KB)",
        png.display(),
        g.window_png.len() as f64 / 1e3
    );
    Ok(())
}

pub fn ico_path(root: &Path) -> PathBuf {
    root.join("packaging").join("windows").join("icon.ico")
}

pub fn window_png_path(root: &Path) -> PathBuf {
    root.join("crates")
        .join("etb-gui")
        .join("assets")
        .join(format!("icon-{WINDOW_ICON}.png"))
}

/// Everything the build's icons are, as bytes.
pub struct Generated {
    pub ico: Vec<u8>,
    pub window_png: Vec<u8>,
}

/// Derive both icons from `logo.png`, writing nothing.
///
/// Separated from `run` so `check-hygiene` can regenerate and compare without
/// touching the working tree — which is what turns "one source, one command"
/// from a claim in a commit message into something CI enforces.
pub fn generate(root: &Path) -> Result<Generated> {
    let source = root.join("logo.png");
    let logo = image::open(&source)
        .with_context(|| format!("reading {}", source.display()))?
        .into_rgba8();

    // Encoded once, then used twice: 128 is one of the .ico sizes, so resizing
    // and re-encoding it for the window would be a second Lanczos pass over the
    // same source -- and would leave the shipped PNG and the .ico's own 128
    // frame as different bytes for the same picture.
    let mut encoded: Vec<(u32, Vec<u8>)> = Vec::new();
    for &size in ICO_SIZES {
        let scaled = image::imageops::resize(&logo, size, size, FilterType::Lanczos3);
        encoded.push((size, encode_png(&scaled)?));
    }

    let frames: Vec<IcoFrame> = encoded
        .iter()
        .map(|(size, png)| {
            IcoFrame::with_encoded(
                png.as_slice(),
                *size,
                *size,
                image::ExtendedColorType::Rgba8,
            )
        })
        .collect::<std::result::Result<_, _>>()?;

    let mut ico = Vec::new();
    IcoEncoder::new(&mut ico)
        .encode_images(&frames)
        .context("encoding the .ico")?;

    let window_png = encoded
        .iter()
        .find(|(size, _)| *size == WINDOW_ICON)
        .expect("WINDOW_ICON is one of ICO_SIZES")
        .1
        .clone();

    Ok(Generated { ico, window_png })
}

/// Encode one image as PNG, the same way every time.
///
/// `Best` with an adaptive filter, because these are written once by a
/// maintainer and downloaded by everyone: the bytes are worth the second.
fn encode_png(img: &image::RgbaImage) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new_with_quality(
        &mut out,
        image::codecs::png::CompressionType::Best,
        image::codecs::png::FilterType::Adaptive,
    )
    .write_image(
        img,
        img.width(),
        img.height(),
        image::ExtendedColorType::Rgba8,
    )
    .context("encoding a PNG")?;
    Ok(out)
}

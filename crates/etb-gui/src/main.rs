//! Easy Turbo Basic.
//!
//! One window, one green button. The design constraint that decides every
//! argument: someone with no command-line experience must be able to open this,
//! click one button and see their program run — and must never be able to damage
//! their source files.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod fonts;
mod markdown;
mod theme;

/// The one thing this program does without opening a window.
///
/// The uninstaller calls it before it removes anything: the work trees and the
/// copy of the compiler live outside the installation directory — they have to,
/// because the installation directory may sit at a path the compiler cannot be
/// given — and an uninstall that left most of a gigabyte behind would be a
/// poor way to say goodbye. It writes nothing but the removal, and says
/// nothing: nobody is watching.
fn clear_scratch_and_exit() -> ! {
    let code = match etb_core::paths::AppPaths::resolve().and_then(|paths| {
        let guard = etb_core::fs_guard::FsGuard::new(paths.write_roots().to_vec())?;
        etb_core::maintenance::clear_scratch(&paths, &guard)
    }) {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("{e}");
            1
        }
    };
    std::process::exit(code)
}

fn main() -> eframe::Result<()> {
    if std::env::args().any(|a| a == "--clear-scratch") {
        clear_scratch_and_exit();
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("ETB_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1120.0, 800.0])
        .with_min_inner_size([760.0, 560.0])
        .with_title("Easy Turbo Basic");
    // eframe decodes it: `image` is already linked in through eframe itself, and
    // `from_png_bytes` converts whatever colour type the file has rather than
    // refusing anything that is not RGBA8. A failure costs the icon and nothing
    // else, which is the right trade for decoration.
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-128.png")) {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Easy Turbo Basic",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}

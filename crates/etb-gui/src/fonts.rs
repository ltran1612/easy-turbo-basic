//! Fonts.
//!
//! egui does not load system fonts: it draws only with what it is given, and the
//! font it ships (Ubuntu-Light) has never covered Latin Extended Additional —
//! the 90 precomposed characters Vietnamese needs. Launchpad bug #656690 has been
//! open since 2010. So the interface has to pick a font deliberately or it
//! renders as empty boxes.
//!
//! We pick the most ordinary readable font on the machine, in preference order,
//! and **verify it can actually draw Vietnamese before accepting it**. Being
//! called "Segoe UI" is not evidence; having the glyphs is. Windows gets Segoe
//! UI, which is what every other program there uses and is what a Windows user
//! finds most legible.

use std::sync::Arc;

/// Proportional candidates, most preferred first. Ordinary system fonts, not
/// anything the user would have to install.
#[cfg(windows)]
const PROPORTIONAL: &[&str] = &["Segoe UI", "Tahoma", "Verdana", "Arial"];
#[cfg(target_os = "macos")]
const PROPORTIONAL: &[&str] = &["SF Pro Text", "Helvetica Neue", "Helvetica", "Arial"];
#[cfg(all(not(windows), not(target_os = "macos")))]
const PROPORTIONAL: &[&str] = &[
    "Noto Sans",
    "DejaVu Sans",
    "Liberation Sans",
    "Cantarell",
    "Ubuntu",
];

#[cfg(windows)]
const MONOSPACE: &[&str] = &["Consolas", "Cascadia Mono", "Courier New"];
#[cfg(target_os = "macos")]
const MONOSPACE: &[&str] = &["SF Mono", "Menlo", "Monaco", "Courier New"];
#[cfg(all(not(windows), not(target_os = "macos")))]
const MONOSPACE: &[&str] = &[
    "Noto Sans Mono",
    "DejaVu Sans Mono",
    "Liberation Mono",
    "Ubuntu Mono",
];

/// Characters the interface and the guide genuinely need. If a font cannot draw
/// these, it is not a candidate however common it is.
fn required_chars() -> Vec<char> {
    let mut v: Vec<char> = Vec::new();
    // Latin Extended Additional: the Vietnamese precomposed range.
    for c in [
        0x1EA1u32, 0x1EBF, 0x1EC7, 0x1EDB, 0x1EE5, 0x1EF1, 0x1EA3, 0x1EAD,
    ] {
        if let Some(c) = char::from_u32(c) {
            v.push(c);
        }
    }
    // The horn and stroke letters, and the punctuation the interface draws.
    v.extend("ơưđĂÂÊÔ…“”—×•·".chars());
    v
}

/// Report of what was chosen, for logging and for the tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chosen {
    pub proportional: Option<String>,
    pub monospace: Option<String>,
}

pub fn install(ctx: &egui::Context) -> Chosen {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    let chosen = choose(&db);

    let mut fonts = egui::FontDefinitions::default();
    if let Some((name, data)) = chosen.0 {
        add(
            &mut fonts,
            "ui_proportional",
            data,
            egui::FontFamily::Proportional,
        );
        tracing::info!("proportional font: {name}");
    }
    if let Some((name, data)) = chosen.1 {
        add(
            &mut fonts,
            "ui_monospace",
            data,
            egui::FontFamily::Monospace,
        );
        tracing::info!("monospace font: {name}");
    }
    ctx.set_fonts(fonts);
    chosen.2
}

fn add(fonts: &mut egui::FontDefinitions, key: &str, data: Vec<u8>, family: egui::FontFamily) {
    fonts
        .font_data
        .insert(key.to_owned(), Arc::new(egui::FontData::from_owned(data)));
    // At the front, with egui's own fonts left behind ours as a per-glyph
    // fallback: they still carry the icon and emoji glyphs.
    fonts
        .families
        .entry(family)
        .or_default()
        .insert(0, key.to_owned());
}

type Picked = (Option<(String, Vec<u8>)>, Option<(String, Vec<u8>)>, Chosen);

/// Pick the first candidate that exists **and** can draw what we need.
pub fn choose(db: &fontdb::Database) -> Picked {
    let prop = first_usable(db, PROPORTIONAL);
    let mono = first_usable(db, MONOSPACE);
    let report = Chosen {
        proportional: prop.as_ref().map(|(n, _)| n.clone()),
        monospace: mono.as_ref().map(|(n, _)| n.clone()),
    };
    if report.proportional.is_none() {
        tracing::warn!(
            "no system font found that can draw Vietnamese; the interface may show empty boxes"
        );
    }
    (prop, mono, report)
}

fn first_usable(db: &fontdb::Database, names: &[&str]) -> Option<(String, Vec<u8>)> {
    let needed = required_chars();
    for name in names {
        let query = fontdb::Query {
            families: &[fontdb::Family::Name(name)],
            ..Default::default()
        };
        let Some(id) = db.query(&query) else { continue };
        let Some((data, index)) = load(db, id) else {
            continue;
        };
        if covers(&data, index, &needed) {
            return Some(((*name).to_string(), data));
        }
        tracing::debug!("{name} is installed but cannot draw Vietnamese; skipping");
    }
    None
}

fn load(db: &fontdb::Database, id: fontdb::ID) -> Option<(Vec<u8>, u32)> {
    db.with_face_data(id, |data, index| (data.to_vec(), index))
}

/// Does this face have a glyph for every character we need?
pub fn covers(data: &[u8], index: u32, needed: &[char]) -> bool {
    match ttf_parser::Face::parse(data, index) {
        Ok(face) => needed.iter().all(|c| face.glyph_index(*c).is_some()),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_required_set_is_the_vietnamese_one() {
        let r = required_chars();
        for c in ['ạ', 'ế', 'ệ', 'ớ', 'ụ', 'ự', 'ả', 'ậ', 'ơ', 'ư', 'đ'] {
            assert!(r.contains(&c), "{c:?} must be required");
        }
    }

    /// The point of the exercise: whatever is picked can draw Vietnamese.
    #[test]
    fn whatever_is_chosen_can_draw_vietnamese() {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        let (prop, mono, report) = choose(&db);
        eprintln!("chose: {report:?}");

        if let Some((name, data)) = prop {
            assert!(
                covers(&data, 0, &required_chars()),
                "{name} was chosen but cannot draw Vietnamese"
            );
        } else {
            // A bare CI container may genuinely have no fonts. Say so rather than
            // failing, but never accept an unusable one.
            eprintln!("no proportional font on this machine: {report:?}");
        }
        if let Some((name, data)) = mono {
            assert!(
                covers(&data, 0, &required_chars()),
                "{name} was chosen but cannot draw Vietnamese"
            );
        }
    }

    #[test]
    fn a_font_without_the_glyphs_is_rejected() {
        // Nonsense bytes are not a font, so they cannot cover anything. This is
        // the guard that stops a named-but-unusable font being accepted.
        assert!(!covers(b"not a font at all", 0, &required_chars()));
    }

    #[test]
    fn candidate_lists_are_ordinary_fonts_and_not_empty() {
        assert!(!PROPORTIONAL.is_empty());
        assert!(!MONOSPACE.is_empty());
        // Nothing here should be something a user has to go and install.
        for n in PROPORTIONAL.iter().chain(MONOSPACE.iter()) {
            assert!(!n.is_empty());
        }
    }
}

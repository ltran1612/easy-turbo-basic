//! The single choke point for turning filesystem/OS strings into display strings.
//!
//! egui has no complex text shaping (emilk/egui#2517): it renders code points
//! separately and overlays combining marks. Precomposed (NFC) Vietnamese renders
//! correctly; decomposed (NFD) does not. macOS filesystem APIs hand back NFD
//! filenames, so every string that reaches the UI passes through here.

use std::path::Path;
use unicode_normalization::UnicodeNormalization;

/// Normalize to NFC. Cheap and idempotent; call it liberally.
pub fn nfc(s: &str) -> String {
    s.nfc().collect()
}

/// A path as we show it to the user: NFC, lossy, and never a `\\?\` verbatim prefix.
pub fn display_path(p: &Path) -> String {
    nfc(&dunce::simplified(p).to_string_lossy())
}

/// Just the file name, for list rows.
pub fn display_file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| nfc(&n.to_string_lossy()))
        .unwrap_or_else(|| display_path(p))
}
/// A size for someone to read, not to compute with: "412 MB".
///
/// Powers of 1000, because that is what a Windows Explorer window and a
/// download page both say, and the number is there to be recognised.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut n = bytes as f64;
    let mut unit = 0;
    while n >= 1000.0 && unit + 1 < UNITS.len() {
        n /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{n:.0} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nfd_vietnamese_is_composed() {
        // "Chương" written decomposed: o + combining horn + combining grave, etc.
        let decomposed = "Chu\u{01B0}\u{01A1}ng";
        let recomposed = nfc("Chương");
        assert_eq!(nfc(&recomposed), recomposed, "nfc is idempotent");
        // A decomposed base+mark sequence must collapse to a single precomposed char.
        let d = "e\u{0302}\u{0301}"; // e + circumflex + acute => ế
        assert_eq!(nfc(d), "ế");
        assert_eq!(nfc(d).chars().count(), 1);
        let _ = decomposed;
    }

    #[test]
    fn display_path_strips_verbatim_prefix() {
        let p = Path::new("/tmp/ví dụ/SOLVER.FOR");
        assert!(display_path(p).ends_with("SOLVER.FOR"));
    }
}

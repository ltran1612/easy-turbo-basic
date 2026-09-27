//! Splitting a source file into physical lines.
//!
//! Line numbers are the currency of everything downstream: QB64 reports errors
//! by line, the saved program reports run-time errors by line, and the user
//! finds the line in Notepad by counting. So this module has one job: produce
//! exactly the lines a text editor would show, numbered the way it numbers them,
//! and remember each line's own ending so an untouched line can be copied back
//! out byte for byte.

/// DOS end-of-file marker. Editors of the period wrote one after the last line,
/// and anything after it was never part of the program.
pub const CTRL_Z: u8 = 0x1A;

const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysLine<'a> {
    /// The line's bytes, without its ending.
    pub text: &'a [u8],
    /// `\r\n`, `\n`, `\r`, or empty for a last line that has none.
    pub ending: &'a [u8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Physical<'a> {
    pub lines: Vec<PhysLine<'a>>,
    /// Something other than blank space followed the Ctrl-Z. Worth a warning:
    /// it was not part of the program then either, but the user may think it is.
    pub text_after_ctrl_z: bool,
}

pub fn split(bytes: &[u8]) -> Physical<'_> {
    let bytes = bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes);
    let (body, after) = match bytes.iter().position(|&b| b == CTRL_Z) {
        Some(i) => (&bytes[..i], &bytes[i + 1..]),
        None => (bytes, &[][..]),
    };
    let text_after_ctrl_z = after
        .iter()
        .any(|&b| !matches!(b, b' ' | b'\t' | b'\r' | b'\n' | CTRL_Z));

    let mut lines = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < body.len() {
        match body[i] {
            b'\n' => {
                lines.push(PhysLine {
                    text: &body[start..i],
                    ending: &body[i..i + 1],
                });
                i += 1;
                start = i;
            }
            b'\r' => {
                let end = if body.get(i + 1) == Some(&b'\n') {
                    i + 2
                } else {
                    i + 1
                };
                lines.push(PhysLine {
                    text: &body[start..i],
                    ending: &body[i..end],
                });
                i = end;
                start = i;
            }
            _ => i += 1,
        }
    }
    if start < body.len() {
        lines.push(PhysLine {
            text: &body[start..],
            ending: &[],
        });
    }
    Physical {
        lines,
        text_after_ctrl_z,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(p: &Physical) -> Vec<String> {
        p.lines
            .iter()
            .map(|l| String::from_utf8_lossy(l.text).into_owned())
            .collect()
    }

    #[test]
    fn lines_are_numbered_the_way_an_editor_numbers_them() {
        // A leading blank line is line 1. The QB64 error that started this
        // project was reported one line lower than a count that skipped it
        // would give.
        let p = split(b"\r\ncolor 14,1\r\nend\r\n");
        assert_eq!(texts(&p), ["", "color 14,1", "end"]);
    }

    #[test]
    fn each_line_keeps_its_own_ending() {
        let p = split(b"a\r\nb\nc\rd");
        let endings: Vec<&[u8]> = p.lines.iter().map(|l| l.ending).collect();
        assert_eq!(endings, [&b"\r\n"[..], b"\n", b"\r", b""]);
        assert_eq!(texts(&p), ["a", "b", "c", "d"]);
    }

    #[test]
    fn everything_from_ctrl_z_on_is_not_the_program() {
        let p = split(b"end\r\n\r\n\x1a");
        assert_eq!(texts(&p), ["end", ""]);
        assert!(!p.text_after_ctrl_z);

        let p = split(b"end\r\n\x1aold junk\r\n");
        assert_eq!(texts(&p), ["end"]);
        assert!(
            p.text_after_ctrl_z,
            "leftover text after Ctrl-Z is worth saying"
        );
    }

    #[test]
    fn a_utf8_byte_order_mark_is_not_part_of_line_one() {
        let p = split(b"\xEF\xBB\xBFprint 1\r\n");
        assert_eq!(texts(&p), ["print 1"]);
    }

    #[test]
    fn a_final_line_ending_does_not_invent_an_empty_line() {
        assert_eq!(split(b"a\r\n").lines.len(), 1);
        assert_eq!(split(b"").lines.len(), 0);
    }
}

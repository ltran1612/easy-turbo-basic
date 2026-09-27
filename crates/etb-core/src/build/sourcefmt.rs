//! Is this file actually BASIC source text?
//!
//! Handed a file that is not source at all, the compiler does not say so: it
//! reports a syntax error on line 1 and carries on reporting them. That is worse than useless in front of
//! someone who did not write the error — nothing in it says "this is not a
//! program". So a file is judged before anything else looks at it, and the
//! ones we can recognise are named, because naming one tells the user what to
//! do instead.
//!
//! BASIC adds a case of its own: GW-BASIC and QuickBASIC saved programs in a
//! compact binary form unless told otherwise, and a file in that form looks
//! like noise but is one `SAVE` away from being usable.
//!
//! The checks are deliberately narrow. Old source is full of things that look
//! wrong and are not: form feeds between listing pages, a Ctrl-Z end-of-file
//! marker from DOS, tabs, and high-bit bytes from a Vietnamese code page. None
//! of those make a file binary, and flagging them would refuse real work.

use crate::error::{FileProblem, ProblemArg};

/// What a file turned out to be, when it is not BASIC source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotSource {
    Empty,
    /// Saved as Unicode rather than plain text. Recoverable, and worth saying so:
    /// Notepad and Word both offer it, and the result is unreadable to a compiler.
    Utf16,
    /// A format we recognise well enough to name. Holds the i18n key for that name.
    Known(&'static str),
    /// A GW-BASIC or BASICA program saved in its compact binary form, which is
    /// what `SAVE "NAME"` wrote unless `,A` was added.
    GwTokenized,
    /// The same, saved with `,P`: scrambled so it could not be listed.
    GwProtected,
    /// QuickBASIC 4.x's own binary save format.
    QbBinary,
    /// Binary by content: a NUL byte, which no text file contains.
    Binary,
}

impl NotSource {
    pub fn key(&self) -> &'static str {
        match self {
            NotSource::Empty => "src.reject.empty",
            NotSource::Utf16 => "src.reject.utf16",
            NotSource::Known(_) => "src.reject.known",
            NotSource::GwTokenized => "src.reject.gw_tokenized",
            NotSource::GwProtected => "src.reject.gw_protected",
            NotSource::QbBinary => "src.reject.qb_binary",
            NotSource::Binary => "src.reject.binary",
        }
    }

    /// The finished, translatable description of this refusal.
    pub fn problem(&self, name: String) -> FileProblem {
        let args = match self {
            // The name of the format is itself prose, so it is translated too.
            NotSource::Known(what_key) => vec![("what", ProblemArg::Key(what_key))],
            _ => Vec::new(),
        };
        FileProblem {
            name,
            title_key: "src.unusable_title",
            reason_key: self.key(),
            args,
        }
    }

    pub fn english(&self) -> String {
        match self {
            NotSource::Empty => "the file is empty.".into(),
            NotSource::Utf16 => "saved as Unicode (UTF-16) rather than plain text. Open it in \
                 Notepad and save it again with encoding set to ANSI or UTF-8."
                .into(),
            // The format's name is a catalog entry, so English takes it from the
            // English catalog rather than keeping a second copy here. The sentence
            // around it stays in this file's own technical register.
            NotSource::Known(k) => format!(
                "a {}, not BASIC source text.",
                crate::i18n::lookup(crate::i18n::Lang::En, k)
            ),
            NotSource::GwTokenized => "a GW-BASIC program saved in binary (tokenised) form. \
                 Load it in GW-BASIC or PC-BASIC and SAVE it again with ,A to get text."
                .into(),
            NotSource::GwProtected => "a GW-BASIC program saved protected (,P). PC-BASIC can \
                 load it and SAVE it again with ,A to get text."
                .into(),
            NotSource::QbBinary => "a QuickBASIC 4.x binary save. Open it in QuickBASIC \
                 itself and save it as text (File, Save As, Text)."
                .into(),
            NotSource::Binary => "not a text file, so it cannot be BASIC source.".into(),
        }
    }
}

/// Where a signature has to appear for it to count.
#[derive(Debug, Clone, Copy)]
enum At {
    /// The very start of the file.
    Start,
    /// Anywhere in the first `n` bytes: some formats print a banner line first.
    Within(usize),
}

/// Signatures worth naming, because naming one tells the user what to do instead.
///
/// The name is an i18n key rather than English prose: it lands in the `{what}`
/// slot of a translated sentence, so English here would strand an English
/// fragment in the middle of a Vietnamese one.
const SIGNATURES: &[(&[u8], At, &str)] = &[
    (b"%PDF-", At::Start, "src.what.pdf"),
    (b"{\\rtf", At::Start, "src.what.rtf"),
    (b"PK\x03\x04", At::Start, "src.what.zip"),
    (
        b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1",
        At::Start,
        "src.what.office",
    ),
    (b"MZ", At::Start, "src.what.windows_program"),
    (b"\x7fELF", At::Start, "src.what.linux_program"),
    // A real case from the same kind of legacy folder: an engineering
    // application's data file with a source-code extension.
    (b"FAS4-FILE", At::Within(64), "src.what.fas4"),
];

/// How much of the file to judge by. A source file declares itself early.
const WINDOW: usize = 8192;

pub fn check(bytes: &[u8]) -> Result<(), NotSource> {
    if bytes.is_empty() {
        return Err(NotSource::Empty);
    }
    // A UTF-16 BOM. Checked before the NUL scan, which would otherwise call this
    // binary and give advice the user cannot act on — and before the GW-BASIC
    // checks, because FF FE is also a tokenised program's first byte.
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        return Err(NotSource::Utf16);
    }
    for (sig, at, what) in SIGNATURES {
        let hit = match at {
            At::Start => bytes.starts_with(sig),
            At::Within(n) => bytes[..bytes.len().min(*n)]
                .windows(sig.len())
                .any(|w| w == *sig),
        };
        if hit {
            return Err(NotSource::Known(what));
        }
    }
    let window = &bytes[..bytes.len().min(WINDOW)];
    let has_nul = window.contains(&0);

    // Binary BASIC saves. Each needs a NUL as well as its first byte: 0xFE and
    // 0xFF are letters in the Vietnamese code pages, and a first byte alone is
    // not worth refusing someone's text over. A real tokenised program has a
    // NUL at the end of its first line at the latest.
    if has_nul {
        match bytes[0] {
            0xFF => return Err(NotSource::GwTokenized),
            0xFE => return Err(NotSource::GwProtected),
            0xFC if bytes.get(1) == Some(&0) => return Err(NotSource::QbBinary),
            _ => {}
        }
        // A NUL byte settles it. Nothing else does: form feeds separate listing
        // pages, Ctrl-Z ends a DOS text file, and a Vietnamese code page fills
        // the high half of the byte range with perfectly ordinary letters.
        return Err(NotSource::Binary);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------ real source passes
    //
    // These matter more than the rejections. A false positive here refuses work
    // the user is entitled to do, on a file that is perfectly good.

    #[test]
    fn ordinary_source_is_accepted() {
        assert_eq!(check(b"10 PRINT \"HELLO\"\r\n20 END\r\n"), Ok(()));
    }

    #[test]
    fn the_things_old_source_is_full_of_are_not_binary() {
        // A form feed between listing pages.
        assert_eq!(check(b"PRINT 1\n\x0cEND\n"), Ok(()));
        // A DOS Ctrl-Z end-of-file marker.
        assert_eq!(check(b"END\r\n\x1a"), Ok(()));
        // Tabs.
        assert_eq!(check(b"\tPRINT 1\n\tEND\n"), Ok(()));
        // CRLF, and a lone CR from a Mac-era editor.
        assert_eq!(check(b"PRINT 1\r\nEND\r\n"), Ok(()));
        assert_eq!(check(b"PRINT 1\rEND\r"), Ok(()));
        // A UTF-8 BOM, which is not UTF-16 and is harmless.
        assert_eq!(check(b"\xef\xbb\xbfEND\n"), Ok(()));
    }

    #[test]
    fn a_vietnamese_code_page_is_text_however_high_its_bytes() {
        // The code pages fill the high half of the byte range with ordinary
        // letters. Judging "binary" by the high bit would reject their own text.
        let mut src = b"PRINT \"T\xednh d\xe2\xf9 b\xea t\xf4ng\"\nEND\n".to_vec();
        assert_eq!(check(&src), Ok(()));
        // ...including one that opens with a byte a tokenised save also opens
        // with. Without a NUL it is text.
        for first in [0xFF, 0xFE, 0xFC, 0xF0] {
            src.insert(0, first);
            assert_eq!(check(&src), Ok(()), "{first:#x}");
            src.remove(0);
        }
    }

    // ------------------------------------------------------------- rejections

    #[test]
    fn a_fas4_file_is_named_rather_than_parsed_as_source() {
        let b = b"\r\n FAS4-FILE ; Do not change it!\r\n1295\r\n108 $\x14\x01\x01\x01\x00";
        assert_eq!(check(b), Err(NotSource::Known("src.what.fas4")));
    }

    #[test]
    fn a_file_saved_as_unicode_gets_advice_the_user_can_act_on() {
        // Notepad and Word both offer this, and the result compiles to nonsense.
        let mut b = vec![0xFF, 0xFE];
        for c in "END\n".bytes() {
            b.push(c);
            b.push(0);
        }
        assert_eq!(check(&b), Err(NotSource::Utf16));
        assert_ne!(check(&b), Err(NotSource::GwTokenized));
    }

    #[test]
    fn binary_basic_saves_are_named_so_the_advice_can_be_one_save_away() {
        // `10 PRINT 1` tokenised: FF, link pointer, line number 10, PRINT's
        // token, the constant, the line's terminating NUL.
        let gw = b"\xff\x0b\x08\x0a\x00\x91\x20\x12\x00\x00\x00";
        assert_eq!(check(gw), Err(NotSource::GwTokenized));
        assert_eq!(check(b"\xfe\x81\x23\x00\x44"), Err(NotSource::GwProtected));
        assert_eq!(check(b"\xfc\x00\x01\x00\x0c"), Err(NotSource::QbBinary));
    }

    #[test]
    fn documents_chosen_by_mistake_are_named() {
        assert_eq!(check(b"%PDF-1.4\n"), Err(NotSource::Known("src.what.pdf")));
        assert_eq!(
            check(b"PK\x03\x04\x14\x00"),
            Err(NotSource::Known("src.what.zip"))
        );
        assert_eq!(
            check(b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1"),
            Err(NotSource::Known("src.what.office"))
        );
        assert_eq!(
            check(b"MZ\x90\x00"),
            Err(NotSource::Known("src.what.windows_program"))
        );
    }

    #[test]
    fn a_nul_byte_settles_it() {
        assert_eq!(check(b"PRI\x00NT 1\n"), Err(NotSource::Binary));
    }

    #[test]
    fn an_empty_file_says_so_rather_than_failing_obscurely() {
        assert_eq!(check(b""), Err(NotSource::Empty));
    }

    #[test]
    fn every_rejection_carries_a_key_and_plain_english() {
        for p in ALL {
            assert!(p.key().starts_with("src.reject."));
            assert!(!p.english().is_empty());
        }
    }

    pub(super) const ALL: [NotSource; 7] = [
        NotSource::Empty,
        NotSource::Utf16,
        NotSource::Known("src.what.pdf"),
        NotSource::GwTokenized,
        NotSource::GwProtected,
        NotSource::QbBinary,
        NotSource::Binary,
    ];

    #[test]
    fn arbitrary_bytes_never_panic() {
        let mut seed = 0x5bf03635u32;
        for _ in 0..3000 {
            let mut b = Vec::new();
            for _ in 0..48 {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                b.push((seed >> 16) as u8);
            }
            let _ = check(&b);
        }
    }
}

/// Every key these enums can name must exist in both catalogs.
///
/// The xtask key check greps literal `tr!` call sites, and cannot see a key
/// chosen at run time — which is every key in this module. They reach the
/// interface through `problem.reason_key`, so without this they were the only
/// user-facing strings nothing verified.
#[cfg(test)]
mod key_coverage {
    use crate::error::ProblemArg;
    use crate::i18n::{keys, Lang};

    /// Checked against each catalog's own key set, NOT through `lookup`.
    ///
    /// `lookup` falls back to English when a Vietnamese key is missing, so asking
    /// it whether a key resolves can never detect the failure that matters here —
    /// the interface silently reverting to English in front of someone who does
    /// not read it. This test was written that way first and passed happily with
    /// a key deleted from vi.toml.
    fn assert_resolves(key: &str) {
        for lang in [Lang::Vi, Lang::En] {
            assert!(
                keys(lang).contains(&key),
                "{key} is missing from the {lang:?} catalog, so it would silently \
                 fall back to the other language"
            );
        }
    }

    #[test]
    fn every_source_rejection_resolves_in_both_languages() {
        for p in super::tests::ALL {
            assert_resolves(p.key());
            assert_resolves(p.problem("x".into()).title_key);
            // and the nouns that fill `{what}` are catalog entries too, which is
            // what stops an English phrase landing inside a Vietnamese sentence
            for (_, arg) in p.problem("x".into()).args {
                if let ProblemArg::Key(k) = arg {
                    assert_resolves(k);
                }
            }
        }
    }

    #[test]
    fn every_named_file_format_resolves_in_both_languages() {
        for (_, _, key) in super::SIGNATURES {
            assert_resolves(key);
        }
    }
}

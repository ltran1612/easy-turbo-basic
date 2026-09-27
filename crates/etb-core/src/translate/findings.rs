//! What the translator has to say about a program, in a form the interface can
//! put into the user's language.

use crate::error::ProblemArg;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Severity {
    /// The program cannot be built as written. Every one of these is reported
    /// together, before the compiler runs: what it reports is about the
    /// the only place a user can learn about all of them at once.
    Refuse,
    /// It builds, but will not behave as it did under Turbo Basic.
    Warn,
    /// Something was adjusted that the user may want to know about.
    Note,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub severity: Severity,
    /// i18n key: `tr.refuse.*`, `tr.warn.*` or `tr.note.*`.
    pub key: &'static str,
    /// The user's file, by the name they know it by.
    pub file: String,
    /// 1-based line in that file, when the finding is about one line.
    pub line: Option<u32>,
    pub args: Vec<(&'static str, ProblemArg)>,
}

/// Every key a finding can carry, so a test can check both catalogs have them.
///
/// Findings choose their key at run time, which is beyond what
/// `check-hygiene`'s scan of literal `tr!` keys can see.
pub const ALL_KEYS: &[&str] = &[
    "tr.warn.text_after_ctrl_z",
    "tr.refuse.call_interrupt",
    "tr.refuse.call_absolute",
    "tr.refuse.inline",
    "tr.refuse.mbf",
    "tr.refuse.reg",
    "tr.refuse.endmem",
    "tr.refuse.eradr",
    "tr.refuse.jump_into_def",
    "tr.refuse.def_without_end",
    "tr.refuse.exit_if_outside_block",
    "tr.refuse.jump_into_sub",
    "tr.refuse.sub_without_end",
    "tr.refuse.include_missing",
    "tr.refuse.include_not_text",
    "tr.refuse.include_cycle",
    "tr.refuse.if_unknown_const",
    "tr.refuse.if_unbalanced",
    "tr.note.meta_ignored",
    "tr.note.memset_ignored",
];

#[cfg(test)]
mod tests {
    use crate::i18n::{keys, Lang};

    /// Against each catalog's own keys, not through `lookup`, which falls back
    /// to English and so cannot see a missing Vietnamese key.
    #[test]
    fn every_finding_key_is_in_both_catalogs() {
        for key in super::ALL_KEYS {
            for lang in [Lang::Vi, Lang::En] {
                assert!(keys(lang).contains(key), "{key} missing from {lang:?}");
            }
        }
    }
}

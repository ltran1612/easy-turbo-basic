//! What type a name without a suffix has, at each point in the source.
//!
//! Turbo Basic decides this at compile time by where `DEFINT`, `DEFLNG`,
//! `DEFSNG`, `DEFDBL` and `DEFSTR` appear in the file, not by the order they
//! run in (Owner's Handbook, the DEFtype entry). The translator needs the same
//! answer whenever it moves code or names something itself: a name moved to
//! where a different DEFtype is in force would silently become a different
//! variable. So moved and generated code always carries an explicit suffix,
//! and this is where the suffix comes from.

use super::lexer::{Kind, Token};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ty {
    Int,
    Lng,
    Sng,
    Dbl,
    Str,
}

impl Ty {
    pub fn suffix(self) -> char {
        match self {
            Ty::Int => '%',
            Ty::Lng => '&',
            Ty::Sng => '!',
            Ty::Dbl => '#',
            Ty::Str => '$',
        }
    }

    pub fn from_suffix(c: u8) -> Option<Ty> {
        Some(match c {
            b'%' => Ty::Int,
            b'&' => Ty::Lng,
            b'!' => Ty::Sng,
            b'#' => Ty::Dbl,
            b'$' => Ty::Str,
            _ => return None,
        })
    }
}

/// The type of every initial letter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefTypes([Ty; 26]);

impl Default for DefTypes {
    /// "By default, when the compiler finds a variable name without a type
    /// identifier, it assumes the variable to be single precision."
    fn default() -> Self {
        Self([Ty::Sng; 26])
    }
}

impl DefTypes {
    /// The type a name without a suffix gets from its first letter.
    pub fn of_letter(&self, c: u8) -> Ty {
        match c.to_ascii_uppercase() {
            l @ b'A'..=b'Z' => self.0[usize::from(l - b'A')],
            _ => Ty::Sng,
        }
    }

    /// Apply one statement, if it is a DEFtype. Returns whether it was one.
    ///
    /// `DEFINT A-M, X` is lexed as `DEFINT`, `A`, `-`, `M`, `,`, `X`.
    pub fn apply(&mut self, line: &[u8], toks: &[Token]) -> bool {
        let Some(first) = toks.first() else {
            return false;
        };
        let ty = match first.text(line).to_ascii_uppercase().as_slice() {
            b"DEFINT" => Ty::Int,
            b"DEFLNG" => Ty::Lng,
            b"DEFSNG" => Ty::Sng,
            b"DEFDBL" => Ty::Dbl,
            b"DEFSTR" => Ty::Str,
            _ => return false,
        };
        let letter = |t: &Token| -> Option<u8> {
            let s = t.text(line);
            (t.kind == Kind::Ident && s.len() == 1 && s[0].is_ascii_alphabetic())
                .then(|| s[0].to_ascii_uppercase())
        };
        let mut i = 1;
        while i < toks.len() {
            if let Some(from) = letter(&toks[i]) {
                let mut to = from;
                if toks.get(i + 1).is_some_and(|t| t.text(line) == b"-") {
                    if let Some(end) = toks.get(i + 2).and_then(letter) {
                        to = end;
                        i += 2;
                    }
                }
                for l in from.min(to)..=from.max(to) {
                    self.0[usize::from(l - b'A')] = ty;
                }
            }
            i += 1;
        }
        true
    }
}

/// The name without its suffix, and its type — from the suffix if it has one,
/// otherwise from `types`. `letter_at` is the index of the letter DEFtype
/// goes by: 0 for a variable; for `FNname`, the first letter of the name.
pub fn resolve(ident: &[u8], types: &DefTypes, letter_at: usize) -> (String, Ty, bool) {
    let (base, explicit) = match ident.last().copied().and_then(Ty::from_suffix) {
        Some(t) => (&ident[..ident.len() - 1], Some(t)),
        None => (ident, None),
    };
    let base_str = String::from_utf8_lossy(base).into_owned();
    match explicit {
        Some(t) => (base_str, t, true),
        None => {
            let c = base.get(letter_at).copied().unwrap_or(b'A');
            (base_str, types.of_letter(c), false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::lexer::lex;
    use super::*;

    fn after(stmts: &[&str]) -> DefTypes {
        let mut d = DefTypes::default();
        for s in stmts {
            let l = lex(s.as_bytes());
            assert!(d.apply(s.as_bytes(), &l.tokens), "{s}");
        }
        d
    }

    #[test]
    fn everything_is_single_until_told_otherwise() {
        let d = DefTypes::default();
        assert_eq!(d.of_letter(b'x'), Ty::Sng);
    }

    #[test]
    fn ranges_single_letters_and_lists() {
        let d = after(&["DEFINT A-C, x", "defdbl m"]);
        for c in *b"abcABCx" {
            assert_eq!(d.of_letter(c), Ty::Int, "{}", c as char);
        }
        assert_eq!(d.of_letter(b'M'), Ty::Dbl);
        assert_eq!(d.of_letter(b'd'), Ty::Sng);
    }

    #[test]
    fn a_suffix_beats_the_letter() {
        let d = after(&["DEFSTR A-Z"]);
        assert_eq!(resolve(b"total#", &d, 0), ("total".into(), Ty::Dbl, true));
        assert_eq!(resolve(b"total", &d, 0), ("total".into(), Ty::Str, false));
    }

    #[test]
    fn a_function_goes_by_the_letter_after_fn() {
        let d = after(&["DEFINT F", "DEFDBL A"]);
        assert_eq!(resolve(b"FNArea", &d, 2).1, Ty::Dbl);
    }

    #[test]
    fn other_statements_are_not_deftypes() {
        let mut d = DefTypes::default();
        let l = lex(b"DEF FNa(x) = x");
        assert!(!d.apply(b"DEF FNa(x) = x", &l.tokens));
    }
}

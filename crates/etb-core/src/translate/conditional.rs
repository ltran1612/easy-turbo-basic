//! Named constants, and the conditional compilation they drive.
//!
//! `%name = 12` names an integer constant (Owner's Handbook, "Named
//! Constants"): assigned once, to a constant, and quite separate from the
//! variable `name%`. `$IF %name` … `$ELSE` … `$ENDIF` compiles one branch or
//! the other depending on whether a constant is nonzero.
//!
//! Both are settled here, in source order, before anything else looks at the
//! program: a line in a branch that is not compiled defines nothing — no
//! function, no constant — so it must be out of the way before the analysis
//! runs. The translator resolves the conditions itself rather than handing
//! them on, which keeps QB64's own `$IF`, a different language, out of it.

use super::lexer::{self, Kind, Token};
use super::stmt;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conditional {
    /// Whether each line is compiled.
    pub active: Vec<bool>,
    /// The `$IF`, `$ELSE` and `$ENDIF` lines themselves.
    pub directive: Vec<bool>,
    /// Each named constant, upper-cased, and its value as written.
    pub consts: HashMap<String, String>,
    /// (line, i18n key) for what could not be settled.
    pub problems: Vec<(usize, &'static str)>,
}

/// Nesting deeper than the handbook allows is a sign of something else wrong.
const MAX_DEPTH: usize = 256;

pub fn settle(lines: &[&[u8]]) -> Conditional {
    let mut c = Conditional {
        active: vec![true; lines.len()],
        directive: vec![false; lines.len()],
        consts: HashMap::new(),
        problems: Vec::new(),
    };
    // One entry per open $IF: was the enclosing code active, and is this
    // branch being compiled.
    let mut stack: Vec<(bool, bool)> = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let lexed = lexer::lex(line);
        let enclosing = stack.last().is_none_or(|&(outer, taken)| outer && taken);

        if let Some(meta) = lexed.tokens.iter().find(|t| t.kind == Kind::Meta) {
            let text = meta.text(line);
            match directive_word(text).as_str() {
                "IF" => {
                    c.directive[i] = true;
                    c.active[i] = false;
                    let cond = match condition(&text[3..], &c.consts) {
                        Some(v) => v,
                        None => {
                            if enclosing {
                                c.problems.push((i, "tr.refuse.if_unknown_const"));
                            }
                            false
                        }
                    };
                    if stack.len() >= MAX_DEPTH {
                        c.problems.push((i, "tr.refuse.if_unbalanced"));
                    }
                    stack.push((enclosing, cond));
                    continue;
                }
                "ELSE" => {
                    c.directive[i] = true;
                    c.active[i] = false;
                    match stack.last_mut() {
                        Some(top) => top.1 = !top.1,
                        None => c.problems.push((i, "tr.refuse.if_unbalanced")),
                    }
                    continue;
                }
                "ENDIF" => {
                    c.directive[i] = true;
                    c.active[i] = false;
                    if stack.pop().is_none() {
                        c.problems.push((i, "tr.refuse.if_unbalanced"));
                    }
                    continue;
                }
                _ => {}
            }
        }

        c.active[i] = enclosing;
        if enclosing {
            for s in stmt::split(line, &lexed) {
                if let Some((name, value)) = definition(line, s.tokens(&lexed.tokens)) {
                    c.consts.insert(name, value);
                }
            }
        }
    }
    if !stack.is_empty() {
        c.problems
            .push((lines.len().saturating_sub(1), "tr.refuse.if_unbalanced"));
    }
    c
}

/// `$IF`, `$ELSE`, `$ENDIF`, upper-cased; anything else as it comes.
fn directive_word(meta: &[u8]) -> String {
    meta.iter()
        .skip(1)
        .take_while(|b| b.is_ascii_alphabetic())
        .map(|b| b.to_ascii_uppercase() as char)
        .collect()
}

/// `%NAME = 12`, `%NAME = -1`, `%NAME = &H1F`: the name and the value.
pub fn definition(line: &[u8], toks: &[Token]) -> Option<(String, String)> {
    match toks {
        [pct, name, eq, rest @ ..]
            if pct.text(line) == b"%"
                && name.kind == Kind::Ident
                && eq.text(line) == b"="
                && !rest.is_empty() =>
        {
            let (a, b) = (rest[0].start, rest[rest.len() - 1].end);
            let value = String::from_utf8_lossy(&line[a..b]).trim().to_string();
            integer(&value)?;
            Some((
                String::from_utf8_lossy(name.text(line)).to_ascii_uppercase(),
                value,
            ))
        }
        _ => None,
    }
}

/// The value of an integer constant as Turbo Basic writes one.
pub fn integer(s: &str) -> Option<i64> {
    let s = s.trim();
    let (neg, s) = match s.strip_prefix('-') {
        Some(r) => (true, r.trim()),
        None => (false, s.strip_prefix('+').unwrap_or(s).trim()),
    };
    let s = s.trim_end_matches(['%', '&']);
    let upper = s.to_ascii_uppercase();
    let v = if let Some(h) = upper.strip_prefix("&H") {
        i64::from_str_radix(h, 16).ok()?
    } else if let Some(o) = upper.strip_prefix("&O") {
        i64::from_str_radix(o, 8).ok()?
    } else if let Some(b) = upper.strip_prefix("&B") {
        i64::from_str_radix(b, 2).ok()?
    } else if let Some(o) = upper.strip_prefix('&') {
        i64::from_str_radix(o, 8).ok()?
    } else {
        upper.parse().ok()?
    };
    Some(if neg { -v } else { v })
}

/// `$IF %name` or `$IF 1`: true when nonzero; `None` when unknown.
fn condition(rest: &[u8], consts: &HashMap<String, String>) -> Option<bool> {
    let text = String::from_utf8_lossy(rest);
    // Stop at a comment.
    let text = text.split('\'').next().unwrap_or("").trim();
    let value = match text.strip_prefix('%') {
        Some(name) => integer(consts.get(&name.trim().to_ascii_uppercase())?)?,
        None => integer(text)?,
    };
    Some(value != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settle_str(src: &str) -> Conditional {
        let lines: Vec<&[u8]> = src.lines().map(str::as_bytes).collect();
        settle(&lines)
    }

    #[test]
    fn named_constants_are_collected() {
        let c = settle_str("%debug = -1\n%maxx = 319: x = 1\n%mask = &HFF\ndebug% = 12409\n");
        assert_eq!(c.consts["DEBUG"], "-1");
        assert_eq!(c.consts["MAXX"], "319");
        assert_eq!(c.consts["MASK"], "&HFF");
        assert!(!c.consts.contains_key("DEBUG%"));
    }

    #[test]
    fn the_branch_not_taken_is_not_compiled() {
        let c = settle_str(
            "%ColorScreen = 1\n\
             $IF %ColorScreen\n\
             DEF SEG = &HB800\n\
             $ELSE\n\
             DEF SEG = &HB000\n\
             $ENDIF\n\
             PRINT\n",
        );
        assert_eq!(c.active, [true, false, true, false, false, false, true]);
        assert_eq!(c.directive, [false, true, false, true, false, true, false]);
        assert!(c.problems.is_empty());
    }

    #[test]
    fn nested_conditions_and_what_they_define() {
        let c = settle_str("%a = 0\n$IF %a\n%b = 1\n$IF 1\nx\n$ENDIF\n$ELSE\n%b = 2\n$ENDIF\n");
        assert_eq!(
            c.consts["B"], "2",
            "a constant in the branch not taken is not defined"
        );
        assert!(!c.active[4]);
    }

    #[test]
    fn what_cannot_be_settled_is_said() {
        assert_eq!(
            settle_str("$IF %nowhere\n$ENDIF\n").problems[0].1,
            "tr.refuse.if_unknown_const"
        );
        assert_eq!(
            settle_str("$IF 1\nx\n").problems[0].1,
            "tr.refuse.if_unbalanced"
        );
        assert_eq!(
            settle_str("$ENDIF\n").problems[0].1,
            "tr.refuse.if_unbalanced"
        );
    }

    #[test]
    fn integers_in_every_base() {
        assert_eq!(integer("&H1F"), Some(31));
        assert_eq!(integer("&O17"), Some(15));
        assert_eq!(integer("&B101"), Some(5));
        assert_eq!(integer("-12%"), Some(-12));
        assert_eq!(integer("1.5"), None);
    }
}

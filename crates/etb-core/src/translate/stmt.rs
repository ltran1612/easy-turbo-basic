//! Statements within one line.
//!
//! A rewrite like "wrap the file name of this OPEN" only needs to know where a
//! statement starts and ends. A statement ends at a `:` outside parentheses, and
//! the branches of a single-line `IF` are statements of their own: `IF x THEN
//! CLOSE #1 ELSE END` holds three, and the `CLOSE` and the `END` each need their
//! own rewrite.

use super::lexer::{Kind, Lexed, Token};

/// A statement, as a range of indices into the line's token list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stmt {
    pub first: usize,
    /// One past the last token.
    pub end: usize,
}

impl Stmt {
    pub fn tokens<'t>(&self, all: &'t [Token]) -> &'t [Token] {
        &all[self.first..self.end]
    }
    pub fn is_empty(&self) -> bool {
        self.first == self.end
    }
}

/// Split one lexed line into statements.
///
/// A leading line number or label is not part of any statement. A comment is
/// never part of one either, so an edit placed at "the end of the statement"
/// lands before the comment rather than inside it.
pub fn split(line: &[u8], lexed: &Lexed) -> Vec<Stmt> {
    let toks = &lexed.tokens;
    let mut i = 0;
    if toks.first().is_some_and(|t| t.kind == Kind::LineNumber) {
        i = 1;
    } else if toks.len() >= 2
        && toks[0].kind == Kind::Ident
        && toks[1].kind == Kind::Colon
        && !super::keywords::is_reserved(toks[0].text(line))
    {
        // `label:` at the start of a line. Turbo Basic calls procedures with
        // CALL, so a bare name that is not a keyword, followed by a colon, is
        // never a statement. `CLS: PRINT` is two statements.
        i = 2;
    }

    let mut out = Vec::new();
    let mut start = i;
    let mut depth: i32 = 0;
    while i < toks.len() {
        let t = toks[i];
        match t.kind {
            Kind::LParen => depth += 1,
            Kind::RParen => depth = (depth - 1).max(0),
            Kind::Colon if depth == 0 => {
                push(&mut out, start, i);
                start = i + 1;
            }
            Kind::Rem => {
                push(&mut out, start, i);
                start = toks.len();
                break;
            }
            Kind::Ident if depth == 0 && t.is_word(line, "THEN") => {
                push(&mut out, start, i + 1);
                start = i + 1;
            }
            Kind::Ident if depth == 0 && t.is_word(line, "ELSE") => {
                push(&mut out, start, i);
                push(&mut out, i, i + 1);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    push(&mut out, start, toks.len());
    out
}

fn push(out: &mut Vec<Stmt>, first: usize, end: usize) {
    if first < end {
        out.push(Stmt { first, end });
    }
}

/// Split a statement's tokens at commas outside parentheses.
pub fn top_level_commas(toks: &[Token]) -> Vec<&[Token]> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, t) in toks.iter().enumerate() {
        match t.kind {
            Kind::LParen => depth += 1,
            Kind::RParen => depth = (depth - 1).max(0),
            Kind::Comma if depth == 0 => {
                parts.push(&toks[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&toks[start..]);
    parts
}

/// Index of the first token outside parentheses that is one of `words`.
pub fn find_top_level_word(line: &[u8], toks: &[Token], words: &[&str]) -> Option<usize> {
    let mut depth = 0i32;
    for (i, t) in toks.iter().enumerate() {
        match t.kind {
            Kind::LParen => depth += 1,
            Kind::RParen => depth = (depth - 1).max(0),
            Kind::Ident if depth == 0 && words.iter().any(|w| t.is_word(line, w)) => {
                return Some(i)
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::super::lexer::lex;
    use super::*;

    fn stmts(src: &str) -> Vec<String> {
        let line = src.as_bytes();
        let lexed = lex(line);
        split(line, &lexed)
            .iter()
            .map(|s| {
                let toks = s.tokens(&lexed.tokens);
                let a = toks.first().unwrap().start;
                let b = toks.last().unwrap().end;
                src[a..b].to_string()
            })
            .collect()
    }

    #[test]
    fn colons_separate_statements_but_not_inside_parentheses() {
        assert_eq!(stmts("a = 1: b = f(1:2): c"), ["a = 1", "b = f(1:2)", "c"]);
    }

    #[test]
    fn a_single_line_if_holds_its_branches_as_statements() {
        assert_eq!(
            stmts("if x then close #1: end else print 2"),
            ["if x then", "close #1", "end", "else", "print 2"]
        );
    }

    #[test]
    fn line_numbers_labels_and_comments_are_not_statements() {
        assert_eq!(stmts("35 ? ' done"), ["?"]);
        assert_eq!(stmts("again: print 1"), ["print 1"]);
        assert_eq!(stmts("cls: print 1"), ["cls", "print 1"]);
        assert_eq!(stmts("print 1 rem two"), ["print 1"]);
    }

    #[test]
    fn a_goto_target_after_then_is_its_own_harmless_statement() {
        assert_eq!(stmts("if lb =5 then 6"), ["if lb =5 then", "6"]);
    }
}

//! Tokens of one physical line of Turbo Basic.
//!
//! The lexer only has to be exact about *where* things are. Every rewrite is an
//! edit to the original bytes at a token's span, never a re-printing of tokens,
//! so anything the lexer does not understand simply passes through untouched —
//! which is the right failure: the compiler will then say what it thinks of it.
//!
//! Bytes outside ASCII only matter inside strings and comments, where they are
//! the user's own text in whatever DOS code page they typed it in. They are
//! never decoded here.

/// Statements that take a file number, and that Turbo Basic accepted with the
/// `#` written straight after the keyword: `PRINT#1,`. FreeBASIC reads `PRINT#`
/// as
/// one name — a variable called `PRINT` with the double-precision suffix — and
/// stops with "Syntax error" (see `fbc.bas`, where file `PRINT` is recognised
/// only as element 1 = `PRINT`, element 2 = `#`).
pub const FILE_KEYWORDS: &[&str] = &[
    "PRINT", "INPUT", "WRITE", "GET", "PUT", "CLOSE", "FIELD", "SEEK", "WIDTH", "LOCK", "UNLOCK",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Digits at the start of a line: a line number, not a value.
    LineNumber,
    /// A name, with any type suffix (`%&!#$`) included in the span.
    Ident,
    Number,
    /// A string literal, quotes included. May be unterminated at end of line,
    /// which GW-BASIC-era code relied on.
    Str,
    /// `'` or `REM`, to the end of the line.
    Rem,
    /// `DATA` and its raw items, up to a `:` outside quotes or the end of line.
    Data,
    /// A `$` metastatement at the start of a line, to the end of the line.
    Meta,
    /// `?`, the shorthand for `PRINT`.
    Question,
    Colon,
    Comma,
    Hash,
    LParen,
    RParen,
    /// Any other single byte: operators, `;`, and bytes we have no opinion on.
    Punct,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub kind: Kind,
    pub start: usize,
    pub end: usize,
}

impl Token {
    pub fn text<'a>(&self, line: &'a [u8]) -> &'a [u8] {
        &line[self.start..self.end]
    }

    /// Case-insensitive comparison of an identifier's full text, suffix included.
    pub fn is_word(&self, line: &[u8], word: &str) -> bool {
        self.kind == Kind::Ident && self.text(line).eq_ignore_ascii_case(word.as_bytes())
    }
}

/// One lexed line, plus the places where Turbo Basic's spelling needs a space
/// that FreeBASIC's does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lexed {
    pub tokens: Vec<Token>,
    /// Byte offsets of a `#` written straight after a file keyword.
    pub glued_hash: Vec<usize>,
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic()
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'.' || b == b'_'
}

fn is_suffix(b: u8) -> bool {
    matches!(b, b'%' | b'&' | b'!' | b'#' | b'$')
}

pub fn lex(line: &[u8]) -> Lexed {
    let mut tokens = Vec::new();
    let mut glued_hash = Vec::new();
    let mut i = 0;
    let n = line.len();

    // Whether the next token is the first on the line (ignoring blanks), and
    // whether it is the first after a line number: both decide what digits and
    // `$` mean.
    let mut at_line_start = true;

    while i < n {
        let b = line[i];
        if b == b' ' || b == b'\t' || b == 0x0C {
            i += 1;
            continue;
        }
        let start = i;
        let first = at_line_start;
        at_line_start = false;

        if first && b.is_ascii_digit() {
            while i < n && line[i].is_ascii_digit() {
                i += 1;
            }
            tokens.push(Token {
                kind: Kind::LineNumber,
                start,
                end: i,
            });
            // A metastatement may follow a line number no more than it may
            // follow a statement, but `$` right after it is still a meta for
            // our purposes: treat the next token as line-initial.
            at_line_start = true;
            continue;
        }

        if first && b == b'$' && line.get(i + 1).is_some_and(|c| c.is_ascii_alphabetic()) {
            tokens.push(Token {
                kind: Kind::Meta,
                start,
                end: n,
            });
            break;
        }

        if b == b'\'' {
            tokens.push(Token {
                kind: Kind::Rem,
                start,
                end: n,
            });
            break;
        }

        if b == b'"' {
            i += 1;
            while i < n && line[i] != b'"' {
                i += 1;
            }
            if i < n {
                i += 1;
            }
            tokens.push(Token {
                kind: Kind::Str,
                start,
                end: i,
            });
            continue;
        }

        if is_ident_start(b) {
            while i < n && is_ident_char(line[i]) {
                i += 1;
            }
            let base_end = i;
            let word = &line[start..base_end];

            if word.eq_ignore_ascii_case(b"REM") {
                tokens.push(Token {
                    kind: Kind::Rem,
                    start,
                    end: n,
                });
                break;
            }
            if word.eq_ignore_ascii_case(b"DATA") {
                // Raw to the next colon outside quotes: DATA items are not
                // expressions, and an apostrophe in them is data.
                let mut j = base_end;
                let mut quoted = false;
                while j < n {
                    match line[j] {
                        b'"' => quoted = !quoted,
                        b':' if !quoted => break,
                        _ => {}
                    }
                    j += 1;
                }
                tokens.push(Token {
                    kind: Kind::Data,
                    start,
                    end: j,
                });
                i = j;
                continue;
            }

            if i < n && is_suffix(line[i]) {
                let glued = line[i] == b'#'
                    && FILE_KEYWORDS
                        .iter()
                        .any(|k| word.eq_ignore_ascii_case(k.as_bytes()));
                if glued {
                    tokens.push(Token {
                        kind: Kind::Ident,
                        start,
                        end: base_end,
                    });
                    glued_hash.push(i);
                    tokens.push(Token {
                        kind: Kind::Hash,
                        start: i,
                        end: i + 1,
                    });
                    i += 1;
                    continue;
                }
                i += 1;
            }
            tokens.push(Token {
                kind: Kind::Ident,
                start,
                end: i,
            });
            continue;
        }

        if b.is_ascii_digit() || (b == b'.' && line.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            i = lex_decimal(line, i);
            tokens.push(Token {
                kind: Kind::Number,
                start,
                end: i,
            });
            continue;
        }

        if b == b'&' {
            let radix = line.get(i + 1).map(u8::to_ascii_uppercase);
            let digits_from = match radix {
                Some(b'H') | Some(b'O') | Some(b'B') => Some(i + 2),
                Some(c) if c.is_ascii_digit() => Some(i + 1),
                _ => None,
            };
            if let Some(mut j) = digits_from {
                while j < n && line[j].is_ascii_alphanumeric() {
                    j += 1;
                }
                if j < n && matches!(line[j], b'%' | b'&') {
                    j += 1;
                }
                tokens.push(Token {
                    kind: Kind::Number,
                    start,
                    end: j,
                });
                i = j;
                continue;
            }
        }

        let kind = match b {
            b'?' => Kind::Question,
            b':' => Kind::Colon,
            b',' => Kind::Comma,
            b'#' => Kind::Hash,
            b'(' => Kind::LParen,
            b')' => Kind::RParen,
            _ => Kind::Punct,
        };
        i += 1;
        tokens.push(Token {
            kind,
            start,
            end: i,
        });
    }

    Lexed { tokens, glued_hash }
}

/// `12`, `1.5`, `.5`, `5.`, `1E+10`, `2.5D-3`, each with an optional suffix.
fn lex_decimal(line: &[u8], mut i: usize) -> usize {
    let n = line.len();
    while i < n && line[i].is_ascii_digit() {
        i += 1;
    }
    if i < n && line[i] == b'.' {
        i += 1;
        while i < n && line[i].is_ascii_digit() {
            i += 1;
        }
    }
    if i < n && matches!(line[i], b'E' | b'e' | b'D' | b'd') {
        let mut j = i + 1;
        if j < n && matches!(line[j], b'+' | b'-') {
            j += 1;
        }
        if j < n && line[j].is_ascii_digit() {
            while j < n && line[j].is_ascii_digit() {
                j += 1;
            }
            i = j;
        }
    }
    if i < n && matches!(line[i], b'%' | b'&' | b'!' | b'#') {
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<(Kind, String)> {
        let l = lex(src.as_bytes());
        l.tokens
            .iter()
            .map(|t| {
                (
                    t.kind,
                    String::from_utf8_lossy(t.text(src.as_bytes())).into_owned(),
                )
            })
            .collect()
    }

    #[test]
    fn a_hash_glued_to_a_file_keyword_is_split_off() {
        let src = "print# 4,space$(15);\"x\"";
        let l = lex(src.as_bytes());
        assert_eq!(l.glued_hash, vec![5]);
        let k = kinds(src);
        assert_eq!(k[0], (Kind::Ident, "print".into()));
        assert_eq!(k[1], (Kind::Hash, "#".into()));
        assert_eq!(k[2], (Kind::Number, "4".into()));
    }

    #[test]
    fn a_double_precision_variable_keeps_its_suffix() {
        let l = lex(b"x# = total# + 1");
        assert!(l.glued_hash.is_empty());
        assert_eq!(kinds("x# = total# + 1")[0], (Kind::Ident, "x#".into()));
    }

    #[test]
    fn digits_first_on_a_line_are_a_line_number() {
        let k = kinds("10 if lb =5 then 6");
        assert_eq!(k[0], (Kind::LineNumber, "10".into()));
        assert_eq!(k.last().unwrap(), &(Kind::Number, "6".into()));
    }

    #[test]
    fn numbers_in_every_period_spelling() {
        for (src, want) in [
            ("x=.2", ".2"),
            ("x=0.", "0."),
            ("x=150.", "150."),
            ("x=1.5D+2", "1.5D+2"),
            ("x=2E-3", "2E-3"),
            ("x=&H1F", "&H1F"),
            ("x=&O17", "&O17"),
            ("x=&B101", "&B101"),
            ("x=12&", "12&"),
        ] {
            let k = kinds(src);
            assert_eq!(k[2], (Kind::Number, want.into()), "{src}");
        }
    }

    #[test]
    fn comments_and_data_are_raw() {
        let k = kinds("x = 1 ' it's: \"odd\"");
        assert_eq!(k.last().unwrap().0, Kind::Rem);

        let k = kinds("rem print# 4");
        assert_eq!(k, vec![(Kind::Rem, "rem print# 4".into())]);

        let k = kinds("data a, b's, \"c:d\": print 1");
        assert_eq!(k[0], (Kind::Data, "data a, b's, \"c:d\"".into()));
        assert_eq!(k[1].0, Kind::Colon);
    }

    #[test]
    fn a_metastatement_is_only_one_at_the_start_of_a_line() {
        assert_eq!(kinds("$INCLUDE \"X.BAS\"")[0].0, Kind::Meta);
        assert_eq!(kinds("a$ = b$")[0], (Kind::Ident, "a$".into()));
    }

    #[test]
    fn strings_may_be_unterminated() {
        let k = kinds("print \"no end");
        assert_eq!(k[1], (Kind::Str, "\"no end".into()));
    }

    #[test]
    fn bytes_outside_ascii_do_not_upset_anything() {
        let line = b"print \"\xE2\xEA\xF4\": x\xB0 = 1";
        let l = lex(line);
        assert_eq!(l.tokens.last().unwrap().kind, Kind::Number);
    }
}

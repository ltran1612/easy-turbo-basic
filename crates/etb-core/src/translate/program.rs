//! What the translator knows about the whole program before it edits a line.
//!
//! Most rewrites need only their own statement. A few need to know about the
//! rest of the program:
//!
//! - which `FN` names are defined, so `FNAME$` — a file name, in a program
//!   that never defines `FNAME` — is left alone;
//! - the DEFtype in force at each line, for code that is moved or generated;
//! - for each `DEF FN`, the variables its body uses, since Turbo Basic shares
//!   them with the main program unless declared otherwise;
//! - which `END IF` each `EXIT IF` leaves by;
//! - which line numbers and labels something jumps to.
//!
//! All of it is gathered here, in one pass, before any line is edited.

use super::deftype::{self, DefTypes, Ty};
use super::keywords;
use super::lexer::{self, Kind, Lexed, Token};
use super::stmt::{self, Stmt};
use std::collections::{HashMap, HashSet};

pub struct Line<'a> {
    pub text: &'a [u8],
    pub lexed: Lexed,
    pub stmts: Vec<Stmt>,
    /// DEFtype in force at the start of the line.
    pub types: DefTypes,
}

impl Line<'_> {
    pub fn toks(&self) -> &[Token] {
        &self.lexed.tokens
    }
    pub fn stmt_toks(&self, s: Stmt) -> &[Token] {
        s.tokens(&self.lexed.tokens)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    /// As written, suffix included if it had one.
    pub written: String,
    pub base: String,
    pub ty: Ty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FnKind {
    /// `DEF FNname(params) = expression`: the index, in the line's token
    /// list, of the `=`.
    Single { eq: usize },
    /// `DEF FNname(params)` … `END DEF`.
    Multi {
        /// Line of the END DEF, if there is one.
        end: Option<usize>,
        /// Variables the body uses without declaring them: shared with the
        /// main program, as Turbo Basic has it. As written, arrays with `()`.
        shared: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefFn {
    /// The name as defined, `FN` included, suffix excluded: `FNArea`.
    pub base: String,
    pub ty: Ty,
    pub params: Vec<Param>,
    pub line: usize,
    pub stmt: Stmt,
    /// Token indices of the name: one token (`FNArea`) or two (`FN Area`).
    pub name: (usize, usize),
    pub kind: FnKind,
}

impl DefFn {
    /// The name it has in the compiled program: ours, with an explicit type.
    pub fn fb_name(&self) -> String {
        format!("ETB_{}{}", self.base, self.ty.suffix())
    }
}

/// A `SUB` … `END SUB`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sub {
    pub line: usize,
    pub stmt: Stmt,
    /// `SUB name … STATIC`: every variable in it keeps its value already.
    pub is_static: bool,
    pub end: Option<usize>,
    /// Variables the body uses without declaring them. Turbo Basic keeps
    /// their values between calls (the handbook, chapter 4: "Within procedure
    /// definitions, the default is STATIC"); FreeBASIC does not, unless told.
    pub statics: Vec<String>,
    /// Token indices of the brackets of each array parameter, `a(1)`, whose
    /// number — Turbo Basic's count of dimensions — FreeBASIC does not take.
    pub array_params: Vec<(usize, usize)>,
}

pub struct Program<'a> {
    pub lines: Vec<Line<'a>>,
    pub fns: Vec<DefFn>,
    pub subs: Vec<Sub>,
    /// Named constants, upper-cased, and their values as written.
    pub consts: HashMap<String, String>,
    /// Every line number and label something jumps to, upper-cased.
    pub jump_targets: HashSet<String>,
    /// For each `EXIT IF` statement (line, statement index), the line of the
    /// `END IF` it leaves by.
    pub exit_if: HashMap<(usize, usize), usize>,
    /// For each line, the innermost `DO` or `WHILE` loop open at it — what
    /// Turbo Basic's `EXIT LOOP` leaves. `FOR` is not one of them: `EXIT FOR`
    /// is its own statement in both languages.
    pub innermost_loop: Vec<Option<&'static str>>,
}

impl<'a> Program<'a> {
    pub fn analyse(texts: &[&'a [u8]]) -> Self {
        Self::analyse_with(texts, HashMap::new())
    }

    pub fn analyse_with(texts: &[&'a [u8]], consts: HashMap<String, String>) -> Self {
        let mut lines = Vec::with_capacity(texts.len());
        let mut types = DefTypes::default();
        for &text in texts {
            let lexed = lexer::lex(text);
            let stmts = stmt::split(text, &lexed);
            let line = Line {
                text,
                lexed,
                stmts,
                types,
            };
            for &s in &line.stmts {
                types.apply(text, line.stmt_toks(s));
            }
            lines.push(line);
        }

        let mut p = Program {
            lines,
            fns: Vec::new(),
            subs: Vec::new(),
            consts,
            jump_targets: HashSet::new(),
            exit_if: HashMap::new(),
            innermost_loop: Vec::new(),
        };
        p.find_jump_targets();
        p.find_fns();
        p.find_subs();
        p.find_if_blocks();
        p.find_loops();
        p
    }

    /// Which loop each line is inside, so `EXIT LOOP` can leave the right one.
    ///
    /// Turbo Basic has one `EXIT LOOP` for both kinds; FreeBASIC has `EXIT DO`
    /// and `EXIT WHILE`, and the wrong one either fails to compile or — worse
    /// — leaves the loop outside the one the program meant, which is a wrong
    /// answer with no error.
    fn find_loops(&mut self) {
        let mut stack: Vec<&'static str> = Vec::new();
        let mut per_line = Vec::with_capacity(self.lines.len());
        for l in &self.lines {
            // Recorded before this line's own openers, so the line that opens
            // a loop is not yet inside it, and closed after, so the line that
            // closes one still is.
            let opened_here = stack.last().copied();
            let mut here = opened_here;
            for &st in &l.stmts {
                let toks = l.stmt_toks(st);
                let Some(first) = toks.first() else { continue };
                // `DO WHILE x` opens one loop, not two: it starts with DO.
                if first.is_word(l.text, "DO") {
                    stack.push("DO");
                    here = Some("DO");
                } else if first.is_word(l.text, "WHILE") {
                    stack.push("WHILE");
                    here = Some("WHILE");
                } else if first.is_word(l.text, "LOOP") || first.is_word(l.text, "WEND") {
                    stack.pop();
                }
            }
            per_line.push(here);
        }
        self.innermost_loop = per_line;
    }

    /// The definition an identifier token calls or names, if any, and how
    /// many tokens its name takes.
    pub fn fn_at(&self, line: usize, tok: usize) -> Option<(&DefFn, usize)> {
        let l = &self.lines[line];
        let toks = l.toks();
        let t = toks.get(tok)?;
        if t.kind != Kind::Ident {
            return None;
        }
        let text = t.text(l.text);
        // `FN Area` — the name split in two.
        let (name, len) = if text.eq_ignore_ascii_case(b"FN") {
            let next = toks.get(tok + 1).filter(|n| n.kind == Kind::Ident)?;
            let mut v = b"FN".to_vec();
            v.extend_from_slice(next.text(l.text));
            (v, 2)
        } else if text.len() > 2 && text[..2].eq_ignore_ascii_case(b"FN") {
            (text.to_vec(), 1)
        } else {
            return None;
        };
        let (base, ty, explicit) = deftype::resolve(&name, &l.types, 2);
        let same_base = |f: &&DefFn| f.base.eq_ignore_ascii_case(&base);
        let found = if explicit {
            self.fns.iter().filter(same_base).find(|f| f.ty == ty)
        } else {
            let mut it = self.fns.iter().filter(same_base);
            let first = it.next();
            // With no suffix at the call, a name defined once is that one;
            // defined more than once, the call's own DEFtype decides.
            match it.next() {
                None => first,
                Some(_) => self.fns.iter().filter(same_base).find(|f| f.ty == ty),
            }
        };
        found.map(|f| (f, len))
    }

    fn find_jump_targets(&mut self) {
        const JUMPS: &[&str] = &[
            "GOTO", "GOSUB", "THEN", "ELSE", "RESTORE", "RESUME", "RETURN",
        ];
        for l in &self.lines {
            let toks = l.toks();
            let mut i = 0;
            while i < toks.len() {
                if JUMPS.iter().any(|w| toks[i].is_word(l.text, w)) {
                    // `ON x GOTO 10, 20, 30` and `THEN 35` alike: the targets
                    // run on, comma-separated, until something else.
                    let mut j = i + 1;
                    while let Some(t) = toks.get(j) {
                        match t.kind {
                            Kind::Number => {
                                self.jump_targets
                                    .insert(String::from_utf8_lossy(t.text(l.text)).into_owned());
                            }
                            Kind::Ident if !keywords::is_reserved(t.text(l.text)) => {
                                self.jump_targets.insert(
                                    String::from_utf8_lossy(t.text(l.text)).to_ascii_uppercase(),
                                );
                            }
                            _ => break,
                        }
                        if toks.get(j + 1).is_some_and(|c| c.kind == Kind::Comma) {
                            j += 2;
                        } else {
                            break;
                        }
                    }
                }
                i += 1;
            }
        }
    }

    fn find_fns(&mut self) {
        let mut fns = Vec::new();
        let mut i = 0;
        while i < self.lines.len() {
            let l = &self.lines[i];
            let mut next = i + 1;
            for &s in &l.stmts {
                let Some(f) = parse_def_header(l, s) else {
                    continue;
                };
                let (name, eq, params) = f;
                let (base, ty, _) = deftype::resolve(&name.2, &l.types, 2);
                let kind = match eq {
                    Some(eq) => FnKind::Single { eq },
                    None => {
                        // The body runs to END DEF. Definitions do not nest,
                        // so the first END DEF is the one.
                        let end = (i + 1..self.lines.len()).find(|&j| {
                            let lj = &self.lines[j];
                            lj.stmts.iter().any(|&s| is_end_def(lj, s))
                        });
                        let body_end = end.unwrap_or(self.lines.len());
                        let shared = undeclared(&self.lines[i + 1..body_end], &params, &base, true);
                        if let Some(e) = end {
                            next = e + 1;
                        }
                        FnKind::Multi { end, shared }
                    }
                };
                fns.push(DefFn {
                    base,
                    ty,
                    params,
                    line: i,
                    stmt: s,
                    name: (name.0, name.1),
                    kind,
                });
                // A multi-line header is the only thing on its line that
                // matters here; a single-line one may share its line.
                if next > i + 1 {
                    break;
                }
            }
            i = next;
        }
        self.fns = fns;
    }

    fn find_subs(&mut self) {
        let mut subs = Vec::new();
        for (i, l) in self.lines.iter().enumerate() {
            for &s in &l.stmts {
                let toks = l.stmt_toks(s);
                if toks.len() < 2 || !toks[0].is_word(l.text, "SUB") || toks[1].kind != Kind::Ident
                {
                    continue;
                }
                let all = l.toks();
                let name_base = String::from_utf8_lossy(toks[1].text(l.text)).into_owned();
                let mut params = Vec::new();
                let mut array_params = Vec::new();
                let mut k = s.first + 2;
                if all.get(k).is_some_and(|t| t.kind == Kind::LParen) && k < s.end {
                    let close = (k..s.end).find(|&j| {
                        all[j].kind == Kind::RParen
                            && all[k..=j].iter().filter(|t| t.kind == Kind::LParen).count()
                                == all[k..=j].iter().filter(|t| t.kind == Kind::RParen).count()
                    });
                    let close = close.unwrap_or(s.end);
                    let mut j = k + 1;
                    while j < close {
                        let t = &all[j];
                        if t.kind == Kind::Ident {
                            let (base, ty, _) = deftype::resolve(t.text(l.text), &l.types, 0);
                            params.push(Param {
                                written: String::from_utf8_lossy(t.text(l.text)).into_owned(),
                                base,
                                ty,
                            });
                            if all.get(j + 1).is_some_and(|p| p.kind == Kind::LParen) {
                                if let Some(r) =
                                    (j + 2..close).find(|&r| all[r].kind == Kind::RParen)
                                {
                                    array_params.push((j + 1, r));
                                    j = r;
                                }
                            }
                        }
                        j += 1;
                    }
                    k = close + 1;
                }
                let is_static = all[k.min(s.end)..s.end]
                    .iter()
                    .any(|t| t.is_word(l.text, "STATIC"));
                let end = (i + 1..self.lines.len()).find(|&j| {
                    let lj = &self.lines[j];
                    lj.stmts.iter().any(|&st| {
                        let t = lj.stmt_toks(st);
                        t.len() == 2 && t[0].is_word(lj.text, "END") && t[1].is_word(lj.text, "SUB")
                    })
                });
                let body = &self.lines[i + 1..end.unwrap_or(self.lines.len())];
                let statics = if is_static {
                    Vec::new()
                } else {
                    undeclared(body, &params, &name_base, false)
                };
                subs.push(Sub {
                    line: i,
                    stmt: s,
                    is_static,
                    end,
                    statics,
                    array_params,
                });
            }
        }
        self.subs = subs;
    }

    fn find_if_blocks(&mut self) {
        // Open block IFs, and the EXIT IFs seen inside each so far.
        let mut stack: Vec<Vec<(usize, usize)>> = Vec::new();
        for (li, l) in self.lines.iter().enumerate() {
            let n = l.stmts.len();
            for (si, &s) in l.stmts.iter().enumerate() {
                let toks = l.stmt_toks(s);
                let Some(first) = toks.first() else { continue };
                let is = |w: &str| first.is_word(l.text, w);
                let second_is = |w: &str| toks.get(1).is_some_and(|t| t.is_word(l.text, w));
                if is("IF") && si + 1 == n && toks.last().is_some_and(|t| t.is_word(l.text, "THEN"))
                {
                    // `IF cond THEN` with nothing after it on the line: a block.
                    stack.push(Vec::new());
                } else if is("END") && second_is("IF") {
                    if let Some(exits) = stack.pop() {
                        for e in exits {
                            self.exit_if.insert(e, li);
                        }
                    }
                } else if is("EXIT") && second_is("IF") {
                    if let Some(top) = stack.last_mut() {
                        top.push((li, si));
                    }
                }
            }
        }
    }
}

fn is_end_def(l: &Line, s: Stmt) -> bool {
    let toks = l.stmt_toks(s);
    toks.len() == 2 && toks[0].is_word(l.text, "END") && toks[1].is_word(l.text, "DEF")
}

/// `((first token, one past last token of the name), name bytes)`, the index
/// of a top-level `=` if this is a single-line definition, and the parameters.
type Header = ((usize, usize, Vec<u8>), Option<usize>, Vec<Param>);

/// Parse `DEF FNname[(params)] [= expr]` at the start of a statement.
fn parse_def_header(l: &Line, s: Stmt) -> Option<Header> {
    let all = l.toks();
    let toks = l.stmt_toks(s);
    if toks.len() < 2 || !toks[0].is_word(l.text, "DEF") {
        return None;
    }
    let at = s.first + 1;
    let t = &all[at];
    if t.kind != Kind::Ident {
        return None;
    }
    let text = t.text(l.text);
    let (name, mut i) = if text.eq_ignore_ascii_case(b"FN") {
        let n = all
            .get(at + 1)
            .filter(|n| n.kind == Kind::Ident && at + 1 < s.end)?;
        let mut v = b"FN".to_vec();
        v.extend_from_slice(n.text(l.text));
        ((at, at + 2, v), at + 2)
    } else if text.len() > 2 && text[..2].eq_ignore_ascii_case(b"FN") {
        ((at, at + 1, text.to_vec()), at + 1)
    } else {
        // `DEF SEG`, and anything else that is not a function.
        return None;
    };

    let mut params = Vec::new();
    if all.get(i).is_some_and(|t| t.kind == Kind::LParen) && i < s.end {
        i += 1;
        while i < s.end && all[i].kind != Kind::RParen {
            if all[i].kind == Kind::Ident {
                let w = all[i].text(l.text);
                let (base, ty, _) = deftype::resolve(w, &l.types, 0);
                params.push(Param {
                    written: String::from_utf8_lossy(w).into_owned(),
                    base,
                    ty,
                });
            }
            i += 1;
        }
        i += 1;
    }
    let eq = (i < s.end && all[i].text(l.text) == b"=").then_some(i);
    Some((name, eq, params))
}

/// Variables a body uses that it neither receives nor declares. Arrays only
/// with `arrays`: a function shares the main program's arrays, while an array
/// a procedure never dimensions is left to the compiler's own default.
fn undeclared(body: &[Line], params: &[Param], own_name: &str, arrays: bool) -> Vec<String> {
    // Names the body declares, as (BASE + suffix), whatever the form.
    let mut declared: HashSet<String> = HashSet::new();
    // Uses, in order of first appearance: (key, as written, is an array).
    let mut uses: Vec<(String, String, bool)> = Vec::new();

    for l in body {
        for &s in &l.stmts {
            let toks = l.stmt_toks(s);
            let Some(first) = toks.first() else { continue };
            // Declarations. DIM inside a function makes a local array, as it
            // does in FreeBASIC. The first name of each item is what is declared;
            // anything in its brackets — `DIM a(n)` — is a use.
            if ["LOCAL", "STATIC", "SHARED", "DIM"]
                .iter()
                .any(|w| first.is_word(l.text, w))
            {
                for item in stmt::top_level_commas(&toks[1..]) {
                    let mut rest = item;
                    while rest.first().is_some_and(|t| {
                        ["DYNAMIC", "STATIC", "SHARED"]
                            .iter()
                            .any(|w| t.is_word(l.text, w))
                    }) {
                        rest = &rest[1..];
                    }
                    if let Some(name) = rest.first().filter(|t| t.kind == Kind::Ident) {
                        let (base, ty, _) = deftype::resolve(name.text(l.text), &l.types, 0);
                        declared.insert(format!("{}{}", base.to_ascii_uppercase(), ty.suffix()));
                        collect_uses(l, &rest[1..], &mut uses);
                    }
                }
                continue;
            }
            collect_uses(l, toks, &mut uses);
        }
    }

    let is_param = |base: &str, ty: Ty| {
        params
            .iter()
            .any(|p| p.base.eq_ignore_ascii_case(base) && p.ty == ty)
    };
    let mut seen: HashSet<(String, bool)> = HashSet::new();
    let mut out = Vec::new();
    for (key, written, array) in uses {
        let base = &key[..key.len() - 1];
        let ty = Ty::from_suffix(key.as_bytes()[key.len() - 1]).unwrap_or(Ty::Sng);
        if (array && !arrays)
            || base.eq_ignore_ascii_case(own_name)
            || is_param(base, ty)
            || declared.contains(&key)
        {
            continue;
        }
        if seen.insert((key, array)) {
            out.push(if array {
                format!("{written}()")
            } else {
                written
            });
        }
    }
    out
}

/// The variables a run of tokens uses: every name that is not a keyword, a
/// function, a named constant or a label being jumped to.
fn collect_uses(l: &Line, toks: &[Token], uses: &mut Vec<(String, String, bool)>) {
    for (k, t) in toks.iter().enumerate() {
        if t.kind != Kind::Ident {
            continue;
        }
        let w = t.text(l.text);
        if keywords::is_reserved(w) {
            continue;
        }
        let prev = k.checked_sub(1).map(|p| &toks[p]);
        if prev.is_some_and(|p| p.text(l.text) == b"%")
            || prev.is_some_and(|p| {
                ["GOTO", "GOSUB", "RESTORE", "RESUME", "RETURN", "CALL"]
                    .iter()
                    .any(|j| p.is_word(l.text, j))
            })
        {
            continue;
        }
        if w.eq_ignore_ascii_case(b"FN") || (w.len() > 2 && w[..2].eq_ignore_ascii_case(b"FN")) {
            continue;
        }
        let (base, ty, _) = deftype::resolve(w, &l.types, 0);
        let array = toks.get(k + 1).is_some_and(|n| n.kind == Kind::LParen);
        uses.push((
            format!("{}{}", base.to_ascii_uppercase(), ty.suffix()),
            String::from_utf8_lossy(w).into_owned(),
            array,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prog(src: &str) -> Program<'_> {
        let lines: Vec<&[u8]> = src.lines().map(str::as_bytes).collect();
        Program::analyse(&lines)
    }

    #[test]
    fn single_and_multi_line_definitions_are_found() {
        let p = prog(
            "DEF FNCtoF(degreesC) = (1.8 * degreesC) + 32\n\
             DEF FNFactorial#(x%)\n\
               LOCAL i%, total#\n\
               total# = 1\n\
               FOR i% = x% TO 2 STEP -1: total# = total# * i%: NEXT i%\n\
               FNFactorial# = total#\n\
             END DEF\n",
        );
        assert_eq!(p.fns.len(), 2);
        assert_eq!(p.fns[0].base, "FNCtoF");
        assert_eq!(p.fns[0].ty, Ty::Sng);
        assert!(matches!(p.fns[0].kind, FnKind::Single { .. }));
        assert_eq!(p.fns[0].params[0].base, "degreesC");
        assert_eq!(p.fns[1].fb_name(), "ETB_FNFactorial#");
        match &p.fns[1].kind {
            FnKind::Multi { end, shared } => {
                assert_eq!(*end, Some(6));
                assert!(
                    shared.is_empty(),
                    "everything is local or a parameter: {shared:?}"
                );
            }
            k => panic!("{k:?}"),
        }
    }

    #[test]
    fn a_functions_undeclared_variables_are_shared() {
        // The handbook's own FNAddReceipts: `receipts` is used and never
        // declared, so it is the main program's array.
        let p = prog(
            "DEF FNAddReceipts\n\
               LOCAL x, y, total\n\
               FOR x = 1 TO 12: FOR y = 1 TO 30\n\
                 total = total + receipts(x, y) * rate\n\
               NEXT y: NEXT x\n\
               FNAddReceipts = total\n\
             END DEF\n",
        );
        match &p.fns[0].kind {
            FnKind::Multi { shared, .. } => assert_eq!(shared, &["receipts()", "rate"]),
            k => panic!("{k:?}"),
        }
    }

    #[test]
    fn a_variable_starting_with_fn_is_not_a_function_unless_one_is_defined() {
        let p = prog("FNAME$ = \"DATA.TXT\"\nDEF FNa(x) = x * 2\ny = FNa(3)\n");
        assert!(p.fn_at(0, 0).is_none(), "FNAME$ is a variable");
        let (f, len) = p.fn_at(2, 2).expect("FNa is called on line 3");
        assert_eq!((f.base.as_str(), len), ("FNa", 1));
    }

    #[test]
    fn a_function_type_follows_deftype_at_the_definition() {
        let p = prog("DEFDBL A-Z\nDEF FNArea(w, h) = w * h\n");
        assert_eq!(p.fns[0].ty, Ty::Dbl);
        assert_eq!(p.fns[0].params[0].ty, Ty::Dbl);
    }

    #[test]
    fn a_procedures_undeclared_variables_are_its_statics() {
        let p = prog(
            "SUB Tally(a(1), n) \n\
               SHARED total\n\
               LOCAL i\n\
               FOR i = 0 TO n: count = count + a(i): NEXT i\n\
               total = total + count\n\
               CALL Report(count)\n\
             END SUB\n\
             SUB Keep STATIC\n\
               k = k + 1\n\
             END SUB\n",
        );
        assert_eq!(p.subs.len(), 2);
        assert_eq!(p.subs[0].statics, ["count"], "not a, n, total, i or Report");
        assert_eq!(p.subs[0].end, Some(6));
        assert_eq!(p.subs[0].array_params.len(), 1);
        assert!(p.subs[1].is_static);
        assert!(p.subs[1].statics.is_empty());
    }

    #[test]
    fn def_seg_is_not_a_function() {
        assert!(prog("DEF SEG = &HB800\n").fns.is_empty());
    }

    #[test]
    fn exit_if_leaves_by_the_innermost_block() {
        let p = prog(
            "IF a THEN\n\
               IF b THEN\n\
                 EXIT IF\n\
               END IF\n\
               EXIT IF\n\
             END IF\n",
        );
        assert_eq!(p.exit_if.get(&(2, 0)), Some(&3));
        assert_eq!(p.exit_if.get(&(4, 0)), Some(&5));
    }

    #[test]
    fn single_line_ifs_are_not_blocks() {
        let p = prog("IF a THEN 100\nIF b THEN PRINT\nIF c THEN ' block\nEXIT IF\nEND IF\n");
        assert_eq!(p.exit_if.get(&(3, 0)), Some(&4));
    }

    #[test]
    fn jump_targets_are_collected() {
        let p = prog("IF a THEN 35 ELSE 40\nON n GOTO 10, 20, again\nGOSUB 900\n");
        for t in ["35", "40", "10", "20", "AGAIN", "900"] {
            assert!(p.jump_targets.contains(t), "{t}");
        }
    }
}

//! The rewrites.
//!
//! Each rule looks at one statement and produces edits: byte ranges of the
//! original line and what to put there. Edits never span lines, which is what
//! keeps every staged line on the same line number as the user's. The one
//! thing that cannot stay on its line — a single-line `DEF FN`, since QB64 has
//! no single-line FUNCTION — is removed from it and rebuilt after the user's
//! last line, with the line map saying where it came from.

use super::deftype;
use super::findings::Severity;
use super::keywords;
use super::lexer::{Kind, Token};
use super::program::{FnKind, Program};
use super::stmt::{self, Stmt};
use crate::error::ProblemArg;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

impl Edit {
    fn insert(at: usize, text: impl Into<String>) -> Self {
        Self {
            start: at,
            end: at,
            text: text.into(),
        }
    }
    fn replace(t: &Token, text: impl Into<String>) -> Self {
        Self {
            start: t.start,
            end: t.end,
            text: text.into(),
        }
    }
    fn span(start: usize, end: usize, text: impl Into<String>) -> Self {
        Self {
            start,
            end,
            text: text.into(),
        }
    }
}

/// A finding about one line, before the file name is attached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineFinding {
    pub severity: Severity,
    pub key: &'static str,
    /// 0-based line index.
    pub line: usize,
    pub args: Vec<(&'static str, ProblemArg)>,
}

/// Code rebuilt after the user's last line, and the line it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moved {
    pub line: usize,
    pub text: Vec<String>,
}

pub struct Rewriter<'p, 'a> {
    prog: &'p Program<'a>,
    pub findings: Vec<LineFinding>,
    pub moved: Vec<Moved>,
    /// For each `$INCLUDE` line, the staged name of what it includes.
    pub includes: HashMap<usize, String>,
    /// Window test mode: keyboard waits are answered, not waited on.
    pub test_window: bool,
    /// The label each targeted END IF gets, by line.
    exit_labels: HashMap<usize, Label>,
    /// Text to put at the start of a line, decided while reading an earlier
    /// one. FreeBASIC takes no statement after a `FUNCTION` or `SUB` header,
    /// so the storage declarations Turbo Basic implies go onto the first line
    /// of the body instead — which keeps the line count, and with it the
    /// promise that line N is line N.
    prefixes: HashMap<usize, String>,
    /// Variables a function uses without declaring them. In Turbo Basic they
    /// are the main program's; FreeBASIC has no `SHARED` inside a procedure,
    /// so they are declared `DIM SHARED` at module level instead — in the
    /// runtime support file, which is compiled first, so the program gains no
    /// line. Arrays keep their `()`.
    pub shared: std::collections::BTreeSet<String>,
    /// `DECLARE` lines for everything the program defines. FreeBASIC will not
    /// call a procedure it has not seen declared, and Turbo Basic programs
    /// call functions defined further down. These go into the runtime support
    /// file, which is compiled first, so the program itself gains no line.
    pub declarations: Vec<String>,
}

#[derive(Debug, Clone)]
enum Label {
    /// The line already has a number or a label to jump to.
    Existing(String),
    /// We add one.
    Ours(String),
}

impl Label {
    fn name(&self) -> &str {
        match self {
            Label::Existing(s) | Label::Ours(s) => s,
        }
    }
}

impl<'p, 'a> Rewriter<'p, 'a> {
    pub fn new(prog: &'p Program<'a>) -> Self {
        let mut targets: Vec<usize> = prog.exit_if.values().copied().collect();
        targets.sort_unstable();
        targets.dedup();
        let mut exit_labels = HashMap::new();
        for (n, line) in targets.into_iter().enumerate() {
            let l = &prog.lines[line];
            let toks = l.toks();
            let label = match toks.first() {
                Some(t) if t.kind == Kind::LineNumber => {
                    Label::Existing(String::from_utf8_lossy(t.text(l.text)).into_owned())
                }
                Some(t)
                    if t.kind == Kind::Ident
                        && toks.get(1).is_some_and(|c| c.kind == Kind::Colon)
                        && !keywords::is_reserved(t.text(l.text)) =>
                {
                    Label::Existing(String::from_utf8_lossy(t.text(l.text)).into_owned())
                }
                _ => Label::Ours(format!("ETB_X{}", n + 1)),
            };
            exit_labels.insert(line, label);
        }
        Self {
            prog,
            findings: Vec::new(),
            moved: Vec::new(),
            includes: HashMap::new(),
            test_window: false,
            exit_labels,
            prefixes: HashMap::new(),
            shared: std::collections::BTreeSet::new(),
            declarations: Vec::new(),
        }
    }

    /// Is this line in the body of a procedure or a multi-line function?
    ///
    /// `SHARED` means one thing there and is not a statement at all outside,
    /// where a variable already belongs to the main program.
    fn inside_procedure(&self, line: usize) -> bool {
        let in_sub = self
            .prog
            .subs
            .iter()
            .any(|sub| sub.line < line && sub.end.is_some_and(|e| line < e));
        let in_fn = self.prog.fns.iter().any(|f| match f.kind {
            FnKind::Multi { end, .. } => f.line < line && end.is_some_and(|e| line < e),
            FnKind::Single { .. } => false,
        });
        in_sub || in_fn
    }

    fn find(&mut self, severity: Severity, key: &'static str, line: usize) {
        self.findings.push(LineFinding {
            severity,
            key,
            line,
            args: Vec::new(),
        });
    }

    /// All edits for line `i`.
    pub fn line(&mut self, i: usize) -> Vec<Edit> {
        let prog = self.prog;
        let l = &prog.lines[i];
        let mut edits = Vec::new();

        if let Some(text) = self.prefixes.get(&i) {
            // After a line number, if the line has one: `STATIC t: 100 t = 1`
            // is not a statement, and the line number is something the rest of
            // the program may jump to.
            let toks = l.toks();
            let at = match toks.first() {
                Some(t) if t.kind == Kind::LineNumber => toks.get(1).map_or(t.end, |n| n.start),
                Some(t) => t.start,
                None => 0,
            };
            edits.push(Edit::insert(at, text.clone()));
        }

        // `PRINT#1,` → `PRINT #1,`. Found by the lexer, which is the only place
        // that knows the `#` was glued on rather than a variable's suffix.
        for &at in &l.lexed.glued_hash {
            edits.push(Edit::insert(at, " "));
        }

        if let Some(Label::Ours(name)) = self.exit_labels.get(&i) {
            if let Some(end) = l
                .stmts
                .iter()
                .find(|&&s| is_words(l.text, l.stmt_toks(s), &["END", "IF"]))
            {
                edits.push(Edit::insert(l.toks()[end.first].start, format!("{name}: ")));
            }
        }

        if let Some(meta) = l.toks().iter().find(|t| t.kind == Kind::Meta) {
            self.metastatement(i, meta, &mut edits);
        }

        for (si, &s) in l.stmts.iter().enumerate() {
            self.statement(i, si, s, &mut edits);
        }
        edits
    }

    fn statement(&mut self, i: usize, si: usize, s: Stmt, edits: &mut Vec<Edit>) {
        let prog = self.prog;
        let l = &prog.lines[i];
        let toks = l.stmt_toks(s);
        let Some(first) = toks.first() else { return };
        let word = |w: &str| first.is_word(l.text, w);
        let second = |w: &str| toks.get(1).is_some_and(|t| t.is_word(l.text, w));

        // A single-line DEF FN leaves its line entirely.
        if let Some(f) = prog
            .fns
            .iter()
            .find(|f| f.line == i && f.stmt == s && matches!(f.kind, FnKind::Single { .. }))
        {
            self.move_single_line_def(i, s, f.clone());
            edits.extend(remove_statement(l.toks(), s));
            return;
        }

        // `SHARED a, b` inside a procedure. Turbo Basic's way of saying these
        // belong to the main program (Owner's Handbook, p.92); FreeBASIC has
        // no SHARED statement at all, so they are declared at module level in
        // the runtime support file and the statement goes.
        if word("SHARED") && self.inside_procedure(i) {
            for item in stmt::top_level_commas(&toks[1..]) {
                let Some(name) = item.first() else { continue };
                let is_array = item.get(1).is_some_and(|t| t.kind == Kind::LParen);
                let (base, ty, _) = deftype::resolve(name.text(l.text), &l.types, 0);
                self.shared.insert(format!(
                    "{}{}",
                    typed_name(&base, ty),
                    if is_array { "()" } else { "" }
                ));
            }
            edits.extend(remove_statement(l.toks(), s));
            return;
        }

        // `%name = 12`: its uses are replaced by the value, so the definition
        // has nothing left to do.
        if super::conditional::definition(l.text, toks).is_some() {
            edits.extend(remove_statement(l.toks(), s));
            return;
        }

        // Statements rebuilt whole: their own text is rendered, with the
        // token-level edits inside it applied, into one replacement.
        if word("INCR") || word("DECR") {
            if let Some(text) = self.incr_decr(i, s) {
                edits.push(Edit::span(first.start, toks[toks.len() - 1].end, text));
            }
            return;
        }
        if word("LOCAL") {
            match self.local_to_dim(i, s) {
                Some(text) => edits.push(Edit::span(first.start, toks[toks.len() - 1].end, text)),
                None => edits.extend(remove_statement(l.toks(), s)),
            }
            return;
        }
        if word("EXIT") && second("IF") {
            match prog
                .exit_if
                .get(&(i, si))
                .and_then(|t| self.exit_labels.get(t))
            {
                Some(label) => edits.push(Edit::span(
                    first.start,
                    toks[1].end,
                    format!("GOTO {}", label.name()),
                )),
                None => self.find(Severity::Refuse, "tr.refuse.exit_if_outside_block", i),
            }
            return;
        }

        self.tokens(i, s.first..s.end, false, edits);

        match first.kind {
            Kind::Question => question(l.text, first, edits),
            Kind::Ident if word("OPEN") => open(l.text, toks, edits),
            Kind::Ident if word("CLOSE") => close(l.text, toks, edits),
            Kind::Ident if toks.len() == 1 && (word("END") || word("SYSTEM") || word("STOP")) => {
                // Our chance to finish what the program started: flush and
                // print anything sent to the printer, and decide whether the
                // window waits for a key. `END IF`, `END SUB` and friends have
                // a second token and are not touched.
                edits.push(Edit::replace(first, "ETB_FINISH: END"));
            }
            Kind::Ident if word("END") && second("DEF") => {
                edits.push(Edit::replace(&toks[1], "FUNCTION"));
            }
            Kind::Ident if word("EXIT") && second("DEF") => {
                edits.push(Edit::replace(&toks[1], "FUNCTION"));
            }
            Kind::Ident if word("EXIT") && second("LOOP") => {
                // Turbo Basic has one EXIT LOOP for both kinds of loop;
                // FreeBASIC has EXIT DO and EXIT WHILE. Inside a WHILE, the
                // wrong one leaves the enclosing DO instead — no error, and
                // the loop the program meant to leave keeps running.
                let kind = prog.innermost_loop.get(i).copied().flatten();
                edits.push(Edit::replace(&toks[1], kind.unwrap_or("DO")));
            }
            Kind::Ident if word("DEF") => self.multi_line_def_header(i, s, edits),
            Kind::Ident if word("SUB") => self.sub_header(i, s, edits),
            Kind::Ident if word("DIM") => dim(l.text, toks, edits),
            Kind::Ident if word("DELAY") && toks.len() > 1 => {
                // Turbo Basic waits seconds; FreeBASIC's SLEEP waits
                // milliseconds. The argument is kept whole and bracketed, so
                // `DELAY a + b` waits for the sum and not for `a`.
                let mut inner = Vec::new();
                self.tokens(i, s.first + 1..s.end, false, &mut inner);
                let arg = render(l.text, l.toks(), s.first + 1..s.end, &inner);
                // Inclusive of an edit sitting exactly at the end: the
                // statement is replaced whole, and a `#` appended to its last
                // constant would otherwise reappear after the replacement.
                let end = toks[toks.len() - 1].end;
                edits.retain(|e| e.end <= first.start || e.start > end);
                edits.push(Edit::span(
                    first.start,
                    end,
                    // `, 1`: sleep the whole time. Without it FreeBASIC's
                    // SLEEP returns the moment a key is waiting — and with
                    // input redirected, at once — where Turbo Basic's DELAY
                    // always waited (Owner's Handbook, p.183).
                    format!("SLEEP CLNG(({}) * 1000), 1", arg.trim()),
                ));
            }
            Kind::Ident if word("CALL") && second("INTERRUPT") => {
                self.find(Severity::Refuse, "tr.refuse.call_interrupt", i)
            }
            Kind::Ident if word("CALL") && second("ABSOLUTE") => {
                self.find(Severity::Refuse, "tr.refuse.call_absolute", i)
            }
            Kind::Ident if word("MEMSET") => {
                // Divided up DOS memory. Windows manages a program's memory
                // for it, so there is nothing to do, and nothing lost.
                edits.clear_within(first.start, toks[toks.len() - 1].end);
                edits.extend(remove_statement(l.toks(), s));
                self.find(Severity::Note, "tr.note.memset_ignored", i)
            }
            Kind::Ident if word("REG") => self.find(Severity::Refuse, "tr.refuse.reg", i),
            _ => {}
        }
    }

    /// Edits that apply to single tokens wherever they are: calls to the
    /// program's own functions, and Turbo Basic's own functions that QB64
    /// spells differently or lacks. With `explicit`, names without a suffix
    /// get the one their DEFtype gives them, for code that is being moved.
    fn tokens(
        &mut self,
        i: usize,
        range: std::ops::Range<usize>,
        explicit: bool,
        edits: &mut Vec<Edit>,
    ) {
        let prog = self.prog;
        let l = &prog.lines[i];
        let toks = l.toks();
        // Suffixes first, so that where one lands at the same point as the
        // bracket closing a wrapped argument, it goes inside it: `(x!)`.
        if explicit {
            for k in range.clone() {
                let text = toks[k].text(l.text);
                if is_variable(l.text, toks, k)
                    && deftype::Ty::from_suffix(*text.last().unwrap_or(&b'x')).is_none()
                    && !keywords::is_qb64_only(&String::from_utf8_lossy(text))
                {
                    let ty = l.types.of_letter(text[0]);
                    edits.push(Edit::insert(toks[k].end, ty.suffix().to_string()));
                }
            }
        }
        for k in range.clone() {
            if toks[k].kind == Kind::Number {
                if let Some(e) = double_constant(l.text, &toks[k]) {
                    edits.push(e);
                }
            }
        }
        for k in range.clone() {
            if is_integer_divide(l.text, &toks[k]) {
                if let Some((from, to)) = divisor_term(l.text, toks, k + 1, range.end) {
                    edits.push(Edit::insert(toks[from].start, "ETB_NZ#("));
                    edits.push(Edit::insert(toks[to - 1].end, ")"));
                }
            }
        }
        let mut k = range.start;
        while k < range.end {
            let t = &toks[k];
            // `%name`, a named constant: its value, in brackets so it binds
            // as one term wherever it stands.
            if t.text(l.text) == b"%" {
                if let Some(n) = toks
                    .get(k + 1)
                    .filter(|n| n.kind == Kind::Ident && k + 1 < range.end)
                {
                    let key = String::from_utf8_lossy(n.text(l.text)).to_ascii_uppercase();
                    if let Some(v) = prog.consts.get(&key) {
                        edits.push(Edit::span(t.start, n.end, format!("({v})")));
                        k += 2;
                        continue;
                    }
                }
            }
            if t.kind != Kind::Ident {
                k += 1;
                continue;
            }
            if is_variable(l.text, toks, k) {
                if let Some(name) = collision_name(t.text(l.text), &l.types) {
                    edits.push(Edit::replace(t, name));
                    k += 1;
                    continue;
                }
            }
            if let Some((f, len)) = prog.fn_at(i, k) {
                let last = &toks[k + len - 1];
                let name = f.qb64_name();
                // The name in its own header is renamed, not called: its
                // brackets hold parameters, not arguments.
                let header = f.line == i && f.name.0 == k;
                match toks
                    .get(k + len)
                    .filter(|p| p.kind == Kind::LParen && k + len < range.end && !header)
                {
                    Some(lp) => {
                        // `FNArea (a, b)` → `ETB_FNArea!((a), (b))`: the space
                        // goes, and each argument is passed as an expression,
                        // which gives it Turbo Basic's by-value meaning.
                        edits.push(Edit::span(t.start, lp.start, name));
                    }
                    None => edits.push(Edit::span(t.start, last.end, name)),
                }
                k += len;
                continue;
            }
            let w = t.text(l.text).to_ascii_uppercase();
            let is_call = toks.get(k + 1).is_some_and(|n| n.kind == Kind::LParen);
            match w.as_slice() {
                b"CEIL" if is_call => edits.push(Edit::replace(t, "ETB_CEIL#")),
                // Turbo Basic's TIMER is seconds since midnight; FreeBASIC's
                // counts from something else entirely and is far too large to
                // keep in the single-precision variables these programs use.
                b"TIMER" if !is_call => edits.push(Edit::replace(t, "ETB_TIMER#")),
                b"BIN$" if is_call => edits.push(Edit::replace(t, "ETB_BIN$")),
                // Microsoft Binary Format: how floating-point numbers sat in
                // random files written by interpretive BASIC. FreeBASIC has
                // no conversion for it, and a number read the wrong way is a
                // wrong answer rather than a failure, so it is refused.
                b"MKMS$" | b"MKMD$" | b"CVMS" | b"CVMD" => {
                    self.find(Severity::Refuse, "tr.refuse.mbf", i)
                }
                // Not written out as `LOG(x) / LOG(10)`: that is
                // 2.9999999999999996 for 1000, so `INT(LOG10(1000))` is 2 and
                // a logarithmic axis loses a decade. Turbo Basic said 3.
                b"LOG2" if is_call => edits.push(Edit::replace(t, "ETB_LOG2#")),
                b"LOG10" if is_call => edits.push(Edit::replace(t, "ETB_LOG10#")),
                b"EXP2" | b"EXP10" if is_call => {
                    if let Some(close) = matching_paren(toks, k + 1) {
                        let base = if w == b"EXP2" { "2" } else { "10" };
                        edits.push(Edit::replace(t, format!("({base} ^ ")));
                        edits.push(Edit::insert(toks[close].end, ")"));
                    }
                }
                // `LBOUND(a(2))`, Turbo Basic's way to name a dimension, is
                // `LBOUND(a, 2)` in QB64.
                b"LBOUND" | b"UBOUND" if is_call => {
                    if let [_, arr, inner, n, close, ..] = &toks[k + 1..] {
                        if arr.kind == Kind::Ident
                            && inner.kind == Kind::LParen
                            && n.kind == Kind::Number
                            && close.kind == Kind::RParen
                        {
                            let n = String::from_utf8_lossy(n.text(l.text)).into_owned();
                            edits.push(Edit::span(inner.start, close.end, format!(", {n}")));
                        }
                    }
                }
                // A test run has no keyboard. `INPUT$(n)` from the keyboard,
                // not `INPUT$(n, #f)` from a file, and `INKEY$`, get Enter.
                b"INPUT$" if self.test_window && is_call => {
                    let reads_a_file = matching_paren(toks, k + 1).is_some_and(|close| {
                        toks[k + 2..close].iter().any(|t| t.kind == Kind::Comma)
                    });
                    if !reads_a_file {
                        edits.push(Edit::replace(t, "ETB_TESTKEYS$"));
                    }
                }
                b"INKEY$" if self.test_window => edits.push(Edit::replace(t, "ETB_TESTINKEY$")),
                b"REG" if is_call => self.find(Severity::Refuse, "tr.refuse.reg", i),
                b"ENDMEM" => self.find(Severity::Refuse, "tr.refuse.endmem", i),
                b"ERADR" => self.find(Severity::Refuse, "tr.refuse.eradr", i),
                _ => {}
            }
            k += 1;
        }
    }

    /// A single-line `DEF FNname(p) = expr` becomes a FUNCTION after the
    /// user's last line. Everything in it gets an explicit type: it now sits
    /// where a later DEFtype may be in force, and must mean what it meant here.
    fn move_single_line_def(&mut self, i: usize, s: Stmt, f: super::program::DefFn) {
        let prog = self.prog;
        let l = &prog.lines[i];
        let FnKind::Single { eq } = f.kind else {
            return;
        };
        let params: Vec<String> = f
            .params
            .iter()
            .map(|p| format!("BYVAL {}", typed_name(&p.base, p.ty)))
            .collect();

        let mut edits = Vec::new();
        self.tokens(i, eq + 1..s.end, true, &mut edits);
        let expr = render(l.text, l.toks(), eq + 1..s.end, &edits);

        // Everything the expression uses that is not a parameter belongs to
        // the main program.
        let mut shared: Vec<String> = Vec::new();
        for k in eq + 1..s.end {
            let t = &l.toks()[k];
            if !is_variable(l.text, l.toks(), k) || prog.fn_at(i, k).is_some() {
                continue;
            }
            let (base, ty, _) = deftype::resolve(t.text(l.text), &l.types, 0);
            if f.params
                .iter()
                .any(|p| p.base.eq_ignore_ascii_case(&base) && p.ty == ty)
            {
                continue;
            }
            let array = l.toks().get(k + 1).is_some_and(|n| n.kind == Kind::LParen);
            let item = format!("{}{}", typed_name(&base, ty), if array { "()" } else { "" });
            if !shared.iter().any(|s| s.eq_ignore_ascii_case(&item)) {
                shared.push(item);
            }
        }

        let name = f.qb64_name();
        let signature = if params.is_empty() {
            name.clone()
        } else {
            format!("{name} ({})", params.join(", "))
        };
        // The body is written out rather than put after the header, because
        // FreeBASIC takes no statement on a header line. These lines are ours
        // and sit after the program, so their number is free.
        for n in &shared {
            self.shared.insert(n.clone());
        }
        let mut text = vec![format!("FUNCTION {signature}")];
        text.push(format!("{name} = {}", expr.trim()));
        text.push("END FUNCTION".into());
        self.declarations
            .push(format!("DECLARE FUNCTION {signature}"));
        self.moved.push(Moved { line: i, text });
    }

    /// `DEF FNname(p)` opening a multi-line definition → `FUNCTION`, with the
    /// variables the body shares with the main program declared SHARED.
    fn multi_line_def_header(&mut self, i: usize, s: Stmt, edits: &mut Vec<Edit>) {
        let prog = self.prog;
        let l = &prog.lines[i];
        let Some(f) = prog
            .fns
            .iter()
            .find(|f| f.line == i && f.stmt == s && matches!(f.kind, FnKind::Multi { .. }))
        else {
            return;
        };
        let toks = l.toks();
        edits.push(Edit::replace(&toks[s.first], "FUNCTION"));
        // Turbo Basic's DEF FN takes its arguments by value; FreeBASIC, like
        // QuickBASIC, takes them by reference unless told. Without this a
        // function that changes its parameter changes the caller's variable —
        // a wrong answer, quietly.
        byval_params(toks, s, edits);
        // The name was renamed by the token pass; nothing else to do there.
        if let FnKind::Multi { shared, end } = &f.kind {
            let _ = end;
            for n in shared {
                let name = list_name(n, &l.types);
                self.shared.insert(name);
            }
            if end.is_none() {
                self.find(Severity::Refuse, "tr.refuse.def_without_end", i);
            }
        }
        self.declarations.push(format!(
            "DECLARE FUNCTION {} ({})",
            f.qb64_name(),
            f.params
                .iter()
                .map(|p| format!("BYVAL {}", typed_name(&p.base, p.ty)))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        // A line number on the header: QB64 does not allow one before
        // FUNCTION. Nothing may jump to it — the handbook forbids jumping into
        // a definition — so it goes, unless something does jump to it.
        if let Some(n) = toks.first().filter(|t| t.kind == Kind::LineNumber) {
            let label = String::from_utf8_lossy(n.text(l.text)).into_owned();
            if prog.jump_targets.contains(&label) {
                self.find(Severity::Refuse, "tr.refuse.jump_into_def", i);
            } else {
                edits.push(Edit::span(n.start, toks[s.first].start, ""));
            }
        }
    }

    /// `SUB name(a(1), n)`: array parameters lose their dimension count, and
    /// the variables the body uses without declaring them are declared
    /// STATIC, because in Turbo Basic they keep their values between calls.
    fn sub_header(&mut self, i: usize, s: Stmt, edits: &mut Vec<Edit>) {
        let prog = self.prog;
        let l = &prog.lines[i];
        let Some(sub) = prog.subs.iter().find(|x| x.line == i && x.stmt == s) else {
            return;
        };
        let toks = l.toks();
        if l.stmt_toks(s)
            .last()
            .is_some_and(|t| t.is_word(l.text, "INLINE"))
        {
            self.find(Severity::Refuse, "tr.refuse.inline", i);
            return;
        }
        for &(open, close) in &sub.array_params {
            edits.push(Edit::span(toks[open].start, toks[close].end, "()"));
        }
        if !sub.statics.is_empty() && sub.end.is_some_and(|e| e > i + 1) {
            let names: Vec<String> = sub.statics.iter().map(|n| list_name(n, &l.types)).collect();
            self.prefixes
                .insert(i + 1, format!("STATIC {}: ", names.join(", ")));
        }
        // The declaration FreeBASIC needs before the first CALL. Built from
        // the header, with every parameter's type spelt out: the declaration
        // is compiled ahead of the program (it goes in the runtime support
        // file), so a `DEFINT A-Z` further down has not been seen yet and an
        // unsuffixed parameter would be declared SINGLE here and INTEGER
        // there. A trailing STATIC belongs on the definition, not on this.
        let first_tok = if toks.first().is_some_and(|t| t.kind == Kind::LineNumber) {
            s.first + 1
        } else {
            s.first
        };
        let name_tok = first_tok + 1;
        let mut decl = format!(
            "DECLARE SUB {}",
            String::from_utf8_lossy(l.toks()[name_tok].text(l.text))
        );
        if let Some(open) = (name_tok..s.end).find(|&k| l.toks()[k].kind == Kind::LParen) {
            if let Some(close) = matching_paren(l.toks(), open) {
                let mut params: Vec<String> = Vec::new();
                for item in stmt::top_level_commas(&l.toks()[open + 1..close]) {
                    let Some(name) = item.first() else { continue };
                    let is_array = item.get(1).is_some_and(|t| t.kind == Kind::LParen);
                    let (base, ty, _) = deftype::resolve(name.text(l.text), &l.types, 0);
                    params.push(format!(
                        "{}{}",
                        typed_name(&base, ty),
                        if is_array { "()" } else { "" }
                    ));
                }
                decl.push_str(&format!(" ({})", params.join(", ")));
            }
        }
        self.declarations.push(decl);

        if sub.end.is_none() {
            self.find(Severity::Refuse, "tr.refuse.sub_without_end", i);
        }
        if let Some(n) = toks.first().filter(|t| t.kind == Kind::LineNumber) {
            let label = String::from_utf8_lossy(n.text(l.text)).into_owned();
            if prog.jump_targets.contains(&label) {
                self.find(Severity::Refuse, "tr.refuse.jump_into_sub", i);
            } else {
                edits.push(Edit::span(n.start, toks[s.first].start, ""));
            }
        }
    }

    /// `INCR x` → `x = x + 1`; `DECR x, n` → `x = x - (n)`.
    fn incr_decr(&mut self, i: usize, s: Stmt) -> Option<String> {
        let prog = self.prog;
        let l = &prog.lines[i];
        let toks = l.toks();
        let op = if toks[s.first].is_word(l.text, "INCR") {
            "+"
        } else {
            "-"
        };
        let body = s.first + 1..s.end;
        let parts = stmt::top_level_commas(&toks[body.clone()]);
        let var = parts.first().filter(|p| !p.is_empty())?;
        let var_range = index_range(toks, var);
        let mut edits = Vec::new();
        self.tokens(i, body, false, &mut edits);
        let v = render(l.text, toks, var_range, &edits);
        let amount = match parts.get(1).filter(|p| !p.is_empty()) {
            Some(a) => format!("({})", render(l.text, toks, index_range(toks, a), &edits)),
            None => "1".into(),
        };
        Some(format!("{v} = {v} {op} {amount}"))
    }

    /// `LOCAL a, b%, arr()` → `DIM a, b%`. A local array is dimensioned by a
    /// DIM of its own later, which in a QB64 procedure is local already.
    fn local_to_dim(&mut self, i: usize, s: Stmt) -> Option<String> {
        let prog = self.prog;
        let l = &prog.lines[i];
        let toks = l.toks();
        let scalars: Vec<String> = stmt::top_level_commas(&toks[s.first + 1..s.end])
            .into_iter()
            .filter(|p| p.len() == 1 && p[0].kind == Kind::Ident)
            .map(|p| String::from_utf8_lossy(p[0].text(l.text)).into_owned())
            .collect();
        (!scalars.is_empty()).then(|| format!("DIM {}", scalars.join(", ")))
    }

    fn metastatement(&mut self, i: usize, meta: &Token, edits: &mut Vec<Edit>) {
        let l = &self.prog.lines[i];
        let text = meta.text(l.text);
        let word: Vec<u8> = text
            .iter()
            .skip(1)
            .take_while(|b| b.is_ascii_alphabetic())
            .map(u8::to_ascii_uppercase)
            .collect();
        match word.as_slice() {
            // FreeBASIC has these, spelt the same way, as a comment that it
            // reads: `'$dynamic`. So they are kept rather than dropped, and
            // arrays go on behaving as the program asked.
            b"DYNAMIC" | b"STATIC" => edits.push(Edit::insert(meta.start, "'")),
            // Memory, stack and device settings for DOS. Nothing to do today.
            //
            // `REM .` rather than `'`: FreeBASIC reads a `$word` at the start
            // of a comment as one of its own directives, so a bare comment
            // marker would leave `$SOUND` to be parsed and rejected.
            b"STACK" | b"SEGMENT" | b"SOUND" | b"COM" | b"EVENT" | b"OPTION" => {
                edits.push(Edit::insert(meta.start, "REM ."));
                self.findings.push(LineFinding {
                    severity: Severity::Note,
                    key: "tr.note.meta_ignored",
                    line: i,
                    args: vec![(
                        "meta",
                        ProblemArg::text(String::from_utf8_lossy(text).trim().to_string()),
                    )],
                });
            }
            b"INLINE" => self.find(Severity::Refuse, "tr.refuse.inline", i),
            b"INCLUDE" => match self.includes.get(&i) {
                Some(staged) => edits.push(Edit::replace(meta, format!("'$INCLUDE:'{staged}'"))),
                // Not found, or not a program: already said; kept as a
                // comment that FreeBASIC will not read as a directive.
                None => edits.push(Edit::insert(meta.start, "REM .")),
            },
            _ => {}
        }
    }
}

/// The name a variable gets when its own is a word QB64 reserves: ours, with
/// its type spelt out, because the new name starts with a different letter.
fn collision_name(text: &[u8], types: &deftype::DefTypes) -> Option<String> {
    let (base, ty, _) = deftype::resolve(text, types, 0);
    keywords::is_qb64_only(&base).then(|| format!("ETB_V_{base}{}", ty.suffix()))
}

/// `base` with its type spelt out, renamed if QB64 reserves it.
fn typed_name(base: &str, ty: deftype::Ty) -> String {
    if keywords::is_qb64_only(base) {
        format!("ETB_V_{base}{}", ty.suffix())
    } else {
        format!("{base}{}", ty.suffix())
    }
}

/// A name from an analysis list (`x`, `arr()`), renamed if QB64 reserves it.
fn list_name(written: &str, types: &deftype::DefTypes) -> String {
    let (name, array) = match written.strip_suffix("()") {
        Some(n) => (n, "()"),
        None => (written, ""),
    };
    if let Some(n) = collision_name(name.as_bytes(), types) {
        return format!("{n}{array}");
    }
    // The type spelt out. These names are declared in the runtime support
    // file, which is compiled before the program and has no `DEFINT` of its
    // own: `v` declared there and `v` under the program's `DEFINT A-Z` would
    // otherwise be two different variables, and the array the function reads
    // would be one nobody had dimensioned.
    let (base, ty, _) = deftype::resolve(name.as_bytes(), types, 0);
    format!("{}{array}", typed_name(&base, ty))
}

/// `DIM DYNAMIC a(n)` is QB64's `REDIM a(n)`; `DIM STATIC` is its `DIM`; and
/// Turbo Basic's bounds, `a(1:10)`, are QB64's `a(1 TO 10)`.
fn dim(line: &[u8], toks: &[Token], edits: &mut Vec<Edit>) {
    match toks.get(1) {
        Some(t) if t.is_word(line, "DYNAMIC") => {
            edits.push(Edit::span(toks[0].start, t.end, "REDIM"));
        }
        Some(t) if t.is_word(line, "STATIC") => {
            edits.push(Edit::span(toks[0].end, t.end, ""));
        }
        _ => {}
    }
    let mut depth = 0i32;
    for t in toks {
        match t.kind {
            Kind::LParen => depth += 1,
            Kind::RParen => depth -= 1,
            Kind::Colon if depth > 0 => edits.push(Edit::replace(t, " TO ")),
            _ => {}
        }
    }
}

trait ClearWithin {
    /// Drop edits inside `[a, b)`, for a statement about to be replaced whole.
    fn clear_within(&mut self, a: usize, b: usize);
}

impl ClearWithin for Vec<Edit> {
    fn clear_within(&mut self, a: usize, b: usize) {
        self.retain(|e| e.end <= a || e.start >= b);
    }
}

fn is_words(line: &[u8], toks: &[Token], words: &[&str]) -> bool {
    toks.len() == words.len() && toks.iter().zip(words).all(|(t, w)| t.is_word(line, w))
}

/// Is token `k` a variable (or an array), rather than a keyword, a function of
/// ours, a named constant or a label being jumped to?
fn is_variable(line: &[u8], toks: &[Token], k: usize) -> bool {
    let t = &toks[k];
    if t.kind != Kind::Ident {
        return false;
    }
    let w = t.text(line);
    if keywords::is_reserved(w)
        || w.eq_ignore_ascii_case(b"FN")
        || (w.len() > 2 && w[..2].eq_ignore_ascii_case(b"FN"))
    {
        return false;
    }
    let prev = k.checked_sub(1).map(|p| &toks[p]);
    !(prev.is_some_and(|p| p.text(line) == b"%")
        || prev.is_some_and(|p| {
            ["GOTO", "GOSUB", "RESTORE", "RESUME", "RETURN", "CALL"]
                .iter()
                .any(|j| p.is_word(line, j))
        }))
}

/// The index of the `)` matching the `(` at `open`.
fn matching_paren(toks: &[Token], open: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (k, t) in toks.iter().enumerate().skip(open) {
        match t.kind {
            Kind::LParen => depth += 1,
            Kind::RParen => {
                depth -= 1;
                if depth == 0 {
                    return Some(k);
                }
            }
            _ => {}
        }
    }
    None
}

/// Wrap each argument of the call whose `(` is at `open` in parentheses.
/// Put `BYVAL` before each parameter of a `DEF FN` header.
fn byval_params(toks: &[Token], s: Stmt, edits: &mut Vec<Edit>) {
    let Some(open) = (s.first..s.end).find(|&k| toks[k].kind == Kind::LParen) else {
        return;
    };
    let Some(close) = matching_paren(toks, open) else {
        return;
    };
    for arg in stmt::top_level_commas(&toks[open + 1..close]) {
        if let Some(first) = arg.first() {
            edits.push(Edit::insert(first.start, "BYVAL "));
        }
    }
}

/// The token-index range a sub-slice of `toks` occupies.
fn index_range(toks: &[Token], part: &[Token]) -> std::ops::Range<usize> {
    let start = toks.iter().position(|t| t == &part[0]).unwrap_or(0);
    start..start + part.len()
}

/// The text of tokens `range`, with the edits that fall inside it applied.
/// A constant with a decimal point is double precision in Turbo Basic, and
/// single in QuickBASIC — so `3.14159 * 100.5 ^ 2` is not the same number in
/// the two languages, and neither is anything computed from one.
///
/// Measured by asking each compiler what its own constants are, because the
/// handbook is wrong about this (p.68 says up to six digits is single):
///
/// | `IF c = c#` | Turbo Basic | fbc -lang qb |
/// |---|---|---|
/// | `3.14159` | equal, so **double** | not equal, so single |
/// | `.7` | **double** | single |
/// | `1.5` | **double** | single |
///
/// and the consequences, real compiler against real compiler:
///
/// | | Turbo Basic | without this rule |
/// |---|---|---|
/// | `PRINT 3.14159 * 100.5 ^ 2` | `31730.8443975` | `31730.84559345245` |
/// | `PI# = 3.14159 : PRINT PI#` | `3.14159` | `3.141590118408203` |
///
/// This rule was once removed on the strength of the handbook and of
/// `IF k = .7` taking a branch that looked wrong. Turbo Basic answers
/// `NOTEQUAL` there too: a single variable never did equal a double constant,
/// in 1987 or now, and reproducing that is the job.
///
/// An integer constant is left alone — Turbo Basic stores those as integers
/// too — and so is one that already says what it is. A constant written with
/// an exponent takes `D`, which is how QuickBASIC spells a double exponent;
/// `1.5E+2#` is not valid there.
fn double_constant(line: &[u8], t: &Token) -> Option<Edit> {
    let text = t.text(line);
    if text.starts_with(b"&") {
        return None; // &H1F, &O17, &B101: integers, whatever base
    }
    if text
        .last()
        .is_some_and(|b| matches!(b, b'!' | b'#' | b'%' | b'&'))
    {
        return None; // already spelled out
    }
    if text.iter().any(|b| matches!(b, b'D' | b'd')) {
        return None; // `1.5D+2` is already double
    }
    match text.iter().position(|b| matches!(b, b'E' | b'e')) {
        Some(at) => Some(Edit::span(t.start + at, t.start + at + 1, "D")),
        None if text.contains(&b'.') => Some(Edit::insert(t.end, "#")),
        None => None, // a whole number: an integer constant, as it is in TB
    }
}

/// `\` or `MOD`: the two operators whose divisor the processor divides by.
fn is_integer_divide(line: &[u8], t: &Token) -> bool {
    match t.kind {
        Kind::Punct => t.text(line) == b"\\",
        Kind::Ident => t.is_word(line, "MOD"),
        _ => false,
    }
}

/// How far the divisor of an integer division reaches.
///
/// It is the term after the operator: names, numbers, calls, bracketed
/// groups, and `* / ^` between them, with a leading sign allowed. It ends at
/// anything that binds less tightly — `+ -` between terms, another `\` or
/// `MOD`, a comparison, `AND`, a comma, a colon, `THEN` — or where the
/// statement does.
fn divisor_term(line: &[u8], toks: &[Token], from: usize, end: usize) -> Option<(usize, usize)> {
    const ENDS_IT: &[&str] = &[
        "MOD", "AND", "OR", "XOR", "EQV", "IMP", "THEN", "ELSE", "TO", "STEP", "USING",
    ];
    let mut depth = 0i32;
    let mut j = from;
    while j < end {
        let t = &toks[j];
        match t.kind {
            Kind::LParen => depth += 1,
            Kind::RParen => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            Kind::Colon | Kind::Comma if depth == 0 => break,
            Kind::Ident if depth == 0 => {
                let w = String::from_utf8_lossy(t.text(line)).to_ascii_uppercase();
                if ENDS_IT.contains(&w.as_str()) {
                    break;
                }
            }
            Kind::Punct if depth == 0 => {
                let c = t.text(line);
                let first = j == from;
                let continues =
                    matches!(c, b"*" | b"/" | b"^") || (first && matches!(c, b"+" | b"-"));
                if !continues {
                    break;
                }
            }
            _ => {}
        }
        j += 1;
    }
    if j > from {
        Some((from, j))
    } else {
        None
    }
}

fn render(line: &[u8], toks: &[Token], range: std::ops::Range<usize>, edits: &[Edit]) -> String {
    if range.is_empty() {
        return String::new();
    }
    let (a, b) = (toks[range.start].start, toks[range.end - 1].end);
    let inside: Vec<Edit> = edits
        .iter()
        .filter(|e| e.start >= a && e.end <= b)
        .map(|e| Edit {
            start: e.start - a,
            end: e.end - a,
            text: e.text.clone(),
        })
        .collect();
    String::from_utf8_lossy(&apply(&line[a..b], &inside)).into_owned()
}

/// Edits removing a statement, and the colon beside it, so what is left of
/// the line still parses.
///
/// Two edits and not one widened span, because a line may have two statements
/// that both go — `%TRUE = -1 : %FALSE = 0` is ordinary Turbo Basic, and so is
/// a pair of single-line `DEF FN`s. One span widened to swallow the colon
/// after the first statement and another widened to swallow the same colon
/// before the second overlap, and overlapping edits cannot both be applied:
/// one was dropped, which left half a definition in the program.
///
/// Emitted separately, the colon's edit is the *same* edit whichever statement
/// asks for it, and `apply` takes it once.
fn remove_statement(toks: &[Token], s: Stmt) -> Vec<Edit> {
    let mut out = vec![Edit::span(toks[s.first].start, toks[s.end - 1].end, "")];
    // The colon and the spaces around it: from the end of the token before it
    // to the start of the token after it, which is the same span computed from
    // either side.
    let gap = |colon: usize| {
        let a = if colon > 0 {
            toks[colon - 1].end
        } else {
            toks[colon].start
        };
        let b = toks.get(colon + 1).map_or(toks[colon].end, |t| t.start);
        Edit::span(a, b, "")
    };
    if toks.get(s.end).is_some_and(|t| t.kind == Kind::Colon) {
        out.push(gap(s.end));
    } else if s.first > 0 && toks[s.first - 1].kind == Kind::Colon {
        out.push(gap(s.first - 1));
    }
    out
}

/// `?` → `PRINT`. QB64 accepts `?` only in some positions; the word is
/// accepted in all of them.
fn question(line: &[u8], q: &Token, edits: &mut Vec<Edit>) {
    let glued = line.get(q.end).is_some_and(|&b| b != b' ' && b != b'\t');
    edits.push(Edit::replace(q, if glued { "PRINT " } else { "PRINT" }));
}

fn span_text(line: &[u8], toks: &[Token]) -> Option<String> {
    let (a, b) = (toks.first()?.start, toks.last()?.end);
    Some(String::from_utf8_lossy(&line[a..b]).into_owned())
}

fn strip_hash(toks: &[Token]) -> &[Token] {
    match toks.first() {
        Some(t) if t.kind == Kind::Hash => &toks[1..],
        _ => toks,
    }
}

/// Route the file name of an `OPEN` through `ETB_DEV$`, which turns a printer
/// port into a file at run time.
///
/// At run time, because the name is usually in a variable: `F$ = "LPT1"` a
/// few lines up, or typed by the user. `OPEN "LPT1" ...` cannot work on a
/// machine with no parallel port, and QB64 does not open devices at all.
///
/// Both of Turbo Basic's spellings are handled:
///   `OPEN name FOR mode AS [#]n [LEN = r]` and `OPEN mode, [#]n, name [, r]`.
fn open(line: &[u8], toks: &[Token], edits: &mut Vec<Edit>) {
    let rest = &toks[1..];
    let (name, num) = if let Some(k) = stmt::find_top_level_word(line, rest, &["FOR", "AS"]) {
        let name = &rest[..k];
        let Some(j) = stmt::find_top_level_word(line, &rest[k..], &["AS"]) else {
            return;
        };
        let mut num = strip_hash(&rest[k + j + 1..]);
        if let Some(l) = stmt::find_top_level_word(line, num, &["LEN"]) {
            num = &num[..l];
        }
        (name, num)
    } else {
        let parts = stmt::top_level_commas(rest);
        if parts.len() < 3 {
            return;
        }
        (parts[2], strip_hash(parts[1]))
    };
    let (Some(first), Some(last), Some(num_text)) =
        (name.first(), name.last(), span_text(line, num))
    else {
        return;
    };
    edits.push(Edit::insert(first.start, "ETB_DEV$(("));
    edits.push(Edit::insert(last.end, format!("), ({num_text}))")));
}

/// After a `CLOSE`, tell the runtime which files closed, so a printer file is
/// printed as soon as the program is done with it, as the paper came out then.
fn close(line: &[u8], toks: &[Token], edits: &mut Vec<Edit>) {
    let Some(last) = toks.last() else { return };
    let args = &toks[1..];
    let mut tail = String::new();
    if args.is_empty() {
        tail.push_str(": ETB_CLOSED -1");
    } else {
        for part in stmt::top_level_commas(args) {
            if let Some(n) = span_text(line, strip_hash(part)) {
                tail.push_str(&format!(": ETB_CLOSED ({n})"));
            }
        }
    }
    if !tail.is_empty() {
        edits.push(Edit::insert(last.end, tail));
    }
}

/// Apply non-overlapping edits to one line. Insertions at the same point keep
/// the order they were made in.
pub fn apply(line: &[u8], edits: &[Edit]) -> Vec<u8> {
    let mut order: Vec<usize> = (0..edits.len()).collect();
    order.sort_by_key(|&i| (edits[i].start, edits[i].end, i));
    let mut out = Vec::with_capacity(line.len() + 32);
    let mut at = 0;
    let mut last: Option<(usize, usize, &str)> = None;
    for i in order {
        let e = &edits[i];
        // The same edit asked for twice — two statements on one line both
        // removing the colon between them — is one edit, not a conflict.
        if last == Some((e.start, e.end, e.text.as_str())) {
            continue;
        }
        last = Some((e.start, e.end, e.text.as_str()));
        if e.start < at {
            debug_assert!(false, "overlapping edits at {}", e.start);
            continue;
        }
        out.extend_from_slice(&line[at..e.start]);
        out.extend_from_slice(e.text.as_bytes());
        at = e.end;
    }
    out.extend_from_slice(&line[at..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Translate a whole program's lines, returning them and the moved code.
    fn run(src: &str) -> (Vec<String>, Vec<Moved>, Vec<LineFinding>) {
        run_all(src).0
    }

    /// The lines, and what the rewrite decided belongs outside them: what the
    /// module must declare, and what the compiler must be told exists before
    /// the program calls it.
    #[allow(clippy::type_complexity)]
    fn run_all(
        src: &str,
    ) -> (
        (Vec<String>, Vec<Moved>, Vec<LineFinding>),
        std::collections::BTreeSet<String>,
        Vec<String>,
    ) {
        let lines: Vec<&[u8]> = src.split('\n').map(str::as_bytes).collect();
        let prog = Program::analyse(&lines);
        let mut r = Rewriter::new(&prog);
        let out = (0..lines.len())
            .map(|i| {
                let e = r.line(i);
                String::from_utf8(apply(lines[i], &e)).unwrap()
            })
            .collect();
        ((out, r.moved, r.findings), r.shared, r.declarations)
    }

    fn rewrite(src: &str) -> String {
        run(src).0.remove(0)
    }

    #[test]
    fn a_hash_glued_to_print_gets_its_space() {
        assert_eq!(
            rewrite("print# 4,space$(15);\"x\""),
            "print # 4,space$(15);\"x\""
        );
        assert_eq!(
            rewrite("print# 4, using \"##.#\";e1;"),
            "print # 4, using \"##.#\";e1;"
        );
        assert_eq!(rewrite("input#1, a"), "input #1, a");
    }

    #[test]
    fn question_marks_become_print_wherever_a_statement_starts() {
        assert_eq!(rewrite("?"), "PRINT");
        assert_eq!(rewrite("? \"x\""), "PRINT \"x\"");
        assert_eq!(rewrite("16 ?"), "16 PRINT");
        assert_eq!(
            rewrite("if a then ?\"y\" else ?b"),
            "if a then PRINT \"y\" else PRINT b"
        );
        assert_eq!(rewrite("?#4, x"), "PRINT #4, x");
    }

    #[test]
    fn a_question_mark_inside_a_string_is_text() {
        assert_eq!(rewrite("print \"(m) ?\""), "print \"(m) ?\"");
    }

    #[test]
    fn every_open_goes_through_the_device_check() {
        assert_eq!(
            rewrite("open lsl2$ for output as 3"),
            "open ETB_DEV$((lsl2$), (3)) for output as 3"
        );
        assert_eq!(
            rewrite("OPEN \"DATA.TXT\" FOR INPUT AS #1"),
            "OPEN ETB_DEV$((\"DATA.TXT\"), (1)) FOR INPUT AS #1"
        );
        assert_eq!(
            rewrite("OPEN F$ AS #2 LEN = 64"),
            "OPEN ETB_DEV$((F$), (2)) AS #2 LEN = 64"
        );
        assert_eq!(
            rewrite("OPEN \"O\", #n, LEFT$(f$, 8) + \".OUT\""),
            "OPEN \"O\", #n, ETB_DEV$((LEFT$(f$, 8) + \".OUT\"), (n))"
        );
    }

    #[test]
    fn close_reports_what_it_closed() {
        assert_eq!(rewrite("close"), "close: ETB_CLOSED -1");
        assert_eq!(rewrite("CLOSE #3"), "CLOSE #3: ETB_CLOSED (3)");
        assert_eq!(
            rewrite("close #1, #2 ' done"),
            "close #1, #2: ETB_CLOSED (1): ETB_CLOSED (2) ' done"
        );
        assert_eq!(rewrite("close#4"), "close #4: ETB_CLOSED (4)");
    }

    #[test]
    fn end_system_and_stop_finish_properly_but_block_ends_do_not() {
        assert_eq!(rewrite("end"), "ETB_FINISH: END");
        assert_eq!(rewrite("35 SYSTEM"), "35 ETB_FINISH: END");
        assert_eq!(
            rewrite("if ix$ =\"k\" then end"),
            "if ix$ =\"k\" then ETB_FINISH: END"
        );
        assert_eq!(rewrite("END IF"), "END IF");
        assert_eq!(rewrite("end sub"), "end sub");
    }

    #[test]
    fn a_line_with_nothing_to_change_comes_back_identical() {
        for src in [
            "color 14,1",
            "2 cls",
            "e1=700*n*f*w*m/p1/(p1/m+n*f)*(qb+ee*q)/(qb+q)",
            "if lb =5 then 6",
            "   ",
            "",
            "data 1, 2, three",
            "FNAME$ = \"KQ.TXT\"",
        ] {
            assert_eq!(rewrite(src), src);
        }
    }

    /// Every decimal constant is double precision in Turbo Basic — measured
    /// against the compiler, not read in the handbook, which says otherwise.
    /// An integer division by zero is a processor fault, not an error: the
    /// program is killed where it stands, with no message, with what it had
    /// written still in a buffer. Turbo Basic stopped the program and what it
    /// had written was on disk — so the divisor is looked at first.
    #[test]
    fn the_divisor_of_an_integer_division_is_looked_at_first() {
        assert_eq!(rewrite("PRINT 7 \\ 2"), "PRINT 7 \\ ETB_NZ#(2)");
        assert_eq!(rewrite("x = a MOD b"), "x = a MOD ETB_NZ#(b)");
        // The divisor is the term, no more and no less.
        assert_eq!(rewrite("x = a \\ b + c"), "x = a \\ ETB_NZ#(b) + c");
        assert_eq!(rewrite("x = a MOD b * 2"), "x = a MOD ETB_NZ#(b * 2)");
        assert_eq!(
            rewrite("IF n \\ d = 2 THEN PRINT 1"),
            "IF n \\ ETB_NZ#(d) = 2 THEN PRINT 1"
        );
        assert_eq!(rewrite("x = a \\ -b"), "x = a \\ ETB_NZ#(-b)");
        assert_eq!(rewrite("x = a \\ f(i)"), "x = a \\ ETB_NZ#(f(i))");
        assert_eq!(rewrite("x = (a+b) \\ (c-d)"), "x = (a+b) \\ ETB_NZ#((c-d))");
        // Two of them on one line, each with its own divisor.
        assert_eq!(
            rewrite("x = a \\ b, y = c MOD d"),
            "x = a \\ ETB_NZ#(b), y = c MOD ETB_NZ#(d)"
        );
    }

    #[test]
    fn a_constant_with_a_decimal_point_is_double_as_it_is_in_turbo_basic() {
        assert_eq!(rewrite("x = 2 ^ .5"), "x = 2 ^ .5#");
        assert_eq!(rewrite("x = 3.14159 * 2"), "x = 3.14159# * 2");
        assert_eq!(rewrite("x = 150."), "x = 150.#");
        assert_eq!(rewrite("x = 1.5E+2"), "x = 1.5D+2");
        assert_eq!(rewrite("x = 2E-3"), "x = 2D-3");
        // Left alone: whole numbers are integers in Turbo Basic too, a
        // constant that already says what it is says it, and a number in
        // another base is an integer whatever it looks like.
        assert_eq!(rewrite("x = 10"), "x = 10");
        assert_eq!(rewrite("x = 1.5#"), "x = 1.5#");
        assert_eq!(rewrite("x = 1.5!"), "x = 1.5!");
        assert_eq!(rewrite("x = &H1F"), "x = &H1F");
        assert_eq!(rewrite("x = 1.5D+2"), "x = 1.5D+2");
        assert_eq!(rewrite("10 GOTO 20"), "10 GOTO 20");
        assert_eq!(rewrite("a$ = \"1.5\""), "a$ = \"1.5\"");
    }

    // ------------------------------------------------------------ DEF FN

    #[test]
    fn a_single_line_function_moves_after_the_program_with_its_types_spelt_out() {
        let (out, moved, findings) =
            run("DEFDBL K\nDEF FNa(x) = x * k + FNb(x)\nDEF FNb(y) = y + 1\nz = FNa (2)");
        assert_eq!(out[1], "", "the definition leaves its line");
        assert_eq!(out[3], "z = ETB_FNa!(2)");
        assert_eq!(
            moved[0].text,
            [
                "FUNCTION ETB_FNa! (BYVAL x!)",
                "ETB_FNa! = x! * k# + ETB_FNb!(x!)",
                "END FUNCTION"
            ]
        );
        assert_eq!(moved[0].line, 1);
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn a_single_line_function_sharing_its_line_leaves_the_rest_in_place() {
        let (out, moved, _) = run("10 DEF FNsq(v) = v * v: PRINT FNsq(3)");
        assert_eq!(out[0], "10 PRINT ETB_FNsq!(3)");
        assert_eq!(moved.len(), 1);
    }

    #[test]
    fn a_multi_line_function_becomes_a_function_in_place() {
        let ((out, moved, findings), shared, decls) = run_all(
            "DEF FNFactorial#(x%)\n\
             LOCAL i%, total#, scratch()\n\
             IF x% < 0 THEN FNFactorial# = -1: EXIT DEF\n\
             total# = 1\n\
             FOR i% = x% TO 2 STEP -1: total# = total# * i% * scale: NEXT i%\n\
             FNFactorial# = total#\n\
             END DEF\n\
             PRINT FNFactorial#(5)",
        );
        assert!(moved.is_empty());
        assert!(findings.is_empty(), "{findings:?}");
        assert_eq!(out[0], "FUNCTION ETB_FNFactorial#(BYVAL x%)");
        assert_eq!(out[1], "DIM i%, total#");
        assert_eq!(
            out[2],
            "IF x% < 0 THEN ETB_FNFactorial# = -1: EXIT FUNCTION"
        );
        assert_eq!(out[5], "ETB_FNFactorial# = total#");
        assert_eq!(out[6], "END FUNCTION");
        assert_eq!(out[7], "PRINT ETB_FNFactorial#(5)");
        // What the function uses without declaring it belongs to the main
        // program, as Turbo Basic has it. FreeBASIC has no SHARED inside a
        // procedure, so it is the module's, declared where the program cannot
        // see it happen.
        assert_eq!(
            shared.iter().cloned().collect::<Vec<_>>(),
            ["scale!"],
            "with its type spelt out, because it is declared where no DEFtype has been seen"
        );
        assert_eq!(decls, ["DECLARE FUNCTION ETB_FNFactorial# (BYVAL x%)"]);
    }

    #[test]
    fn a_line_number_on_a_function_header_goes_unless_something_jumps_to_it() {
        let (out, _, findings) = run("100 DEF FNd(a)\n110 FNd = a\n120 END DEF");
        assert_eq!(out[0], "FUNCTION ETB_FNd!(BYVAL a)");
        assert!(findings.is_empty());

        let (_, _, findings) = run("GOTO 100\n100 DEF FNd(a)\n110 FNd = a\n120 END DEF");
        assert_eq!(findings[0].key, "tr.refuse.jump_into_def");
    }

    #[test]
    fn a_function_with_no_end_def_is_refused() {
        let (_, _, findings) = run("DEF FNd(a)\nFNd = a\n");
        assert_eq!(findings[0].key, "tr.refuse.def_without_end");
    }

    // ---------------------------------------------------------- the rest

    #[test]
    fn incr_and_decr_become_assignments() {
        assert_eq!(rewrite("INCR n"), "n = n + 1");
        assert_eq!(rewrite("decr a(i), 2 * k"), "a(i) = a(i) - (2 * k)");
        assert_eq!(
            rewrite("IF x THEN INCR c ELSE DECR c"),
            "IF x THEN c = c + 1 ELSE c = c - 1"
        );
    }

    #[test]
    fn exit_forms_that_qb64_spells_differently() {
        assert_eq!(rewrite("EXIT LOOP"), "EXIT DO");
        assert_eq!(rewrite("EXIT FOR"), "EXIT FOR");
        assert_eq!(rewrite("EXIT SELECT"), "EXIT SELECT");
        let (out, _, _) = run("IF a THEN\nIF b THEN EXIT IF\nPRINT 1\nEND IF");
        assert_eq!(out[1], "IF b THEN GOTO ETB_X1");
        assert_eq!(out[3], "ETB_X1: END IF");
        let (out, _, _) = run("IF a THEN\nEXIT IF\n200 END IF");
        assert_eq!(out[1], "GOTO 200");
        assert_eq!(out[2], "200 END IF");
    }

    #[test]
    fn turbo_basic_functions_qb64_spells_differently() {
        assert_eq!(rewrite("x = CEIL(y)"), "x = ETB_CEIL#(y)");
        assert_eq!(rewrite("b$ = BIN$(n)"), "b$ = ETB_BIN$(n)");
        assert_eq!(rewrite("p = LOG10(x * 2)"), "p = ETB_LOG10#(x * 2)");
        assert_eq!(rewrite("p = LOG2(n)"), "p = ETB_LOG2#(n)");
        assert_eq!(
            rewrite("p = EXP10(3) + EXP2(n)"),
            "p = (10 ^ (3)) + (2 ^ (n))"
        );
        // Seconds there, milliseconds here.
        assert_eq!(rewrite("DELAY 1.5"), "SLEEP CLNG((1.5#) * 1000), 1");
        assert_eq!(rewrite("DELAY a + b"), "SLEEP CLNG((a + b) * 1000), 1");
    }

    #[test]
    fn microsoft_format_numbers_in_a_random_file_are_refused() {
        // Reading one the wrong way is a wrong answer, not a failure, and a
        // wrong answer is the one thing this must never produce quietly.
        let (_, _, findings) = run("a$ = MKMS$(v)");
        assert_eq!(findings[0].key, "tr.refuse.mbf");
        let (_, _, findings) = run("v = CVMD(a$)");
        assert_eq!(findings[0].key, "tr.refuse.mbf");
    }

    #[test]
    fn what_needs_the_hardware_of_the_time_is_refused() {
        for (src, key) in [
            ("CALL INTERRUPT &H21", "tr.refuse.call_interrupt"),
            ("CALL ABSOLUTE(x%)", "tr.refuse.call_absolute"),
            ("REG 1, &H0900", "tr.refuse.reg"),
            ("x = REG(1)", "tr.refuse.reg"),
            ("m = ENDMEM", "tr.refuse.endmem"),
            ("SUB Beep INLINE\n$INLINE &HCC\nEND SUB", "tr.refuse.inline"),
            ("$INLINE \"X.COM\"", "tr.refuse.inline"),
        ] {
            let (_, _, findings) = run(src);
            assert_eq!(findings.first().map(|f| f.key), Some(key), "{src}");
            assert_eq!(findings[0].severity, Severity::Refuse);
        }
    }

    #[test]
    fn named_constants_become_their_values() {
        let lines: Vec<&[u8]> = vec![
            b"%maxx = 319",
            b"x = %maxx * 2: debug% = 1",
            b"IF %maxx THEN PRINT",
        ];
        let mut consts = std::collections::HashMap::new();
        consts.insert("MAXX".to_string(), "319".to_string());
        let prog = Program::analyse_with(&lines, consts);
        let mut r = Rewriter::new(&prog);
        let out: Vec<String> = (0..3)
            .map(|i| String::from_utf8(apply(lines[i], &r.line(i))).unwrap())
            .collect();
        assert_eq!(
            out,
            ["", "x = (319) * 2: debug% = 1", "IF (319) THEN PRINT"]
        );
    }

    #[test]
    fn a_procedure_keeps_its_variables_between_calls_as_turbo_basic_did() {
        let (out, _, findings) = run(
            "SUB Tally(a(1), n)\nSHARED total\ncount = count + a(n)\ntotal = total + count\nEND SUB",
        );
        assert_eq!(out[0], "SUB Tally(a(), n)");
        // The SHARED statement itself goes: FreeBASIC has no such statement,
        // and `total` is declared at module level instead.
        assert_eq!(out[1].trim_end(), "STATIC count!:");
        assert!(findings.is_empty(), "{findings:?}");
        let (out, _, _) = run("SUB Keep STATIC\nk = k + 1\nEND SUB");
        assert_eq!(out[0], "SUB Keep STATIC", "already static throughout");
    }

    #[test]
    fn dim_forms_and_bounds() {
        assert_eq!(rewrite("DIM a(1:10, 0:5)"), "DIM a(1 TO 10, 0 TO 5)");
        assert_eq!(rewrite("DIM DYNAMIC b(n)"), "REDIM b(n)");
        assert_eq!(rewrite("DIM STATIC c(4)"), "DIM c(4)");
        assert_eq!(
            rewrite("x = LBOUND(a(2)) + UBOUND(a(1))"),
            "x = LBOUND(a, 2) + UBOUND(a, 1)"
        );
        assert_eq!(rewrite("x = UBOUND(a)"), "x = UBOUND(a)");
    }

    #[test]
    fn a_variable_named_after_a_qb64_word_is_renamed_everywhere() {
        let (out, moved, _) = run(
            "DEFINT T\ntype = 3: long# = 1.5\nDEF FNk(v) = v * type\nPRINT type, string$(3, \"*\")",
        );
        assert_eq!(out[1], "ETB_V_type% = 3: ETB_V_long# = 1.5#");
        assert_eq!(out[3], "PRINT ETB_V_type%, string$(3, \"*\")");
        assert_eq!(moved[0].text[0], "FUNCTION ETB_FNk! (BYVAL v!)");
    }

    #[test]
    fn memset_is_left_out_with_a_note() {
        let (out, _, findings) = run("MEMSET &H9000: PRINT 1");
        assert_eq!(out[0], "PRINT 1");
        assert_eq!(findings[0].key, "tr.note.memset_ignored");
        assert_eq!(findings[0].severity, Severity::Note);
    }

    #[test]
    fn dos_settings_are_kept_as_comments() {
        let (out, _, findings) = run("$STACK &H7FFF\n$DYNAMIC");
        assert_eq!(out[0], "REM .$STACK &H7FFF");
        assert_eq!(out[1], "'$DYNAMIC");
        assert_eq!(findings[0].key, "tr.note.meta_ignored");
    }
}

//! Turbo Basic's reserved words.
//!
//! Needed to tell a label (`again:`) from a statement that happens to be
//! followed by a colon (`CLS: PRINT`). The list is the statements, functions
//! and operators of Turbo Basic 1.x as the 1987 Owner's Handbook lists them;
//! a word missing from it only matters if someone writes it alone before a
//! colon at the start of a line.

const RESERVED: &[&str] = &[
    "ABS",
    "ABSOLUTE",
    "ACCESS",
    "AND",
    "APPEND",
    "AS",
    "ASC",
    "ATN",
    "BASE",
    "BEEP",
    "BIN$",
    "BINARY",
    "BLOAD",
    "BSAVE",
    "CALL",
    "CASE",
    "CDBL",
    "CEIL",
    "CHAIN",
    "CHDIR",
    "CHR$",
    "CINT",
    "CIRCLE",
    "CLEAR",
    "CLNG",
    "CLOSE",
    "CLS",
    "COLOR",
    "COM",
    "COMMAND$",
    "COMMON",
    "COS",
    "CSNG",
    "CSRLIN",
    "CVD",
    "CVI",
    "CVL",
    "CVMD",
    "CVMS",
    "CVS",
    "DATA",
    "DATE$",
    "DECR",
    "DEF",
    "DEFDBL",
    "DEFINT",
    "DEFLNG",
    "DEFSNG",
    "DEFSTR",
    "DELAY",
    "DIM",
    "DO",
    "DRAW",
    "DYNAMIC",
    "ELSE",
    "ELSEIF",
    "END",
    "ENDMEM",
    "ENVIRON",
    "ENVIRON$",
    "EOF",
    "EQV",
    "ERADR",
    "ERASE",
    "ERDEV",
    "ERDEV$",
    "ERL",
    "ERR",
    "ERROR",
    "EXIT",
    "EXP",
    "EXP10",
    "EXP2",
    "FIELD",
    "FILES",
    "FIX",
    "FN",
    "FOR",
    "FRE",
    "GET",
    "GET$",
    "GOSUB",
    "GOTO",
    "HEX$",
    "IF",
    "IMP",
    "INCR",
    "INKEY$",
    "INLINE",
    "INP",
    "INPUT",
    "INPUT$",
    "INSTAT",
    "INSTR",
    "INT",
    "INTERRUPT",
    "IOCTL",
    "IOCTL$",
    "KEY",
    "KILL",
    "LBOUND",
    "LCASE$",
    "LEFT$",
    "LEN",
    "LET",
    "LINE",
    "LIST",
    "LOC",
    "LOCAL",
    "LOCATE",
    "LOCK",
    "LOF",
    "LOG",
    "LOG10",
    "LOG2",
    "LOOP",
    "LPOS",
    "LPRINT",
    "LSET",
    "LTRIM$",
    "MEMSET",
    "MID$",
    "MKD$",
    "MKDIR",
    "MKI$",
    "MKL$",
    "MKMD$",
    "MKMS$",
    "MKS$",
    "MOD",
    "MTIMER",
    "NAME",
    "NEXT",
    "NOT",
    "OCT$",
    "OFF",
    "ON",
    "OPEN",
    "OPTION",
    "OR",
    "OUT",
    "OUTPUT",
    "PAINT",
    "PALETTE",
    "PEEK",
    "PEN",
    "PLAY",
    "PMAP",
    "POINT",
    "POKE",
    "POS",
    "PRESET",
    "PRINT",
    "PSET",
    "PUT",
    "PUT$",
    "RANDOM",
    "RANDOMIZE",
    "READ",
    "REDIM",
    "REG",
    "REM",
    "RESET",
    "RESTORE",
    "RESUME",
    "RETURN",
    "RIGHT$",
    "RMDIR",
    "RND",
    "RSET",
    "RTRIM$",
    "RUN",
    "SAVE",
    "SCREEN",
    "SEEK",
    "SEG",
    "SELECT",
    "SGN",
    "SHARED",
    "SHELL",
    "SIN",
    "SOUND",
    "SPACE$",
    "SPC",
    "SQR",
    "STATIC",
    "STEP",
    "STICK",
    "STOP",
    "STR$",
    "STRIG",
    "STRING$",
    "SUB",
    "SWAP",
    "SYSTEM",
    "TAB",
    "TAN",
    "THEN",
    "TIME$",
    "TIMER",
    "TO",
    "TROFF",
    "TRON",
    "UBOUND",
    "UCASE$",
    "UNTIL",
    "USING",
    "VAL",
    "VARPTR",
    "VARPTR$",
    "VARSEG",
    "VIEW",
    "WAIT",
    "WEND",
    "WHILE",
    "WIDTH",
    "WINDOW",
    "WRITE",
    "XOR",
];

/// Words QB64 reserves that Turbo Basic does not, so a Turbo Basic program may
/// use them as names: `type$ = "A"`, `long = 12.5`. QB64 would stop at each.
/// (`STRING$` is Turbo Basic's function; a variable `string` is not.)
const QB64_ONLY: &[&str] = &[
    "ALIAS",
    "ANY",
    "BYVAL",
    "CALLS",
    "CDECL",
    "CONST",
    "CVDMBF",
    "CVSMBF",
    "DECLARE",
    "DOUBLE",
    "FREEFILE",
    "FUNCTION",
    "INTEGER",
    "INTERRUPTX",
    "IS",
    "LIBRARY",
    "LONG",
    "PCOPY",
    "SADD",
    "SETMEM",
    "SIGNAL",
    "SINGLE",
    "SLEEP",
    "STRING",
    "TYPE",
    "UEVENT",
    "UNLOCK",
];

/// Is this name, without its suffix, a word QB64 reserves and Turbo Basic
/// does not?
pub fn is_qb64_only(base: &str) -> bool {
    QB64_ONLY.iter().any(|w| base.eq_ignore_ascii_case(w))
}

pub fn is_reserved(word: &[u8]) -> bool {
    RESERVED
        .iter()
        .any(|r| word.eq_ignore_ascii_case(r.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_is_upper_case_and_sorted_so_it_can_be_audited() {
        for w in RESERVED {
            assert_eq!(*w, w.to_ascii_uppercase(), "{w}");
        }
        let mut sorted = RESERVED.to_vec();
        sorted.sort_unstable();
        assert_eq!(sorted, RESERVED, "keep the list sorted");
    }

    #[test]
    fn qb64_only_words_are_not_turbo_basic_words() {
        for w in QB64_ONLY {
            assert!(
                !is_reserved(w.as_bytes()),
                "{w} is reserved in Turbo Basic too"
            );
        }
        let mut sorted = QB64_ONLY.to_vec();
        sorted.sort_unstable();
        assert_eq!(sorted, QB64_ONLY, "keep the list sorted");
    }

    #[test]
    fn lookups_ignore_case() {
        assert!(is_reserved(b"cls"));
        assert!(is_reserved(b"Print"));
        assert!(!is_reserved(b"again"));
    }
}

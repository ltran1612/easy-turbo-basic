# Architecture

For someone about to change this code. `README.md` says what the product is;
this says how it is put together and why it is put together that way.

## The rule everything else follows

**The application never modifies the user's source files.** Not a policy — a
structure. One module writes to the filesystem, every write asserts where it
lands, CI fails the build if another module tries, and the test suite hashes the
sources before and after every build and asserts they are byte-identical.

It is also why there is no editor in the UI, and why almost every design
question below resolves the same way: copy, never touch. The translation from
Turbo Basic is itself a copy — the translator returns bytes; it has no way to
write.

## Crates

| | What it is |
|---|---|
| `etb-core` | All logic, including the translator. No GUI dependency, so the whole pipeline is testable headlessly. |
| `etb-gui` | The window (`eframe`/`egui`). Holds no logic, and **may not touch the filesystem or spawn processes** — CI enforces both. |
| `etb-cli` | Headless driver: `doctor`, `build`, `translate`. |
| `etb-testkit` | A fake FreeBASIC and a fake program, the corpus harness, and the corpus. |
| `xtask` | Repository chores: hygiene, fetching FreeBASIC, packaging, icons. |

`etb-core` is the only crate worth learning first, and `translate/` the part of
it most worth learning.

## How a build flows

`etb-gui` never builds anything itself. It sends a `Program` to `etb-core` on a
worker thread and receives `BuildEvent`s back through a channel.

```
App::start_build          etb-gui/src/app.rs      spawns a thread, keeps drawing
  build::build            build/mod.rs            the state machine
    Preflight             ─ every listed file still exists
    Preparing             toolchain/mod.rs        only when the compiler's own
                                                  path cannot be given to it
    Translating           build/stage.rs          read each file through FsGuard
                          build/sourcefmt.rs      refuse what is not BASIC text
                          translate/              Turbo Basic → FreeBASIC, bytes to bytes
                          ─ write the translation into the work tree
    Compiling             build/fbargs.rs         fbc -lang qb -include … src/prog.bas -x out/program
                          build/diagnostics.rs    parse, then map back to the user's lines
  BuildOutcome            ─ exe path, findings, diagnostics, or a FileProblem
App::save_program         fs_guard.rs             copy the built program out
```

A program is a list of files: the first is the program, any others are files
it includes. The first file is translated to `prog.bas`; the runtime support
file, `etb_prelude.bas`, goes beside it and reaches the compiler through
`-include`, which is what lets it be compiled *first* without putting a line
in front of the user's program.

## The translator: `etb-core/src/translate/`

FreeBASIC's `-lang qb` follows QuickBASIC; Turbo Basic is its own dialect. The
translator closes the gap, and three decisions shape it.

**It edits; it never re-prints.** Each rule turns a statement into edits —
byte ranges of the original line and what goes there — and a line with no edits
is copied byte for byte, whatever code page its strings are in. Nothing is
decoded or transcoded.

**Line numbers are kept.** Edits never cross a line, so line N of `prog.bas` is
line N of the user's file, and nothing at all is added above it. What has to
exist before the program — the declarations of everything it defines, the
variables its functions share with it, the runtime support itself — lives in a
separate file that the compiler is given with `-include`. The only line added
is the call that finishes the program, after the user's last. `LineMap` records
every line's origin, and FreeBASIC's willingness to take SUB and FUNCTION
blocks in the middle of the program is what lets `DEF FN` become a FUNCTION
*in place*.

Three things FreeBASIC will not take, and where they went instead: a statement
after a `FUNCTION` or `SUB` header (it moves to the first line of the body),
`SHARED` inside a procedure (it becomes `DIM SHARED` in the support file), and
a call to a procedure it has not seen declared (every declaration is in the
support file).

**Pure.** No filesystem, no processes, no clock: bytes in, bytes out, and a
list of findings — refusals, warnings, notes — carried as i18n keys so the
window says them in the user's language.

| Module | Job |
|---|---|
| `physical.rs` | Lines as an editor numbers them; each keeps its own ending; Ctrl-Z ends the file |
| `lexer.rs` | Tokens with spans. Splits `PRINT#` the way Turbo Basic did |
| `stmt.rs` | Statements: `:` outside parentheses, and the branches of a single-line IF |
| `rewrite.rs` | The rules, as edits |
| `linemap.rs` | Staged line → the user's file and line, or "ours" |
| `findings.rs` | What the translator has to say, as keys |
| `assets/prelude/etb_prelude.bas` | The runtime support, compiled first: declarations, shared variables, `ETB_DEV$` (printer ports become files), `ETB_CEIL#`, `ETB_BIN$`, `ETB_CLOSED`, `ETB_FINISH` |

Every name in the prelude starts `ETB_` and has an explicit type, so a
`DEFINT` or `DEFSTR` in the user's program cannot change it.

## Where the compiler comes from

It is never in git. `toolchain/<target>.toml` pins FreeBASIC's release archive
by SHA-256; `cargo xtask fetch-toolchain` downloads, verifies, extracts, prunes
to a keep-list, and writes an integrity manifest the application embeds. See
`toolchain/README.md`.

**The version is pinned in several places and they are checked against each
other**: the recipe, the archive's name and URL, the directory the archive
unpacks into, and the bundle descriptor; and `fbc --version` where the host can
run it.

**A compiler whose path cannot be given to it is copied, once.** A compiler on
Windows receives every path — including the path to itself — through
the ANSI code page, which has no Vietnamese: installed under
`C:\Users\Nguyễn Văn A\…` it is handed `Nguy?n Van A` and cannot find its own
files (`docs/verification.md`, F3). `Toolchain::prepare` copies it to an ASCII
directory of ours — 176 MB, once, in about four seconds — and the build runs
from there. The installation is left untouched
and stays the thing integrity is about — `verify_integrity` hashes the
original, never the copy. The copy is stamped with what it was made from and
remade when that changes; an interrupted copy has no stamp and is not reused.
On a machine whose paths are ASCII, `usable_in_place()` is true and nothing is
copied at all.

**The compiler writes nothing inside its own directory**, so the integrity
check has no exceptions at all: the recipe's `[mutable]` list is empty, and
`cargo test -p etb-testkit --test mutable` builds a program and fails if that
stops being true. QB64-PE needed a list there — it built its own runtime and
kept its settings beside itself — and the machinery is kept for the next
compiler that does.

**A damaged bundle is never replaced by whatever is on PATH.** `discover` falls
through to an `fbc` on PATH only when no bundle was shipped at all; a bundle
that is present and will not load is reported as a damaged installation.

**The environment is built, not inherited.** On x86-64 `fbc` translates to C
and runs GCC and a linker over it, and a stray `CPATH`, `LIBRARY_PATH` or
`CFLAGS` would change what those do, so the compiler search paths and the
make-style variables are banned; a bundle descriptor may add variables but
never those.

**Every flag is pinned, and a test says so.** `-lang qb` above all: without it
`fbc` compiles modern FreeBASIC, where a variable must be declared before use
and `GOSUB` does not exist, and every program this application exists for
would fail at once.

## What CI enforces

`cargo xtask check-hygiene` turns the structural rules into build failures:

- only `fs_guard.rs` may create, write, copy, rename, retime or delete files;
- never spawn a child through a shell;
- `etb-gui` may not use `std::fs` or `std::process`;
- no compiler flag outside the pinned set (`build/fbargs.rs` and its test);
- no binaries committed, nothing tracked over 1 MB;
- **every tracked BASIC file starts with `' Written for Easy Turbo Basic`** —
  programs other people wrote are tested locally, from where they live, and are
  never committed;
- the committed toolchain manifest stays a placeholder;
- the committed icons still match `logo.png`;
- every translation key named in the code exists in **both** catalogs.

Keys chosen at run time — translator findings, file refusals — are beyond that
scan, so each module that chooses one has a test that checks both catalogs.

## Tests

| Tier | Needs | Where |
|---|---|---|
| 0 | nothing | unit tests beside the code — the translator's are the bulk |
| 1 | a fake FreeBASIC | `etb-testkit/tests/pipeline.rs`, `bundle.rs` |
| 2 | a real FreeBASIC | `corpus.rs`; `examples.rs`; `numbers.rs`; `mutable.rs`; `local_corpus.rs` |

Tier 2 skips when no compiler is found; `ETB_REQUIRE_TOOLCHAIN=1` turns that
skip into a failure. Corpus programs are built in **console test mode** — one
injected `$CONSOLE:ONLY` line, recorded in the line map, and `SYSTEM` instead
of the wait at `END` — so a test can type answers and read the screen. Programs
that draw are built in **window test mode** instead: they run in their own
window on a virtual screen (`xvfb-run`), waits for a key are answered with
Enter, and `ETB_FINISH` saves a picture of the screen, whose size and colours
the test checks. Neither mode is ever set by the window the user sees. Wrapped
around every case is the assertion the product exists for: the sources are
byte-identical before and after.

`local_corpus.rs` runs cases kept outside the repository, beside programs that
are not ours to publish, named by `ETB_LOCAL_CORPUS`. It builds from a copy and
hashes the original besides.

The fake FreeBASIC finds its instructions beside the build tree it is handed,
not in its working directory, which for the fake is shared by every test.

## Things that look wrong and are not

- **`prog.bas` has one line more than the user's file.** It is the last: the
  call that finishes the program. Nothing is added above the user's lines, so
  no line moves — everything that has to come first is in the runtime support
  file, which reaches the compiler through `-include`.
- **Inactive lines are commented with `REM .`, not `'`.** FreeBASIC reads a
  `$word` at the start of any comment as one of its own directives, so a
  commented-out `$IF` would be parsed and rejected (`docs/verification.md`,
  F11).
- **The corpus runs programs from their own folder.** A program writes the
  files it names where it was started from, which after a double-click is its
  own folder (F2).
- **`?` is rewritten to `PRINT` everywhere.** The compiler accepts `?` only in
  some positions.
- **A second copy of the whole compiler, under our data directory.** Only on a
  machine where the installed one sits at a path it cannot be given (F3), and
  only made once. `etb-cli doctor` prints both paths when there is a copy.
- **A lock file in the system temp directory.** Only while a copy of the
  compiler is being made, so two applications starting at once make it once
  between them. Builds themselves need no lock any more: `fbc` writes only
  where it is told.

## Where to look

| Question | File |
|---|---|
| What does a Turbo Basic line become? | `translate/rewrite.rs`; `etb-cli translate FILE --map` |
| Why was my file refused? | `build/sourcefmt.rs` |
| How is an error put on the right line? | `build/diagnostics.rs`, `translate/linemap.rs` |
| How is the compiler found and trusted? | `toolchain/mod.rs`, `toolchain/manifest.rs` |
| Where does anything get written? | `fs_guard.rs` — everywhere, or nowhere |
| What has been checked against the compiler itself? | `docs/verification.md` |

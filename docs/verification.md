# What has been verified, and how

Facts this code depends on, how each was established, and against what.
"Under wine" means FreeBASIC 1.10.1's Windows release driven from Linux
(wine 11.0) — evidence, not proof, until the same is seen on Windows.

This file changed shape when the compiler did. Everything measured against
**Turbo Basic itself** (V1, V13, and the goldens in `tools/tb-oracle/`) still
stands: it is about the language, not about what compiles it. Everything
measured against QB64 Phoenix Edition was re-measured against FreeBASIC, and
what was found is below.

## Turbo Basic, the language

| # | Question | Status | How |
|---|---|---|---|
| V1 | Turbo Basic's default storage for undeclared variables in a SUB: STATIC or LOCAL? The 1987 handbook says both (Table 4-4 and p.93: STATIC; the SUB reference: local). | **Settled: STATIC** | The original compiler was asked, in DOSBox. See below. |
| V13 | How does Turbo Basic print numbers, and what does its arithmetic answer? | **59 values measured** | The original compiler, in DOSBox. See below. |

## FreeBASIC, the compiler this builds with

| # | Question | Status | How |
|---|---|---|---|
| F-a | Does `fbc -lang qb` accept what the translator emits? | **After four changes** | Probes, compiled and run; see below. |
| F-b | Is a SUB or FUNCTION in the middle of the main program skipped when execution reaches it? | **Yes** | Probe. |
| F-c | Does a build work headless on Linux, and can stdout be captured? | **Yes** | Natively, and under wine. |
| F-h | What does the Windows bundle actually contain? | **fbc, GCC 9.3.0, binutils 2.34, MinGW-w64** | Read from the binaries: on x86-64 FreeBASIC has no assembly backend, so it emits C and compiles it. |
| F-d | Does the compiler write anything inside its own directory? | **No** | `cargo test -p etb-testkit --test mutable`, which fails if that changes. |
| F-e | Can a pure-Rust extractor decode the Windows archive? | **Yes** | It is a zip; `cargo xtask fetch-toolchain`. |
| F-f | How large is the bundle, and how long is a build? | **37 MB; under a second** | See below. |
| F-g | Real printing, with no printer, to Print-to-PDF, several pages. | **Open** | On Windows, and yours to check. |

### F-a: the four things FreeBASIC would not take

Each found by handing it a probe and reading what it said, not by reading its
manual:

1. **A procedure must be declared before it is called.** These programs call
   functions defined further down, and without a `DECLARE` the call is read as
   an array reference: `Array not dimensioned`. Every declaration now goes in
   the runtime support file, which is handed over with `-include` and so is
   compiled first.
2. **No statement may follow a `FUNCTION` or `SUB` header.**
   `FUNCTION F(x): SHARED y` is `Expected End-of-Line`. What the header implies
   moves to the first line of the body, which keeps the line count.
3. **No `SHARED` inside a procedure**, in any dialect. What Turbo Basic shares
   with the main program becomes `DIM SHARED` at module level — `REDIM SHARED`
   for an array — in that same support file.
4. **Arguments are by reference unless the header says `BYVAL`.** Turbo Basic's
   `DEF FN` passes by value: without this, a function that changed its
   parameter changed the caller's variable. Measured, not assumed — the corpus
   case `deffn` prints the caller's variable afterwards.

And two the translator has to hide:

- **`$word` at the start of any comment is a directive.** `'$IF`, `REM $IF`,
  even `' $IF`, are all parsed and rejected; only text before the `$` hides it.
  Inactive lines are commented `REM .` (F11). `$DYNAMIC` and `$STATIC` are the
  exception: FreeBASIC has those, so they are kept as directives.
- **`DEF FN` does not exist in `-lang qb` or `-lang fblite`.** It is translated
  to a `FUNCTION`, exactly as it was for QB64.

### F-f: size and speed

| | QB64-PE 4.6.0 | FreeBASIC 1.10.1 |
|---|---|---|
| download | 102 MB | 37.5 MB |
| Windows bundle, pruned | 403 MB | 176 MB |
| what the installer carries | 424 MB | 196 MB |
| **the installer itself** | **65.6 MB** | **26.4 MB** |
| a corpus build, natively | ~0.5 s | ~0.1 s |
| a corpus build, under wine | ~6 s | ~4.6 s |
| the copy for a Vietnamese user name | 403 MB, ~6 s | 176 MB, under a second |

### Dialect details, checked on this compiler

`CASE < 5` without `IS`; `EXIT SELECT`; `?` for PRINT; `KEY OFF`;
`DEF SEG = &HB800: POKE` (accepted, and runs); `1.5D+2`, `&H1F`, `&O17`.

### Graphics, seen

Window test mode runs a program in its own window on a virtual X screen
(`xvfb-run`), answers its waits for a key with Enter, and has it save a picture
of the screen at the end (`BSAVE`, which writes a BMP). The corpus then checks
the picture's size and counts pixels of the colours it should show: SCREEN 12
draws its filled box, its circle and PAINT's fill, and the SCREEN 9 example
draws its axes, curve and circle.

A text program is not a picture any more. FreeBASIC runs one in a console,
where QB64 gave every program a window of its own, so `COLOR`/`LOCATE` output
is checked as text (`screen_text`) rather than as pixels.

## Found along the way

**F1. Both compilers read `PRINT#1,` as a variable.** `print#` lexes as one
name with a double-precision suffix. QB64-PE 4.6.0 reported:

```
Syntax error
Caused by (or after):PRINT# 4 , SPACE$ ( 3 ) ; "x",1
LINE 6:print# 4,space$(3);"x"
```

and FreeBASIC 1.10.1, on the same line of the same real program — a 1995
engineering calculation that is not ours to publish, so its name stands as
`<program>` here — reports:

```
<program>.BAS(155) error 10: Expected '='
print# 4,space$(15);"..."
       ^
```

That second block is the format `build/diagnostics.rs` parses. The translator
puts the space in, and neither compiler ever sees the glued form.

**F2. A program writes where it was started from.** QB64 changed to the
program's own folder at startup (`libqb.cpp`, after `FS_SaveStartDirectory`);
FreeBASIC does not. After a double-click the two are the same thing — Explorer
starts a program in its own folder — but a program started from a shell
somewhere else writes its files there. The corpus and the `numbers` test run
programs from their own folder for that reason.

**F3. A Windows QB64-PE receives its arguments through the ANSI code page.**
Under wine, a path through a folder named `Nguyễn Văn A` arrived as
`Nguy?n Van A` and the source could not be found. Every path QB64-PE is given
must be ASCII — including the path to itself, since it is given its own
`internal` folder.

*Settled.* Two moves, both in `paths.rs`: the work tree and a second root for
the compiler are chosen with `choose_ascii_root`, which falls back to a plain
system scratch directory when our own data directory is not ASCII. Before the
first build, `Toolchain::prepare` copies an installation that cannot be named
— `root()` is not ASCII — into that second root, keeping file times, because
`make` decides what to rebuild from them. The installation is left alone and
stays the thing integrity is about: `verify_integrity` hashes the original,
never the copy. The copy is stamped with where it came from, the compiler's
version, the driver's size and date and the manifest's digest, and remade when
any of those change; a copy interrupted part way has no stamp and is not
mistaken for a finished one.

Measured on this machine, against the real Windows bundle put under
`Người dùng/`, driven through wine
(`cargo test -p etb-testkit --test bundle -- --ignored vietnamese`):

| | |
|---|---|
| first build, including the copy of 799 MB | 12.4 s |
| second build, reusing the copy | 6.5 s |

So the cost is one copy of the compiler, once, on the machines that need it —
and nothing at all on the machines that do not, because `usable_in_place()` is
true and `prepare` returns the compiler itself. The copy has its own disk cost,
which is the argument for the prune still to come (M4).

The same test proves the point end to end: a QB64-PE at a Vietnamese path
builds a program, where before it could not find its own files.

And once more through what is actually shipped —
`packaging/windows/verify-vietnamese-under-wine.sh` puts the packaged product
at `C:\users\Nguyễn Văn A\AppData\Local\Programs\Easy Turbo Basic`, points
its data directory at the same account, and drives the real `etb-cli.exe`
under wine. This run is the QB64-era one, kept because it is the evidence the
design rests on; the same arrangement measured against FreeBASIC is F12, and the
sizes there are a tenth of these:

```
config dir : C:\users\Nguyễn Văn A\AppData\Roaming\Easy Turbo Basic\config
work root  : C:\ProgramData\EasyTurboBasic\work\4b4b677deda05c33
compiler   : C:\users\Nguyễn Văn A\...\Easy Turbo Basic\toolchain\qb64pe.exe
integrity  : ok (5739 files verified)
installed  : C:\users\Nguyễn Văn A\...\toolchain
run from   : C:\ProgramData\EasyTurboBasic\tools\...\qb64pe.exe  (the installed path cannot be given to it)
smoke test : ok
```

The build succeeded, the source file's hash was unchanged, and the program it
produced wrote both its results file and `MAY-IN-LPT1.TXT`. Note what integrity
verified: the 5739 files of the *installation*, not the copy.

The disk cost of that account: 421 MB installed plus a 404 MB copy. On a
machine whose paths are ASCII there is no copy at all.

**F4. `$CONSOLE:ONLY` only counts on the first line.** Placed at the end of a
program it is silently ignored and the output goes to a window. Console test
mode therefore costs exactly one injected line, which the line map records.

**F5. Nothing waits at `END`. The wait is ours to emit.** QB64-PE paused at
`END` and on falling off the end, printing "Press any key to continue"
(`libqb.cpp`), and `SYSTEM` was the way to avoid it. FreeBASIC does neither:

```
$ printf 'PRINT "done"\nEND\n' > ENDW.BAS && fbc -lang qb ENDW.BAS -x endw
$ ./endw            # prints, exits, returns 0 immediately
```

For a while after the compiler changed, nothing emitted a wait, so the option
*Wait for a key before the window closes* — on by default, described in the
help — did nothing at all: a double-clicked program printed its results into a
window that closed the same instant.

*Settled.* `translate::prelude_text` puts `WHILE INKEY$ <> "": WEND` and a bare
`SLEEP` in `ETB_FINISH`, and a unit test fails if either goes missing. Measured
for the two cases that matter: bare `SLEEP` waits for a key on a terminal (on a
pty, still running after 1.5 s; it exits when a key arrives) and returns at once
where there is no terminal, so a Linux test or CI run cannot hang on it.

Windows is not the same: the same program built for Windows and run under wine
waits at `SLEEP` even with stdin at `/dev/null`. For the user that is the point.
For automation it means anything that *runs* a built program must build it with
`--no-keep-open`, or expect to be waiting — which is what
`packaging/windows/verify-vietnamese-under-wine.sh` now checks for, treating a
timeout as the pass. Nothing is
printed with it — a program that drew a graph would have the picture scrolled
away by a two-line prompt — which is also what Turbo Basic did: the screen
stayed as the program had painted it.

**F6. `qb64pe.exe` under wine accepted Unix paths** (`/home/...`), so the
application could pass the same absolute paths on every host. Not relied on any
more: the wine scripts hand the Windows `etb-cli.exe` Windows paths
(`C:\etb-work\...`), which is what a Windows machine gives it anyway.

**F7. An error inside an included file** reaches the user on that file and that
line. QB64-PE reported it after a `\x01` in the message (`Name already in
use\x01 in line 3 of etb_inc_1.bas included`) with `LINE` naming the `$INCLUDE`
line; FreeBASIC names the included file directly, in the `FILE(LINE) error N:`
form `build/diagnostics.rs` parses. The conclusion is the one that is tested
either way: the corpus case `include_error` gets its message on the included
file's line 2.

**F8. Two builds at once through one QB64-PE produced the wrong program**
(kept because it is why the lock exists at all, and the lock still guards the
copy of a compiler being made).
QB64-PE wrote its intermediate C++ into its own directory, in the same place
for every program. Two builds overlapping in time — seen when two tests ran in
parallel — gave one of them the *other's* program: a test expecting
`phan chinh` got `HESO = 2.50`. FreeBASIC writes only where it is told (F9), so
builds no longer take turns; the lock remains around the one thing that is
still a whole tree appearing at once, the copy `Toolchain::prepare` makes.

**F9. A build writes nothing inside the compiler's own directory.** Measured
the same way the old answer was: build a program, compare the tree before and
after. QB64-PE wrote its generated C++, its settings and the runtime it
compiled on first use, and the recipe had to list all of that as `[mutable]`
so the integrity check would not cry wolf. FreeBASIC writes only its output,
so the list is empty — and `cargo test -p etb-testkit --test mutable` fails if
that stops being true.

**F10. What the Windows bundle can lose.** The FreeBASIC release is 181 MB
extracted, of which `examples/` is 4.6 MB in 1689 files a build never opens.
Pruned: 176 MB, which compresses to a 26 MB installer. `doc/` is *not* dropped,
though a build never opens it either: 75 KB of it is the licence text FreeBASIC
ships with its own binaries — `gpl.txt`, `lgpl.txt`, `libffi-license.txt` — and
dropping it meant distributing GPL binaries having deleted the licence they came
with. The notice `cargo xtask package` writes points a reader at those files by
name, and the integrity manifest attests them like everything else.

(The same measurement against QB64-PE dropped 399 MB of sysroots for other
architectures, a bundled Python, lldb and clangd — the reason for the change of
compiler is in that number.)

**F11. `$word` at the start of a comment is a FreeBASIC directive.** `'$IF`,
`REM $IF` and `' $IF` are all parsed, and an unknown one is an error; only
text before the `$` hides it. Lines the translator comments out — a branch a
`$IF` did not take, a metastatement with no meaning today — are therefore
commented with `REM .`. The two that FreeBASIC does have, `$DYNAMIC` and
`$STATIC`, are kept as directives instead, so arrays behave as the program
asked.

## F14: what a decimal constant is, and why the handbook cannot settle it

**Every decimal constant in Turbo Basic is double precision.** Asked of the
compiler itself, `IF c = c#`:

| constant | Turbo Basic | `fbc -lang qb` |
|---|---|---|
| `3.14159` | equal, so **double** | not equal, so single |
| `.7` | **double** | single |
| `1.5` | **double** | single |

so the translator marks them: `.5` → `.5#`, `1.5E+2` → `1.5D+2`. Whole numbers
are integers in both and are left alone. Measured consequence, both compilers
on the same program:

| | Turbo Basic | without the rule |
|---|---|---|
| `PRINT 3.14159 * 100.5 ^ 2` | `31730.8443975` | `31730.84559345245` |
| `PI# = 3.14159 : PRINT PI#` | `3.14159` | `3.141590118408203` |

**The handbook is wrong here** (p.68: "up to six digits … single-precision"),
which is the third time it has disagreed with its own compiler — after V1 and
after its claims about overflow and division by zero. The rule for this
project is now explicit: *where the handbook and the compiler disagree, the
compiler wins, and the measurement goes in this file.*

This rule was removed once, on the strength of the handbook and of a probe
where `IF k = .7` against a single variable took the branch that looked wrong.
Turbo Basic answers `NOTEQUAL` there too — a single variable never did equal a
double constant, in 1987 or now — so removing it did not fix a wrong answer,
it introduced one, in every number computed from a constant. It is back, and
the corpus case `thu_tuc_va_bien` asserts the `NOTEQUAL` that Turbo Basic
gives.

### The one difference that cannot be removed

Turbo Basic evaluated intermediates in the 8087's 80 bits (handbook p.410,
and this one is true). x86-64 has no 80-bit arithmetic. Where a whole
expression is single precision, results can differ by one unit in the last
place — the seventh significant digit. `LOG10` and `LOG2` were the visible
case: written out as `LOG(x) / LOG(10)` they gave 2.9999999999999996 for 1000,
so `INT(LOG10(1000))` was 2 and a logarithmic axis lost a decade. They now call
the C library's correctly-rounded `log10` and `log2`, which give Turbo Basic's
answers exactly (3, 6, 6, 10).

## V13: the numbers, against Turbo Basic's own

59 values — plain `PRINT`, `PRINT USING`, integer division, rounding, the
TB-only functions — through the original compiler and through this
application. **29 agree exactly**, and the 30 that do not are listed in
`cargo test -p etb-testkit --test numbers`, which fails if that list changes in
either direction. `tools/tb-oracle/` holds the probes and both answers.

**What was fixed by measuring.** Turbo Basic prints `2 ^ .5` as
`1.414213562373095`; both QuickBASIC-family compilers printed `1.414214`. The
reason is not the printing but the arithmetic: **an unsuffixed constant with a
decimal point is double precision in Turbo Basic and single in QuickBASIC**, so
the whole calculation was being done in single. The translator marks such
constants double (`.5` → `.5#`, `1.5E+2` → `1.5D+2`; whole numbers are integers
in both languages and are left alone), and `1/3`, `2^.5`, `SQR(2)`, `ATN(1)`,
`3.14159*2` and `1.1+2.2` now carry the digits Turbo Basic carried.

**What differs is how a number is written, not what it is.** Two habits account
for most of the thirty:

| | Turbo Basic | FreeBASIC |
|---|---|---|
| after a number | a space | nothing |
| a fraction | `.333` | `0.333` |
| exponential | `1E-004`, `1E+020` | `1e-05`, `1e+20` |

and the rest:

| | Turbo Basic | FreeBASIC | why |
|---|---|---|---|
| a SINGLE variable | `.3333333432674408` | `0.3333333` | TB prints the exact stored value, up to 16 digits; seven for a SINGLE here |
| a SINGLE above 1E7 | `12345678` | `1.234568e+07` | same cause |
| below 0.1 | `1E-002` | `0.01` | TB goes exponential below 0.1 |
| 16th digit of a double | `.1428571428571429` | `0.1428571428571428` | one unit in the last place |
| `USING "#.###^^^^"` | `1.235E+03` | `0.123E+04` | TB normalises to one digit before the point |
| `USING` overflow | `%123.456 ` | `%123.46` | TB shows the number unformatted after the `%` |

**What got better with the change of compiler.** QB64 rounded `PRINT USING
"###.##"; 7.005` to `7.00` — it rounds the binary value, and 7.005 is really
7.00499… — where Turbo Basic gives `7.01`. FreeBASIC gives `7.01`, and the same
for `-1.005`. The sign position in `USING "+###.#"` also matches now.

What matches, and is worth saying plainly: integer division and `MOD` with
negative operands, `CINT`'s round-half-to-even, `INT`, `FIX`, `CEIL`, `STR$`,
`HEX$`, `OCT$`, `BIN$`, `VAL` of `&H`, `LOG2`, `LOG10`, `EXP2`, `EXP10`, and
thirteen of the fifteen `PRINT USING` forms.

The trailing space is the one a user will notice: a table of numbers is spaced
differently from the printout they remember. The values are the same, which is
what was asked for.

## F12: FreeBASIC mangles a Vietnamese path exactly as QB64 did

Measured, because the whole ASCII-path arrangement rested on a finding about
the compiler that was replaced. The Windows `fbc.exe`, under wine, given a
source at `C:\users\Nguyễn Văn A\Tài liệu\CHUONGTRINH.BAS`:

```
C:\users\Nguy?n Van A\T?i li?u\CHUONGTRINH.BAS() error 23: File not found
```

So the work tree and, where the installation itself sits at such a path, a
copy of the compiler still have to go somewhere ASCII, and on Windows that
means `%ProgramData%`.

### What follows from that, and what it cost

`%ProgramData%` is shared. Every account on the machine can create directories
there, and whoever creates one owns it and decides what may go inside. Two
things were done about it:

- **The name is the user's own secret**, 128 bits kept in their configuration
  directory inside their profile, which no other account can read
  (`fs_guard::stable_secret`). It used to be a hash of the profile path, which
  anyone knowing the account name could work out — and create first.
- **A copy of the compiler is trusted for what it hashes to**, not for the
  note beside it. `Toolchain::prepare` now verifies the copy against the
  manifest embedded in this application before reusing it. The note says where
  the copy came from, its version and the driver's size and date, all of which
  someone else could work out from a public release and write down themselves;
  the hashes they cannot forge.

Residual risk, stated plainly: on a machine with more than one account, this
is defence in depth rather than a wall. The wall is an access-control list on
the directory, which needs Windows API calls this code does not make yet. On a
machine with one account — which is what this is for — there is nobody to
defend against.

## F13: the file lock was in the shared temp directory

`/tmp` is writable by everyone, and the lock was `/tmp/etb-locks/<hash>.lock`.
Another account could own that folder, put a symlink where the lock file goes,
or take the lock and never release it — not enough to read anything of ours,
but enough to make every build hang for ever. The locks are now in the
application's own data directory (`AppPaths::lock_dir`), which is the user's.
On Windows this was never exposed: `%TEMP%` is per-user there.

## F15: dividing by zero killed the program and lost its results

`\` and `MOD` divide on the processor, and an integer division by zero there
is a fault rather than an error: SIGFPE, the program killed where it stands,
no message, exit code 136. Measured consequences — the results file the
program had been writing was **0 bytes**, because what it had printed was
still in a buffer, and `ETB_FINISH` never ran, so the printer file was never
written either. On Windows the window simply disappears.

Turbo Basic stopped the program and kept what it had written. Asked:

```basic
OPEN "O", #1, "OUT.TXT" : PRINT #1, "TRUOC" : CLOSE #1
A = 5 : B = 0 : PRINT A \ B
OPEN "A", #1, "OUT.TXT" : PRINT #1, "SAU" : CLOSE #1
```

leaves `TRUOC` in the file and no `SAU`: it stopped at the division.

So the translator wraps the divisor — `a \ b` becomes `a \ ETB_NZ#(b)` — and
the runtime support says what happened in both languages, closes the files and
stops. The wrapping takes the divisor's *term*: `a \ b + c` guards `b`,
`a MOD b * 2` guards `b * 2`, `IF n \ d = 2` guards `d`. The values either
side are unchanged and still match Turbo Basic, including `7.5 \ 2.5` = 4 and
`-7 MOD 3` = -1.

Floating-point division by zero is not affected: neither compiler faults on
it, and neither raises an error, contrary to the handbook (p.386).

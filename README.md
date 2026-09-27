<img src="logo.png" alt="" width="96" align="right">

# Easy Turbo Basic

Build Turbo Basic programs from the 1980s and 1990s into programs that run on
today's Windows. You add your `.BAS` file, press one button, and save the
finished program wherever you like. No terminal, no DOS box, no toolchain to
install.

Built for someone with working Turbo Basic programs — engineering calculations,
menus, results written to files and to the printer — who is not comfortable at
a command line. The interface and the built-in guide are in **Vietnamese and
English**. It is the sibling of Easy Fortran 77, and shares its design.

## Why this is needed at all

No compiler of this century builds Turbo Basic. Turbo Basic 1.x is 16-bit DOS
software, and 64-bit Windows runs neither it nor the programs it made. Its
successor, PowerBASIC, has had no release since 2011. The maintained compilers
for this family of BASIC follow QuickBASIC, and none accepts Turbo Basic as
written: they dropped `DEF FN`, Turbo Basic's only way to write a function, and
they read `PRINT#1,` as a variable name.

So this application **translates** each program into what FreeBASIC's
QuickBASIC dialect accepts (`fbc -lang qb`), and has FreeBASIC build it.
FreeBASIC was chosen because it produces an ordinary Windows program, draws
QuickBASIC's `SCREEN` graphics, is still maintained, and is a quarter of the
size of the alternative: a 26 MB installer against 66 MB, and 176 MB on disk
against 403 MB. On 64-bit machines it translates to C and has the GCC it
carries compile that, which is why the bundle is not smaller still.

## The promise

**Your source files are only ever read. This application never modifies them.**

That is structural, not a policy: one module (`etb-core/src/fs_guard.rs`) is the
only code permitted to write to the filesystem, every write asserts it lands
inside the application's own data directory, `cargo xtask check-hygiene` fails
the build if any other module tries, and the test suite hashes the source files
before and after every build in the corpus and asserts they are byte-for-byte
identical. The translation is written into a work folder; the compiler only ever
sees that copy.

It is also why there is **no source editor here**. Editing your code is
Notepad's job.

## Status

**v0.1 — working on Linux, checked against FreeBASIC's Windows release under
wine.** Translated and verified: GW-BASIC-style programs (line numbers, `?`,
`print#`, `PRINT USING`, files, a printer opened as `"lpt1"`), `DEF FN`,
`SUB` with Turbo Basic's storage rules, `$INCLUDE`, named constants and `$IF`,
`INCR`/`DECR`, the `EXIT` forms and Turbo Basic's own functions; what needs DOS
itself is refused with a reason. Not yet: printing to a real printer (printer
output is saved as a file), the Windows installer, and the check on a real
Windows machine. See `docs/verification.md` for what has been checked, and how.

## Something to try it on

`examples/` holds seven programs, easiest first, with a bilingual guide,
`DOC-TRUOC.txt`, beside them — from INPUT and PRINT USING through functions,
files and the printer to a graph drawn in `SCREEN 9`, a program in two files,
and one broken on purpose. `cargo test -p etb-testkit --test examples` checks
that each does what the guide says.

## Building and running

```sh
cargo run -p etb-gui                          # the application
cargo test --workspace                        # everything that needs no compiler
cargo xtask check-hygiene                     # the structural safety rules

cargo xtask fetch-toolchain --target windows-x86_64   # FreeBASIC, pinned, verified, pruned
toolchain/verify-under-wine.sh                        # ... and exercised from Linux
cargo xtask fetch-toolchain --target linux-x86_64     # or the Linux build, for development
```

The Linux FreeBASIC drives the system's linker and links against the system's
X and ncurses libraries, so it needs (Fedora): `gcc binutils ncurses-devel
ncurses-compat-libs libX11-devel libXext-devel libXrender-devel libXrandr-devel
libXpm-devel libXcursor-devel libXinerama-devel mesa-libGL-devel`, and
`xorg-x11-server-Xvfb` to run graphical programs in tests. (`ncurses-compat-libs`
is for `libtinfo.so.5`, which the official binary was built against.) Without
installing them on the machine itself, a toolbox container does it:

```sh
toolbox create etb-build
toolbox run -c etb-build sudo dnf install -y gcc binutils ncurses-devel \
    ncurses-compat-libs libX11-devel libXext-devel libXrender-devel \
    libXrandr-devel libXpm-devel libXcursor-devel libXinerama-devel \
    mesa-libGL-devel xorg-x11-server-Xvfb
toolbox run -c etb-build cargo xtask fetch-toolchain --target linux-x86_64
toolbox run -c etb-build env ETB_REQUIRE_TOOLCHAIN=1 \
    ETB_TOOLCHAIN_BUNDLE=$PWD/target/toolchain/linux-x86_64 cargo test --workspace
```

### The headless driver

`etb-cli` runs the same pipeline without a window. It is how tests exercise
everything, and how you diagnose an installation over the phone.

```sh
etb-cli doctor                          # which FreeBASIC, is it intact, does it run
etb-cli build TINHTOAN.BAS --out ~/Desktop/tinhtoan.exe
etb-cli translate TINHTOAN.BAS --map    # what the compiler is given, line by line
```

## Layout

Changing this code? Start with [ARCHITECTURE.md](ARCHITECTURE.md).

| Crate | What it is |
|---|---|
| `etb-core` | All the logic, including the translator. No GUI dependencies, so the whole pipeline is testable headlessly. |
| `etb-gui` | The window (`eframe`/`egui`). Holds no logic; may not touch the filesystem or spawn processes. |
| `etb-cli` | Headless driver for tests and for support. |
| `etb-testkit` | A fake FreeBASIC and a fake program, so the pipeline is testable with no compiler installed; the corpus. |
| `xtask` | Repository chores: the hygiene check, fetching FreeBASIC, packaging. |

## What it does and does not do

It **builds**. It does not run your program — that is yours to do. When the
program ends its window stays open with "Press any key to continue", so the
results can be read after double-clicking it; an option turns that off.

Files your program writes by name land in **the folder it is started from**,
which after a double-click is the folder the program sits in, as in its DOS
folder.

A printer opened as `LPT1`, `LPT2`, `LPT3` or `PRN` — by name, or through a
variable, as programs of this age usually did — becomes a text file beside the
program, `MAY-IN-LPT1.TXT`. There is no printer port to send it to.

## Things that bite, and what is done about them

**The compiler reads `PRINT#1,` as a variable.** The `#` right after the
keyword makes `PRINT#` one name with a double-precision suffix, and the
compiler stops with "Expected '='". The translator puts the space in.

**Errors would otherwise name the wrong file.** The compiler sees our
translated copy, so its line numbers and quoted text are ours. The translator
never moves a line — every rewrite stays within its line, and the runtime
support is a separate file handed over with `-include` — so a line map takes
each message back to the user's file and line, quoting the line as the user
typed it. An error in something we generated is reported as ours, not theirs.

**A procedure must be declared before it is called**, and these programs call
functions defined further down. Every declaration goes into that same runtime
support file, so the program itself gains nothing.

**A Windows compiler reads its arguments through the ANSI code page**, which
has no Vietnamese: a path through `Nguyễn Văn A` arrives as `Nguy?n Van A`. The
user's files are only read by us, and everything the compiler is given sits
under an ASCII path — including the compiler itself, which is copied to one
before the first build if it was installed under a Vietnamese user name. The
installation is left untouched, and is still the one the integrity check
attests to.

**A file with a `.BAS` name need not be a program.** GW-BASIC saved programs in
a compact binary form unless told `,A`, and QuickBASIC had a binary format of
its own. Handed one, the compiler fails on line 1. Files are checked
first, and a binary save is named, with the one `SAVE` that turns it into text.

## No unsafe code

```toml
[workspace.lints.rust]
unsafe_code = "forbid"
```

## Licence

`MIT OR Apache-2.0`. The bundled FreeBASIC compiler is GPL and the assembler
and linker it runs are GNU binutils, also GPL; both are run as separate
programs, and where to get their source is listed in
`toolchain/windows-x86_64.toml`. FreeBASIC's runtime, which is linked into the
programs it builds, is licensed so that those programs carry no GPL obligation
of their own.

No fonts are bundled: the interface uses whatever ordinary font the machine
already has.

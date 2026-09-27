# The bundled FreeBASIC

The application ships its own compiler so nobody ever has to install one. Each
platform has a *recipe* here naming the exact release archive, pinned by
SHA-256, and a `bundle.toml` template telling the application how to drive the
result.

| File | What it is |
|---|---|
| `windows-x86_64.toml` | FreeBASIC's Windows release, which carries its own assembler and linker, the prune rules, and the source of every component |
| `windows-x86_64.bundle.toml` | the `bundle.toml` shipped inside that bundle |
| `linux-x86_64.toml` | FreeBASIC's Linux release. Development and CI only |
| `linux-x86_64.bundle.toml` | its `bundle.toml` |

```sh
cargo xtask fetch-toolchain --target windows-x86_64   # download, verify, extract, prune (181 -> 176 MB)
cargo xtask fetch-toolchain --target linux-x86_64     # ... and the Linux one
```

## Verifying what was built

The fetch task writes `crates/etb-gui/assets/toolchain-manifest.txt`, a SHA-256 of
every file in the bundle, and the application embeds it. At startup it re-hashes
the bundle in the background and refuses to build with a compiler that does not
match — distinguishing a **missing** file (almost always antivirus, and
recoverable) from a **changed** one (not, and not).

The manifest is embedded rather than shipped inside the bundle deliberately: one
sitting next to the compiler could be rewritten by whatever rewrote the compiler.

The committed copy is a placeholder. Running the fetch task overwrites it, and
that generated version is **not** committed — it belongs to one specific fetched
bundle, and a release regenerates it during packaging.

## What the prune drops

Only `examples/` and `doc/` — 4.7 MB of FreeBASIC's own sample programs and
its manual, which a build never opens. Nothing else in the release is spare:
the compiler, its assembler and linker, its headers and its runtime libraries
are all reachable.

It was a different story with the compiler this replaced, where 399 MB of the
803 went: sysroots and driver programs for four other architectures, a bundled
Python, lldb, clangd and clang-tidy, and compiler-runtime libraries built for
Linux targets inside a Windows toolchain. That measurement is in
`docs/verification.md`, F10, and the method — run a build under wine with
`WINEDEBUG=+file` and collect every path it opened — is worth repeating after
a version bump.

A prune is only correct if the whole corpus still passes against the pruned
tree, which is what `toolchain/verify-under-wine.sh` runs.

## What the compiler is allowed to write

Nothing. `fbc` writes its output where it is told and leaves its own directory
alone, so the recipe's `[mutable]` list is empty and the integrity check has no
exceptions at all. That is measured, not assumed:
`cargo test -p etb-testkit --test mutable` builds a program and fails if
anything under the compiler's directory changed.

The machinery is kept because it was needed once — QB64-PE compiled its own
runtime, kept settings and wrote its generated C++ beside itself, and every one
of those paths had to be listed or every user would have been told their
compiler had been altered after the first build.

## An install path the compiler cannot be given

A compiler on Windows receives every path through the ANSI code page, which has
no Vietnamese, so an installation under `C:\Users\Nguyễn Văn A\…` cannot find
what it was given (`docs/verification.md`, F3). The application copies such an
installation, once, to an ASCII directory of its own — 176 MB, under a second —
and builds from there. The installation itself is never written to, and stays
the thing the manifest attests to.

`toolchain/verify-under-wine.sh` exercises it against the real bundle, and
`packaging/windows/verify-vietnamese-under-wine.sh` does it once more through
the packaged product, installed where a Vietnamese user name puts it.

## Exercising the Windows bundle from Linux

```sh
toolchain/verify-under-wine.sh
```

The Windows bundle is fetched on a machine that cannot run its binaries.
Without this there is nothing between "the files look right" and "Windows CI
said so", which is a long way to push a mistake.

The script gives the fetched bundle a `launcher = "wine"` and a `WINEPREFIX`
through its `bundle.toml`, then runs `doctor` and the whole corpus. It uses its
own wine prefix under `target/`, not your `~/.wine`. A shipped bundle carries
none of that — `cargo xtask package` refuses a bundle that does.

**wine is not Windows.** A pass here is evidence, not proof. It earned its
place on the first day: it showed that a Windows compiler receives its
arguments through the ANSI code page, so a Vietnamese path arrives as
`Nguy?n Van A`.

## Corresponding Source

```sh
cargo xtask fetch-sources --target windows-x86_64 --list   # what, and from where
cargo xtask fetch-sources --target windows-x86_64          # download it, write SOURCES.md
```

FreeBASIC's compiler is GPL, and the assembler and linker it runs are GNU
binutils, also GPL. Both oblige us to offer their source from the same place as
the binaries, for as long as those are distributed. The recipe's `[[source]]`
entries say where each component's source is and what it covers.

FreeBASIC's *runtime*, which is linked into the programs it builds, is licensed
so that those programs carry no GPL obligation of their own — which matters,
because the programs this application builds are the user's.

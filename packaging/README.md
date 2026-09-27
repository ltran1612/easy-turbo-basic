# Packaging

```sh
cargo xtask fetch-toolchain --target <t>    # build the compiler bundle (writes the manifest)
cargo build --release -p etb-gui             # build the application (embeds the manifest)
cargo xtask package --target <t>            # stage and archive
```

The order is not cosmetic: the fetch writes the integrity manifest and the build
embeds it, so building first ships an application that cannot verify the compiler
it ships with.

A package is the application, the compiler in `toolchain/` beside it — which is
where discovery looks, at `<exe>/../toolchain` — the licences, and a short
bilingual `README.txt`. Linux gets a `.tar.gz`, Windows a `.zip`, and Windows
additionally gets an NSIS installer.

## The Windows installer

`packaging/windows/installer.nsi`, built by CI on `windows-latest`. Two choices
worth stating because both are deliberate:

- **Per-user install**, into `%LOCALAPPDATA%\Programs`, so there is no UAC
  prompt. An elevation dialog on an unsigned installer is where a non-technical
  user stops, and that costs more than an admin-only directory is worth. The
  toolchain integrity manifest recovers most of that protection anyway.
- **Uninstalling leaves `%APPDATA%` alone.** The user's list of programs lives there,
  and removing the application should not throw away the user's work.

### Checking it without waiting for CI

```sh
cargo xtask package --target windows-x86_64
packaging/windows/verify-installer-under-wine.sh
```

Builds the installer with `makensis` and runs the whole cycle under wine: silent
install, shortcuts, the uninstall registry entry, compiling a program with the
*installed* compiler, then uninstalling and confirming the install directory,
shortcuts and registry entry are gone while the user's saved programs survive.

It needs `mingw32-nsis` and `wine`. Both wine scripts use a prefix under
`target/`, not your `~/.wine`.

**What this does not prove.** wine is not Windows. And this machine has no
Windows Rust target, so unless one has been cross-built the application inside
the installer is a placeholder — what gets validated is the installer, not the
application. CI builds both for real.

> If you run the verification locally, note that `cargo xtask package
> --target windows-x86_64` packages whatever sits at
> `target/release/easy-turbo-basic.exe`. Delete a placeholder when you are done
> with it, so a later package cannot pick it up thinking it is real.

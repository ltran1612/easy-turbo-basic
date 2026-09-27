#!/usr/bin/env bash
# Build the Windows installer on Linux and exercise it under wine.
#
# CI builds the real thing on windows-latest. This exists so the installer can be
# changed without waiting for a push to find out it is broken, and it checks the
# parts that are easy to get wrong and invisible until someone runs it: whether
# it installs without asking for administrator rights, whether the shortcuts and
# the uninstall entry appear, whether the bundled compiler still works from the
# installed location, and whether uninstalling leaves the user's saved programs
# alone.
#
# The application inside it is whatever `cargo xtask package` staged. What is
# validated here is the installer, not the application.
set -euo pipefail

command -v makensis >/dev/null || { echo "makensis is not installed (dnf install mingw32-nsis)"; exit 1; }
command -v wine     >/dev/null || { echo "wine is not installed"; exit 1; }

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo"
version="$(grep -m1 '^version = ' Cargo.toml | cut -d'"' -f2)"
staged="$repo/target/dist/EasyTurboBasic-$version-windows-x86_64"
setup="$repo/target/dist/EasyTurboBasic-$version-Setup.exe"
export WINEPREFIX="${ETB_WINEPREFIX:-$repo/target/wineprefix}"
export WINEDEBUG=-all

[ -d "$staged" ] || { echo "no staged package; run: cargo xtask package --target windows-x86_64"; exit 1; }

echo "== building the installer =="
makensis -DSRC="$staged" -DVERSION="$version" \
  -DICON="$PWD/packaging/windows/icon.ico" \
  -DOUT="$setup" packaging/windows/installer.nsi | tail -3
ls -l "$setup" | awk '{printf "installer: %.1f MB\n", $5/1e6}'

user_dir="$WINEPREFIX/drive_c/users/$USER"
installed="$user_dir/AppData/Local/Programs/Easy Turbo Basic"
# Where `directories` puts our configuration on Windows:
# %APPDATA%\<organization>\<application>\config (see etb-core/src/paths.rs).
config="$user_dir/AppData/Roaming/easy-turbo-basic/Easy Turbo Basic/config"

# Plant something that must survive uninstalling: the user's list of programs.
mkdir -p "$config"
printf 'schema_version = 1\n\n[[program]]\nname = "Tinh dam be tong"\n' > "$config/programs.toml"

# And something that must not: a work tree, where a build would have left one.
# The uninstaller asks the application to clear these, because they can live
# outside the installation directory and only the application knows where.
work="$user_dir/AppData/Roaming/easy-turbo-basic/Easy Turbo Basic/data/work/left-behind"
mkdir -p "$work"
head -c 4096 /dev/zero > "$work/program.exe"

echo
echo "== installing (silently, and with no elevation) =="
wine "$setup" /S
[ -d "$installed" ] || { echo "FAIL: nothing installed at $installed"; exit 1; }
du -sh "$installed" | awk '{print "installed:", $1}'

echo
echo "== what it left behind =="
find "$WINEPREFIX/drive_c" -path '*Start Menu*' -name '*.lnk' 2>/dev/null | sed 's|.*Programs/|  shortcut: |'
wine reg query 'HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\EasyTurboBasic' 2>/dev/null \
  | grep -E 'DisplayName|DisplayVersion' | sed 's/^\s*/  /'

echo
echo "== the examples came with it =="
for f in examples/DOC-TRUOC.txt examples/01-CO-BAN.BAS examples/06-NHIEU-TEP/HANGSO.BAS; do
  [ -f "$installed/$f" ] && echo "  $f" || { echo "FAIL: $f was not installed"; exit 1; }
done

echo
echo "== the installed compiler still works =="
# FreeBASIC itself, from where the installer put it, on a program of its own
# dialect: this is about the installation, not about the translator.
# Inside the prefix's own C: drive. A path outside it reaches the compiler only
# through wine's Z: mapping, which a prefix need not have, and the failure
# looks like a broken compiler rather than a missing drive.
t="$WINEPREFIX/drive_c/etb-check"
rm -rf "$t"; mkdir -p "$t"
printf 'PRINT 6 * 7\r\nSYSTEM\r\n' > "$t/t.bas"
(cd "$installed/toolchain" && wine ./fbc.exe -lang qb 'C:\etb-check\t.bas' -x 'C:\etb-check\t.exe')
wine "$t/t.exe"

echo
echo "== uninstalling =="
wine "$installed/Uninstall.exe" /S || true
sleep 2
fail=0
[ -d "$installed" ] && { echo "FAIL: install directory survived"; fail=1; } || echo "  install directory removed"
[ "$(find "$WINEPREFIX/drive_c" -path '*Start Menu*' -name '*Turbo*' 2>/dev/null | wc -l)" -eq 0 ] \
  && echo "  shortcuts removed" || { echo "FAIL: shortcuts survived"; fail=1; }
[ -d "$work" ] && { echo "FAIL: a work tree survived the uninstall"; fail=1; } || echo "  work trees removed"
# The one thing uninstalling must NOT take with it.
[ -f "$config/programs.toml" ] && echo "  the user's saved programs kept" || { echo "FAIL: the user's saved programs were deleted"; fail=1; }
rm -rf "$t"
exit $fail

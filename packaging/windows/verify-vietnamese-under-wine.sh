#!/usr/bin/env bash
# The shipped product, installed under a Vietnamese user name, building a
# Turbo Basic program — under wine, from Linux.
#
# This is the path the whole application is shaped around. A Windows compiler
# receives every path through the ANSI code page, which has no Vietnamese, so
# an installation under `C:\Users\Nguyễn Văn A\…` cannot find its own files
# (docs/verification.md, F3). The answer is `Toolchain::prepare`: a copy of the
# compiler at a plain ASCII path, made once, with the installation left alone.
#
# What this proves: the installer's output, at a Vietnamese path, with the
# application's own data directory at a Vietnamese path too, builds a program
# and leaves the source untouched — through the real `.exe`s, not a Linux
# build of the same code.
#
# What it cannot prove: that wine mangles the path the way Windows does. It did
# once, which is how F3 was found, but a pass here is evidence, not proof. The
# Windows check is a person's job.
#
# Usage:  packaging/windows/verify-vietnamese-under-wine.sh
set -euo pipefail

command -v wine >/dev/null || { echo "wine is not installed"; exit 1; }

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo"
version="$(grep -m1 '^version = ' Cargo.toml | cut -d'"' -f2)"
staged="$repo/target/dist/EasyTurboBasic-$version-windows-x86_64"
cli="$repo/target/x86_64-pc-windows-gnu/release/etb-cli.exe"
export WINEPREFIX="${ETB_WINEPREFIX:-$repo/target/wineprefix-vi}"
export WINEDEBUG=-all

[ -d "$staged" ] || { echo "no staged package; run: cargo xtask package --target windows-x86_64"; exit 1; }
[ -f "$cli" ] || { echo "no etb-cli.exe; run: cargo build --release --target x86_64-pc-windows-gnu -p etb-cli"; exit 1; }

if [ ! -d "$WINEPREFIX" ]; then
  echo "creating a wine prefix at $WINEPREFIX"
  wineboot -i >/dev/null 2>&1 || true
fi

# The two paths that matter, both with diacritics and a space, as a real
# Vietnamese Windows account produces: where the product is installed, and
# where it keeps its own data.
user='Nguyễn Văn A'
installed="$WINEPREFIX/drive_c/users/$user/AppData/Local/Programs/Easy Turbo Basic"
data="$WINEPREFIX/drive_c/users/$user/AppData/Roaming/Easy Turbo Basic"
win_home="C:\\users\\$user\\AppData\\Roaming\\Easy Turbo Basic"

rm -rf "$installed" "$data"
mkdir -p "$(dirname "$installed")" "$data"
echo "== putting the product where a Vietnamese user name puts it =="
cp -a "$staged" "$installed"
cp -a "$cli" "$installed/etb-cli.exe"
du -sh "$installed" | awk '{print "  installed:", $1}'

work="$WINEPREFIX/drive_c/etb-work"
rm -rf "$work"; mkdir -p "$work"
src="$work/TINHTOAN.BAS"
# Turbo Basic, not QuickBASIC: a glued `print#`, a DEF FN, a printer by name.
printf 'DEF FNDT(R) = 3.14159 * R * R\r\n' >  "$src"
printf 'OPEN "O", #1, "KETQUA.TXT"\r\n'    >> "$src"
printf 'FOR I = 1 TO 3\r\n'                >> "$src"
printf '  A = FNDT(I)\r\n'                 >> "$src"
printf '  PRINT "R ="; I; " S ="; A\r\n'   >> "$src"
printf '  print# 1, "R ="; I; " S ="; A\r\n' >> "$src"
printf 'NEXT I\r\n'                        >> "$src"
printf 'CLOSE #1\r\n'                      >> "$src"
printf 'OPEN "lpt1" FOR OUTPUT AS #2\r\n'  >> "$src"
printf 'PRINT #2, "xong"\r\n'              >> "$src"
printf 'CLOSE #2\r\n'                      >> "$src"
printf 'SYSTEM\r\n'                        >> "$src"
before="$(sha256sum "$src" | cut -d' ' -f1)"

echo
echo "== doctor, through the installation at that path =="
ETB_HOME="$win_home" wine "$installed/etb-cli.exe" doctor 2>&1 | sed 's/^/  /'

echo
echo "== building =="
ETB_HOME="$win_home" wine "$installed/etb-cli.exe" build 'C:\etb-work\TINHTOAN.BAS' \
  --out 'C:\etb-work\tinhtoan.exe' 2>&1 | sed 's/^/  /'

fail=0
after="$(sha256sum "$src" | cut -d' ' -f1)"
[ "$before" = "$after" ] && echo "  source unchanged" || { echo "FAIL: the source file changed"; fail=1; }
[ -f "$work/tinhtoan.exe" ] || { echo "FAIL: no program was built"; exit 1; }

echo
echo "== the compiler was copied somewhere it can be named =="
# ProgramData, because the application's own data directory is not ASCII, and
# under a name that is this user's secret rather than anything derivable from
# their account (docs/verification.md, F12) — so it is found by looking, not
# by knowing.
copy=$(find "$WINEPREFIX/drive_c/ProgramData" -maxdepth 4 -type d -name tools 2>/dev/null | head -1)
found=$(find "$copy" -maxdepth 3 -name 'fbc.exe' 2>/dev/null | head -1)
if [ -n "$found" ]; then
  printf '%s\n' "$found" | sed "s|$WINEPREFIX/drive_c|  C:|"
  du -sh "$copy" | awk '{print "  copy:", $1}'
  case "$copy" in
    */EasyTurboBasic-*/tools) echo "  named from the user's own secret" ;;
    *) echo "FAIL: the copy is at a name anyone could have worked out: $copy"; fail=1 ;;
  esac
else
  echo "FAIL: no copy was made, so the build cannot have gone through one"; fail=1
fi

echo
echo "== running the program it built =="
(cd "$work" && wine ./tinhtoan.exe >/dev/null 2>&1 || true)
for f in KETQUA.TXT MAY-IN-LPT1.TXT; do
  if [ -f "$work/$f" ]; then
    echo "  --- $f"
    sed 's/^/    /' "$work/$f"
  else
    echo "FAIL: $f was not written"; fail=1
  fi
done

exit $fail

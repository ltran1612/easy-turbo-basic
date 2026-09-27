# Asking Turbo Basic

Two probe programs, and what the real Turbo Basic 1.1 compiler answered.

| File | What it is |
|---|---|
| `SOSANH.BAS` | numbers, `PRINT USING`, integer division, rounding, the TB-only functions |
| `KIEUSO.BAS` | what type an unsuffixed constant is, and when TB switches to exponential |
| `tb-golden.txt` | what **Turbo Basic** printed, run in DOSBox |
| `fbc-golden.txt` | what **this application** prints, built with FreeBASIC |

`cargo test -p etb-testkit --test numbers` builds `SOSANH.BAS`, runs it, and
holds it to `fbc-golden.txt` — and to the list of differences from Turbo
Basic, which is written out in that test with a reason for each. A new
difference fails; so does an old one silently disappearing.

## Why an oracle at all

The 1987 Owner's Handbook contradicts itself about storage
(`docs/verification.md`, V1) and says nothing about how many digits `PRINT`
shows. Both decide what a user's program prints, and neither can be argued
from documentation. The compiler is still runnable, so it can be asked.

## Running it

**Turbo Basic is not in this repository, and must never be put in it.** It is
Borland's; a licensed copy belongs to whoever licensed it, and so does the
handbook. `cargo xtask check-hygiene` fails if a file named `TB.EXE`,
`TBCONFIG.TB`, `tb-handbook.txt` or `*.TBC` appears anywhere in the working
tree — including under `target/`, because a copy made "just to test something"
is how it would otherwise happen.

So the compiler stays in a directory of its own, outside the repository. Copy
the probe *to it*, never the other way round:

```sh
tb=~/turbo-basic              # wherever your licensed copy lives
mkdir -p "$tb/probe" && cp tools/tb-oracle/SOSANH.BAS "$tb/probe/"
cp "$tb"/TB.EXE "$tb"/TBCONFIG.TB "$tb/probe/"
cd "$tb/probe"
```

Then, with dosbox-staging:

```ini
[autoexec]
mount c .
c:
autotype -w 4 -p 0.35 f10 right right enter enter , , , , , , , , , ,
TB SOSANH.BAS
exit
```

```sh
SDL_AUDIODRIVER=dummy xvfb-run -a dosbox -conf dosbox.conf -exit
```

and bring `OUT.TXT` back to `tools/tb-oracle/tb-golden.txt` — the measurement
is ours to keep, the compiler is not.

`f10` opens the menu bar, two `right`s reach Run, the `enter`s run it, and the
commas are pauses while it compiles. AUTOTYPE cannot send `alt-r` — it looks
each argument up as one button name — which is why the menu is walked. The
probes write to a file rather than the screen: reading a DOSBox window is
harder than reading a file.

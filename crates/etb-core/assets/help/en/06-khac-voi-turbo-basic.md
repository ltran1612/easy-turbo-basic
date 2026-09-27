# How it differs from Turbo Basic

The translated program does what your program did. A few things about the
machine it runs on have changed, and show.

## Speed

Today's computers are hundreds of times faster. A calculation that took a
minute takes a moment — and a delay made with an empty `FOR` loop now takes no
time at all. `DELAY` still waits as long as it says.

## Vietnamese text

DOS had no standard way to write Vietnamese: each program or font package used
its own (VNI, TCVN3/ABC, VISCII). Text written that way appears on screen as
other symbols, because today's Windows does not have those fonts. Text written
without accents — as most programs of the time did — appears as it always did.

## Printing

There is no printer port. What the program sends to `LPT1` or `PRN` is saved
beside the program as `MAY-IN-LPT1.TXT` (see *Saving and running*).

## Memory and the screen

`PEEK`, `POKE` and `DEF SEG` reached into the memory of a DOS computer. That
memory no longer exists, so these cannot do what they did. If a program that does this behaves differently, this is the likely
reason.

## Numbers

The arithmetic agrees with Turbo Basic's. That was checked by running the same
program through the original Turbo Basic compiler and comparing every value:
59 of them, 45 identical. What still differs is how a number is *shown*, not
what it is:

- **Fewer digits after the point.** Turbo Basic printed every digit it held,
  including the last few that carry no real precision. Where it printed
  `.3333333432674408`, this prints `.3333333`. It is the same number.
- **Very large and very small numbers are written differently.** Turbo Basic
  wrote `1E-002` for 0.01; this writes `.01`. Above ten thousand million
  million, Turbo Basic wrote `1E+020` and this writes `1D+20`.
- **`PRINT USING` differs in a few rare cases:** when the digit being dropped
  is exactly half (7.005 with `###.##`), where the `+` sign sits, the `^^^^`
  exponent form, and the `%` shown when a number does not fit its picture.

If a printed table looks different from an old printout, it is almost
certainly one of these, and not a wrong calculation.

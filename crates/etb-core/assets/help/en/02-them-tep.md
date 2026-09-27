# Adding files

## One program, one file

Most Turbo Basic programs are one `.BAS` file. Add it with **Add files…** and
it is the program.

## Programs made of several files

If your program pulls other files in with `$INCLUDE`, add those files to the
list too, **after** the program itself:

- The **first file** in the list is the program. Use **Up** and **Down** to
  change which one it is.
- The others are the files it includes.

An included file that is not in the list is still looked for in the folders of
the files that are, the way Turbo Basic looked for it. Adding it to the list
is simply the surest way.

## Names with or without capitals

DOS did not care whether a name was written `CONST.BAS` or `const.bas`, and
neither does this application: `$INCLUDE "const"` finds `CONST.BAS`.

## A file that is not a program

Some files look like programs by their name but are not text: a program saved
by GW-BASIC in its compact form (without `,A`), a QuickBASIC binary save, a
document or a data file. The application recognises these before building and
says what the file is, and how to get a text version when there is one.

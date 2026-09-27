# Common problems

## An error on a line

The message names your file and the line, and shows the line as you wrote it.
Open the file in Notepad, go to that line, and correct it.

The compiler stops at the **first** error it finds. After correcting one, build
again: there may be another further down.

## Things that cannot be translated

A few things in Turbo Basic only ever worked on a DOS computer, and are
reported before anything is built:

- **CALL INTERRUPT**, **REG** — calling DOS or the BIOS directly.
- **CALL ABSOLUTE**, **INLINE**, **$INLINE** — machine code or assembly
  language for DOS.
- **ENDMEM**, **ERADR** — questions about DOS memory.

These parts of the program have to be rewritten with ordinary BASIC. The
message says which line each one is on.

Some DOS-only settings are simply left out, with a note: `$STACK`,
`$SEGMENT`, `$SOUND`, `$COM`, `$EVENT` and `MEMSET` have nothing to do on
Windows.

## “This is a problem in Easy Turbo Basic”

If a message says the problem is in Easy Turbo Basic rather than in your
program, it is not something you did. Use **Copy** in the details and send
them to whoever gave you this application.

## The compiler is missing or damaged

See *Security warnings*: antivirus software sometimes removes part of it by
mistake.

# Getting started

This application turns your Turbo Basic programs into programs that run on
today's Windows, without a DOS box and without a command-line window.

## Four steps

1. Click **New program** and give it a name, for example *Pile calculation*.
2. Click **Add files…** and choose your `.BAS` file. If it uses `$INCLUDE`,
   add the included files too (see *Adding files*).
3. Click the green **Compile program** button.
4. Click **Save the program…** and choose where to keep it.

If anything is wrong, the messages appear in the panel below, on your file and
your line. See *Saving and running* for how to run the program you saved.

## Nothing to try it on yet?

A few examples come with the application, in the **examples** folder beside
where it was installed. Open **DOC-TRUOC.txt** in there to see what each one
covers — from INPUT and PRINT USING through to functions, files, the printer
and graphics.

Click **Add files…** and pick `examples\01-CO-BAN.BAS` to try one.

## How it works

No compiler of today reads Turbo Basic as it was written. So the application
first **translates** your program into the BASIC that FreeBASIC understands — FreeBASIC is
the compiler it brings with it — and then builds that. The translation keeps
every line where it was, so a message about line 155 is about line 155 of your
file.

## The application never changes your files

Your files are only ever **read**. The translation is written into the
application's own working folder, and only that copy is built. Your originals
are untouched, even when the build fails.

That is also why this application has **no editor**. To change your program,
open the file in Notepad or whichever editor you already use.

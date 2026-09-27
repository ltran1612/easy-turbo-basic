# Saving and running

## Saving

After a successful build, click **Save the program…** and choose a folder and a
name. The saved program is an ordinary Windows program: double-click it to run
it. It does not need this application to run.

## The window

The program opens in its own text window, with the colours your program
chooses — as it looked in DOS. A program that draws (`SCREEN`, `LINE`,
`CIRCLE`) opens a second window for the drawing.

When the program ends, the window stays open until you press a key, so the
results can be read. Nothing is printed over what the program left on the
screen, so a graph stays as it was drawn. The option *Wait for a key before the
window closes* turns the waiting off.

## Where the program's files go

Files your program opens by name — `OPEN "KQ.TXT" FOR OUTPUT AS 1` — are made
**beside the saved program**, in the same folder, as they were beside the
program in its DOS folder.

## Printing

Today's computers have no printer port. When your program prints to `LPT1`,
`LPT2`, `LPT3` or `PRN` — by name, or through a variable, as programs of this
age usually did — what it prints is saved beside the program as a text file:
`MAY-IN-LPT1.TXT`. Open it in Notepad to read it or print it.

## Running it from somewhere else

The saved program can be copied to another computer, a USB stick or an email,
and it runs there the same way.

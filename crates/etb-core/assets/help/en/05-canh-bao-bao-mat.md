# Security warnings and antivirus

## “Windows protected your PC”

The first time you run the installer — or, on another computer, a program you
built and sent there by email or a download link — Windows may show a blue
dialog saying *Windows protected your PC*. This does **not** mean there is a
virus. It means few people have run this program yet, so Windows does not
recognise it.

To continue:

1. Click the small **More info** link.
2. Click **Run anyway**.

## Antivirus removing part of the compiler

The compiler is made of many small programs, among them `qb64pe.exe`,
`clang.exe` and `mingw32-make.exe`. Some antivirus products occasionally
mistake these for something dangerous and delete them — and the same can happen
to a program you have built and saved. This is a long-known false positive.

If the application reports *The bundled compiler is missing or damaged*, this is
the likely cause. To restore it on Windows:

1. Open **Windows Security**.
2. Go to **Virus & threat protection** → **Protection history**.
3. Find the entry mentioning Easy Turbo Basic and choose **Restore**.
4. Start the application again.

If it cannot be restored, reinstall the application.

> **Note:** this application will never add an exclusion to your antivirus for
> you. That has to be your decision.

## What this application does with your files

It only ever **reads** the files you choose. Everything the build produces goes
into the application's own working folder.

The program you build is different: it is your program, and it can write files
wherever its code says to. Run only programs you trust, as with any other
program.

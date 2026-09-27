' Written for Easy Turbo Basic: runtime support, included at the end of
' every program.
'
' Everything here is named ETB_ and has an explicit type, so a DEFINT or
' DEFSTR in the program cannot change what it means. The translator fills in
' the placeholders in double braces before this is written out.

{{DECLARATIONS}}
' And what this file itself defines, for the same reason: FreeBASIC reads
' this from top to bottom and ETB_CLOSED calls ETB_PRINTFILE below it.
DECLARE FUNCTION ETB_NZ# (BYVAL ETB_X AS DOUBLE)
DECLARE FUNCTION ETB_TIMER#
DECLARE FUNCTION ETB_CEIL# (BYVAL ETB_X AS DOUBLE)
DECLARE FUNCTION ETB_BIN$ (BYVAL ETB_N AS DOUBLE)
DECLARE FUNCTION ETB_DEV$ (ETB_NAME AS STRING, ETB_N AS LONG)
DECLARE SUB ETB_CLOSED (ETB_N AS LONG)
DECLARE SUB ETB_PRINTFILE (ETB_PATH AS STRING)
DECLARE SUB ETB_FINISH ()

DIM SHARED ETB_SPOOLS AS STRING
DIM SHARED ETB_SPOOLCOUNT AS LONG

' The name to open instead of the one the program asked for.
'
' A printer port becomes a text file beside the program, printed when it is
' closed. Everything else is opened as asked.
FUNCTION ETB_DEV$ (ETB_NAME AS STRING, ETB_N AS LONG)
    DIM ETB_U AS STRING
    DIM ETB_P AS STRING
    ETB_U = UCASE$(LTRIM$(RTRIM$(ETB_NAME)))
    IF RIGHT$(ETB_U, 1) = ":" THEN ETB_U = LEFT$(ETB_U, LEN(ETB_U) - 1)
    SELECT CASE ETB_U
        CASE "LPT1", "LPT2", "LPT3", "PRN"
            ETB_SPOOLCOUNT = ETB_SPOOLCOUNT + 1
            ETB_P = "{{SPOOL_STEM}}-" + ETB_U
            IF ETB_SPOOLCOUNT > 1 THEN ETB_P = ETB_P + "-" + LTRIM$(STR$(ETB_SPOOLCOUNT))
            ETB_P = ETB_P + ".TXT"
            ETB_SPOOLS = ETB_SPOOLS + "|" + LTRIM$(STR$(ETB_N)) + "=" + ETB_P
            ETB_DEV$ = ETB_P
        CASE ELSE
            ETB_DEV$ = ETB_NAME
    END SELECT
END FUNCTION

' Called after every CLOSE with the file number it closed, or -1 for all.
SUB ETB_CLOSED (ETB_N AS LONG)
    DIM ETB_REST AS STRING
    DIM ETB_ENTRY AS STRING
    DIM ETB_BAR AS LONG
    DIM ETB_EQ AS LONG
    ETB_REST = ETB_SPOOLS
    ETB_SPOOLS = ""
    DO WHILE LEN(ETB_REST) > 0
        ETB_BAR = INSTR(2, ETB_REST, "|")
        IF ETB_BAR = 0 THEN ETB_BAR = LEN(ETB_REST) + 1
        ETB_ENTRY = MID$(ETB_REST, 2, ETB_BAR - 2)
        ETB_REST = MID$(ETB_REST, ETB_BAR)
        ETB_EQ = INSTR(ETB_ENTRY, "=")
        IF ETB_N = -1 OR VAL(LEFT$(ETB_ENTRY, ETB_EQ - 1)) = ETB_N THEN
            ETB_PRINTFILE MID$(ETB_ENTRY, ETB_EQ + 1)
        ELSE
            ETB_SPOOLS = ETB_SPOOLS + "|" + ETB_ENTRY
        END IF
    LOOP
END SUB

' Send a finished printer file to the printer. The file itself is always kept.
SUB ETB_PRINTFILE (ETB_PATH AS STRING)
    {{PRINT_FILE}}
END SUB

' Turbo Basic's own functions that FreeBASIC does not have.
'
' CEIL is the whole number at or above its argument, which is what Turbo
' Basic's CEIL returns; INT rounds down, so the negative of INT of the
' negative rounds up.
FUNCTION ETB_CEIL# (BYVAL ETB_X AS DOUBLE)
    ETB_CEIL# = -INT(-ETB_X)
END FUNCTION

' BIN$ writes a whole number in base two, with no leading zeros, as Turbo
' Basic's does: sixteen bits, two's complement for a negative number, over the
' range -32768 to 65535 (Owner's Handbook, p.134). FreeBASIC has BIN, which is
' neither that name nor available in this dialect.
'
' BYVAL, because without it a caller writing BIN$(I%) -- which is the
' handbook's own example -- cannot pass its INTEGER to a LONG parameter.
FUNCTION ETB_BIN$ (BYVAL ETB_N AS DOUBLE)
    DIM ETB_V AS LONG
    DIM ETB_OUT AS STRING
    ETB_V = CLNG(ETB_N) AND 65535
    IF ETB_V = 0 THEN
        ETB_BIN$ = "0"
        EXIT FUNCTION
    END IF
    DO WHILE ETB_V > 0
        ETB_OUT = LTRIM$(STR$(ETB_V AND 1)) + ETB_OUT
        ETB_V = ETB_V \ 2
    LOOP
    ETB_BIN$ = ETB_OUT
END FUNCTION

' The divisor of a `\\` or a MOD, checked for zero before the processor sees
' it.
'
' An integer division by zero is not an error on x86, it is a fault: the
' program is killed there and then, with no message, with whatever it had
' written still sitting in a buffer, and without the chance to write out what
' it had printed to the printer. Turbo Basic stopped the program with
' "Division by zero" and everything it had already written was on disk.
'
' So does this: it says so, closes the files properly, and stops.
FUNCTION ETB_NZ# (BYVAL ETB_X AS DOUBLE)
    IF CLNG(ETB_X) = 0 THEN
        PRINT
        PRINT "Chia cho 0 -- chuong trinh dung o day."
        PRINT "Division by zero -- the program stopped here."
        ETB_FINISH
        END
    END IF
    ETB_NZ# = ETB_X
END FUNCTION

' Logarithms to base 10 and base 2, as Turbo Basic's LOG10 and LOG2.
'
' Written as LOG(x)/LOG(10) these are not exact: LOG(1000)/LOG(10) is
' 2.9999999999999996, so INT() of it is 2 and a decade disappears off a
' logarithmic axis. Turbo Basic answered 3, because it evaluated in the 8087's
' 80 bits. The C library's own log10 and log2 are correctly rounded and give
' Turbo Basic's answers exactly, so they are what these call.
DECLARE FUNCTION ETB_LOG10# CDECL ALIAS "log10" (BYVAL ETB_X AS DOUBLE)
DECLARE FUNCTION ETB_LOG2# CDECL ALIAS "log2" (BYVAL ETB_X AS DOUBLE)

' Seconds since midnight, which is what Turbo Basic's TIMER returns
' (Owner's Handbook, p.336). FreeBASIC's TIMER counts from a different moment
' altogether -- around 1.79e9 today -- and a program that keeps it in an
' ordinary single-precision variable, as these programs do, loses everything
' below about two minutes. Elapsed times then come out as nonsense, and
' RANDOMIZE TIMER stops varying.
FUNCTION ETB_TIMER#
    DIM ETB_T AS STRING
    ETB_T = TIME$
    ETB_TIMER# = VAL(LEFT$(ETB_T, 2)) * 3600# + VAL(MID$(ETB_T, 4, 2)) * 60# _
        + VAL(MID$(ETB_T, 7, 2)) + (TIMER - INT(TIMER))
END FUNCTION

' The program is ending: close its files, print what it printed, and either
' leave the window up for the user to read or go.
SUB ETB_FINISH
    CLOSE
    ETB_CLOSED -1
    {{FINISH_EXIT}}
END SUB

{{TEST_SUPPORT}}

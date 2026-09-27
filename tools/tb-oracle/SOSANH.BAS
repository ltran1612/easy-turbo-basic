' Written for Easy Turbo Basic
' How Turbo Basic prints numbers, and what its arithmetic answers.
'
' Every one of these is a silent wrong answer if QB64 differs: the program
' still runs, the number is just not the one the user has been reading for
' thirty years. Run through the real TB 1.1 to make the goldens, then through
' this application, and compare. See docs/verification.md, V13.
OPEN "O", #1, "OUT.TXT"
A! = 1 / 3
B# = 1 / 3
PRINT #1, "N01"; A!
PRINT #1, "N02"; B#
PRINT #1, "N03"; 1000000!
PRINT #1, "N04"; 12345678!
PRINT #1, "N05"; 123456789#
PRINT #1, "N06"; .0001
PRINT #1, "N07"; .00001
PRINT #1, "N08"; 7
PRINT #1, "N09"; -7
PRINT #1, "N10"; 2 ^ .5
PRINT #1, "N11"; 1E20
PRINT #1, "N12"; 100!
PRINT #1, "N13"; 2.5
PRINT #1, "N14"; -2.5
PRINT #1, "N15"; 1 / 7
PRINT #1, "N16"; 22 / 7
PRINT #1, "U01"; USING "###.##"; 7
PRINT #1, "U02"; USING "###.##"; 7.005
PRINT #1, "U03"; USING "##.##"; -1.005
PRINT #1, "U04"; USING "#####"; 12345.6
PRINT #1, "U05"; USING "**#.#"; 3.14
PRINT #1, "U06"; USING "$$##.##"; 12.3
PRINT #1, "U07"; USING "+###.#"; 42
PRINT #1, "U08"; USING "###.#-"; -42
PRINT #1, "U09"; USING "#.###^^^^"; 1234.5
PRINT #1, "U10"; USING "\   \"; "abcdefg"
PRINT #1, "U11"; USING "!"; "abc"
PRINT #1, "U12"; USING "&"; "abc"
PRINT #1, "U13"; USING "## tui"; 5
PRINT #1, "U14"; USING "##.##"; 123.456
PRINT #1, "U15"; USING "###,.##"; 1234567.891
PRINT #1, "D01"; 7 \ 2
PRINT #1, "D02"; -7 \ 2
PRINT #1, "D03"; 7 MOD 2
PRINT #1, "D04"; -7 MOD 2
PRINT #1, "D05"; 7 MOD -2
PRINT #1, "R01"; CINT(.5)
PRINT #1, "R02"; CINT(1.5)
PRINT #1, "R03"; CINT(2.5)
PRINT #1, "R04"; CINT(-.5)
PRINT #1, "R05"; CINT(-2.5)
PRINT #1, "R06"; INT(-2.5)
PRINT #1, "R07"; FIX(-2.5)
PRINT #1, "R08"; CEIL(2.1)
PRINT #1, "R09"; CEIL(-2.1)
PRINT #1, "S01"; STR$(5)
PRINT #1, "S02"; STR$(-5)
PRINT #1, "S03"; STR$(1 / 3)
PRINT #1, "S04"; HEX$(255)
PRINT #1, "S05"; OCT$(8)
PRINT #1, "S06"; BIN$(5)
PRINT #1, "S07"; VAL("12abc")
PRINT #1, "S08"; VAL("&H1F")
PRINT #1, "T01"; LOG2(8)
PRINT #1, "T02"; LOG10(1000)
PRINT #1, "T03"; EXP2(10)
PRINT #1, "T04"; EXP10(3)
PRINT #1, "T05"; SQR(2)
PRINT #1, "T06"; ATN(1)
CLOSE #1
END

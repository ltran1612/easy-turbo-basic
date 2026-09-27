' Written for Easy Turbo Basic
' Vi du 4: ghi tep, doc lai, va in ra may in.
OPEN "SOLIEU.TXT" FOR OUTPUT AS #1
FOR I = 1 TO 5
  PRINT #1, I, I * I
NEXT I
CLOSE #1
TONG = 0
OPEN "SOLIEU.TXT" FOR INPUT AS #1
WHILE NOT EOF(1)
  INPUT #1, A, B
  TONG = TONG + B
WEND
CLOSE #1
PRINT "Tong cac binh phuong ="; TONG
MI$ = "LPT1"
OPEN MI$ FOR OUTPUT AS #2
PRINT #2, "Tong cac binh phuong ="; TONG
CLOSE #2
PRINT "Da gui ket qua ra may in (tep MAY-IN-LPT1.TXT)"
END

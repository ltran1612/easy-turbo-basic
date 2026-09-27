' Written for Easy Turbo Basic
' Nhung dieu FreeBASIC khong co, va Turbo Basic thi co.
DEFINT A-Z
tong = 0
CALL Cong(3, tong)
CALL Cong(4, tong)
PRINT "TONG"; tong
CALL Demso
CALL Demso
n = 0
DO
  n = n + 1
  WHILE 1
    EXIT LOOP
  WEND
  PRINT "VONG"; n
LOOP UNTIL n >= 2
K! = 0.7
IF K! = 0.7 THEN PRINT "SS DUNG" ELSE PRINT "SS SAI"
J! = 2.1
PRINT "CEIL"; CEIL(J!)
I% = -5
PRINT "BIN "; BIN$(I%)
PRINT "MANG"; FNCong
END

SUB Cong(them, ket)
  SHARED KHONGDUNG
  ket = ket + them
END SUB

SUB Demso STATIC
100 dem = dem + 1
  PRINT "DEM"; dem
END SUB

DEF FNCong
  LOCAL i, t
  FOR i = 1 TO 3
    t = t + v(i)
  NEXT i
  FNCong = t
END DEF

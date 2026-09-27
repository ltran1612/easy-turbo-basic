' Written for Easy Turbo Basic
' Vi du 5: ve do thi bang do hoa (SCREEN 9: 640 x 350 diem, 16 mau).
SCREEN 9
CLS
LINE (40, 175)-(620, 175), 7
LINE (40, 20)-(40, 330), 7
FOR X = 0 TO 580
  Y = 175 - 140 * SIN(X / 45) * EXP(-X / 400)
  IF X = 0 THEN PSET (40 + X, Y), 14 ELSE LINE -(40 + X, Y), 14
NEXT X
CIRCLE (320, 175), 60, 12
LOCATE 2, 10: PRINT "Dao dong tat dan: y = sin(x) * e^(-x)"
LOCATE 24, 10: PRINT "Nhan phim bat ky de ket thuc";
A$ = INPUT$(1)
END

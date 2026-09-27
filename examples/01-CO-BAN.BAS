' Written for Easy Turbo Basic
' Vi du 1: nhap so lieu, tinh toan, in ket qua co dinh dang.
' Kieu viet cua thoi DOS: so dong, ? thay cho PRINT, IF ... THEN so dong.
10 CLS
20 PRINT "TINH DIEN TICH VA CHU VI HINH CHU NHAT"
30 ?
40 INPUT "Chieu dai (m) "; A
50 INPUT "Chieu rong (m) "; B
60 IF A <= 0 OR B <= 0 THEN PRINT "Kich thuoc phai lon hon 0": GOTO 40
70 S = A * B: P = 2 * (A + B)
80 ?
90 PRINT USING "Dien tich : ####.## m2"; S
100 PRINT USING "Chu vi    : ####.## m"; P
110 END

' Written for Easy Turbo Basic
' Vi du 2: vong lap, mang va SELECT CASE.
DIM T(1:12)
FOR I = 1 TO 12
  T(I) = 20 + I
NEXT I
TONG = 0: CAO = T(1)
FOR I = 1 TO 12
  TONG = TONG + T(I)
  IF T(I) > CAO THEN CAO = T(I)
NEXT I
PRINT USING "Trung binh: ##.##"; TONG / 12
PRINT "Cao nhat  :"; CAO
N = 0
DO WHILE N < 3
  INCR N
  SELECT CASE N
    CASE 1: PRINT "Mot"
    CASE 2: PRINT "Hai"
    CASE ELSE: PRINT "Nhieu"
  END SELECT
LOOP
END

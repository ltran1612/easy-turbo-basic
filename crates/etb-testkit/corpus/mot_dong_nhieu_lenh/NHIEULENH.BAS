' Written for Easy Turbo Basic
' Nhieu lenh tren cung mot dong, kieu viet thuong gap nhung nam 1990.
%DUNG = -1 : %SAI = 0 : %SOMAX = 50
DEF FNBP(X) = X * X : DEF FNGAP(Y) = Y * 2
DIM A(%SOMAX)
FOR I = 1 TO 3 : A(I) = FNBP(I) : NEXT I
PRINT "CONST"; %DUNG; %SAI; %SOMAX
PRINT "FN"; FNBP(7); FNGAP(4)
PRINT "MANG"; A(1); A(2); A(3)
END

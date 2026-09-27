' Written for Easy Turbo Basic
' Vi du 3: ham tu dinh nghia (DEF FN), thu tuc (SUB) va hang so co ten.
%SOLAN = 3
DEF FNVuong(x) = x * x
DEF FNGiaiThua#(n%)
  LOCAL i%, t#
  t# = 1
  FOR i% = 2 TO n%: t# = t# * i%: NEXT i%
  FNGiaiThua# = t#
END DEF
PRINT "5 binh phuong ="; FNVuong(5)
PRINT "10! ="; FNGiaiThua#(10)
FOR k = 1 TO %SOLAN
  CALL Dem
NEXT k
END
SUB Dem
  INCR lan
  PRINT "Lan goi thu"; lan
END SUB

' Written for Easy Turbo Basic
' The Owner's Handbook's own examples of functions, and what they promise.
DEF FNCtoF(degreesC) = (1.8 * degreesC) + 32
DEF FNFtoC(degreesF) = (degreesF - 32) * .555555
100 DEF FNFactorial#(x%)
  LOCAL i%, total#
  IF x% < 0 OR x% > 170 THEN FNFactorial# = -1 : EXIT DEF
  total# = 1
  FOR i% = x% TO 2 STEP -1
    total# = total# * i%
  NEXT i%
  FNFactorial# = total#
END DEF
DEF FNArea (x,y) = x * y
DEF FNDummy(a,b,c)
  a = a + b + c
  FNDummy = a
END DEF
DEF FNAddScaled(v)
  hits = hits + 1
  FNAddScaled = v * scale
END DEF
scale = 3
x = 56
PRINT "C2F"; FNCtoF(100)
PRINT "F2C"; FNFtoC (212)
PRINT "AREA"; FNArea(2,3)
PRINT "XKEPT"; x
PRINT "FACT"; FNFactorial#(10)
PRINT "NEG"; FNFactorial#(-2)
p = 1 : q = 2 : r = 3
t = FNDummy(p, q, r)
PRINT "BYVAL"; t; p
PRINT "SHARED"; FNAddScaled(2); FNAddScaled(5); hits
END

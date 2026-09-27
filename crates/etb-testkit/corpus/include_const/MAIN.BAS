' Written for Easy Turbo Basic
$INCLUDE "HANGSO"
DIM a(1:5)
FOR i = 1 TO 5: a(i) = i * %HESO: NEXT i
PRINT "BOUNDS"; LBOUND(a(1)); UBOUND(a(1))
CALL Tong(a(), 5, s)
PRINT "SUM"; s
CALL Dem: CALL Dem: CALL Dem
$IF %DEBUG
PRINT "DEBUG ON"
$ELSE
PRINT "DEBUG OFF"
$ENDIF
type = 7: PRINT "TYPE"; type
END
SUB Tong(v(1), n, total)
  total = 0
  FOR k = 1 TO n: total = total + v(k): NEXT k
END SUB
SUB Dem
  lan = lan + 1
  PRINT "CALL"; lan
END SUB

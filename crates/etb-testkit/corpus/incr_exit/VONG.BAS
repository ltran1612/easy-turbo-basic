' Written for Easy Turbo Basic
n = 0
DO
  INCR n
  IF n >= 3 THEN EXIT LOOP
LOOP
PRINT "N"; n
k = 10: DECR k, 4
PRINT "K"; k
IF k > 0 THEN
  PRINT "IN"
  IF k = 6 THEN EXIT IF
  PRINT "NOT HERE"
END IF
PRINT "AFTER"
PRINT "CEIL"; CEIL(2.1)
PRINT USING "LOG #.###"; LOG10(1000)
PRINT "EXP"; EXP10(2)
END

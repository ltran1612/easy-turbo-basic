' Written for Easy Turbo Basic
' Storage of undeclared variables, as Turbo Basic itself does it.
'
' The 1987 Owner's Handbook contradicts itself: chapter 4 says procedures
' default to STATIC and functions to shared, while the SUB reference entry
' says otherwise. The real TB 1.1 compiler was asked (docs/verification.md,
' V1) and these are the numbers it produced. They are what this program must
' still print, because a variable that silently forgets its value between
' calls is a wrong answer, not a crash.
X = 5
N = 0
CALL P
CALL P
PRINT "MAIN"; X; N
Y = 100
Z = FNF(1)
PRINT "FN1"; Z
Z = FNF(1)
PRINT "FN2"; Z
PRINT "MAINY"; Y
END

SUB P
  X = X + 1
  N = N + 1
  PRINT "INP"; X; N
END SUB

DEF FNF(A)
  Y = Y + A
  FNF = Y
END DEF

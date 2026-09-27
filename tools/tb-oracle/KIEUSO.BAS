' Written for Easy Turbo Basic
' Two questions the first probe raised: what type is an unsuffixed constant,
' and when does Turbo Basic switch to exponential notation?
OPEN "O", #1, "OUT.TXT"
X = 1 / 3
PRINT #1, "C01"; 1 / 3
PRINT #1, "C02"; X
PRINT #1, "C03"; 1.0 / 3.0
PRINT #1, "C04"; 2 ^ .5
PRINT #1, "C05"; 3.14159 * 2
PRINT #1, "C06"; 1.1 + 2.2
Y! = 1.1 + 2.2
PRINT #1, "C07"; Y!
Z# = 1.1 + 2.2
PRINT #1, "C08"; Z#
PRINT #1, "E01"; .001
PRINT #1, "E02"; .0001
PRINT #1, "E03"; .01
PRINT #1, "E04"; 9999999#
PRINT #1, "E05"; 10000000#
PRINT #1, "E06"; 1E15
PRINT #1, "E07"; 1E16
PRINT #1, "E08"; 123456.7
PRINT #1, "E09"; 1000000!
PRINT #1, "E10"; 1000000#
PRINT #1, "E11"; 32767
PRINT #1, "E12"; 32768
PRINT #1, "E13"; 2000000000
PRINT #1, "E14"; -.0001
PRINT #1, "I01"; 5 / 2
PRINT #1, "I02"; 4 / 2
CLOSE #1
END

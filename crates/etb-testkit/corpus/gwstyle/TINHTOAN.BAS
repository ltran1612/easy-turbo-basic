' Written for Easy Turbo Basic

color 14,1
2 cls
input "Chieu dai (m) ";a
input "Chieu rong (m) ";b
?
s=a*b
? "Dien tich =";
print using "###.##";s;
? " m2"
input "Luu ket qua (c/k) ";k$
if k$ ="k" then 20
input "Ten file";f$
open f$ for output as 4
print# 4,"Dien tich =";
print# 4,using "###.##";s
close #4
20 p$="lpt1"
open p$ for output as 3
print# 3,"KET QUA:";s
close
input "Tinh lai (c/k) ";r$
if r$ ="c" then 2
end


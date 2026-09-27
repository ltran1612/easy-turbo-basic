# Khác gì so với Turbo Basic

Chương trình sau khi chuyển đổi làm đúng những gì chương trình của bạn đã làm.
Chỉ có một vài điều về chiếc máy nó chạy trên đó đã thay đổi, và có thể nhận
thấy.

## Tốc độ

Máy tính ngày nay nhanh hơn hàng trăm lần. Một phép tính trước kia mất một phút
nay chỉ mất một thoáng — và một khoảng chờ tạo bằng vòng lặp `FOR` rỗng giờ gần
như không mất thời gian. `DELAY` thì vẫn chờ đúng bằng thời gian đã ghi.

## Chữ tiếng Việt

DOS không có cách chung để viết tiếng Việt: mỗi chương trình hay bộ phông chữ
dùng một cách riêng (VNI, TCVN3/ABC, VISCII). Chữ viết theo những cách đó sẽ
hiện trên màn hình thành các ký hiệu khác, vì Windows ngày nay không có các
phông chữ ấy. Chữ viết không dấu — như phần lớn chương trình thời đó — vẫn hiện
như xưa.

## In ấn

Không còn cổng máy in. Những gì chương trình gửi tới `LPT1` hoặc `PRN` được lưu
cạnh chương trình thành tệp `MAY-IN-LPT1.TXT` (xem mục *Lưu và chạy chương
trình*).

## Bộ nhớ và màn hình

`PEEK`, `POKE` và `DEF SEG` tác động thẳng vào bộ nhớ của máy DOS. Vùng nhớ đó không
còn tồn tại, nên những câu lệnh này không làm được việc chúng từng làm. Nếu một chương trình có làm việc này mà chạy khác đi, thì nhiều khả
năng là vì lý do đó.

## Số liệu

Kết quả tính toán giống Turbo Basic. Điều này đã được kiểm chứng bằng cách chạy
cùng một chương trình qua chính trình biên dịch Turbo Basic cũ rồi so từng con
số: 59 phép thử, 45 trùng khít hoàn toàn. Những chỗ còn khác đều nằm ở *cách
hiển thị*, không phải ở giá trị:

- **Số lẻ in ra ít chữ số hơn.** Turbo Basic in ra mọi chữ số nó lưu trong máy,
  kể cả những chữ số cuối vốn không còn ý nghĩa. Ví dụ Turbo Basic in
  `.3333333432674408`, ở đây in `.3333333`. Giá trị vẫn là một.
- **Số rất lớn hoặc rất nhỏ viết khác.** Turbo Basic viết `1E-002` cho 0,01;
  ở đây viết `.01`. Số từ mười triệu tỷ trở lên, Turbo Basic viết `1E+020`,
  ở đây là `1D+20`.
- **`PRINT USING` lệch ở vài trường hợp hiếm:** khi chữ số bỏ đi đúng bằng nửa
  đơn vị cuối (ví dụ 7,005 với khuôn `###.##`), vị trí của dấu `+`, khuôn số mũ
  `^^^^`, và khi số không vừa khuôn (dấu `%` ở đầu).

Nếu một bảng số in ra trông khác bản in ngày xưa, gần như chắc chắn là vì một
trong những điều trên, chứ không phải vì tính sai.

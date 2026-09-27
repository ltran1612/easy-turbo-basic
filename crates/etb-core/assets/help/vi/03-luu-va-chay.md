# Lưu và chạy chương trình

## Lưu

Sau khi biên dịch thành công, bấm **Lưu chương trình…** rồi chọn thư mục và tên.
Chương trình đã lưu là một chương trình Windows bình thường: bấm đúp vào là
chạy. Nó không cần ứng dụng này để chạy.

## Cửa sổ

Chương trình mở trong cửa sổ riêng, rộng 80 ký tự và cao 25 dòng, với màu sắc
do chương trình của bạn chọn — giống như trên DOS. Đồ hoạ (`SCREEN`, `LINE`,
`CIRCLE`) hiện ngay trong cửa sổ đó.

Khi chương trình kết thúc, cửa sổ vẫn mở với dòng **Press any key to
continue** để bạn kịp đọc kết quả. Tuỳ chọn *Chờ nhấn phím trước khi đóng cửa
sổ* dùng để tắt việc này.

## Các tệp chương trình tạo ra nằm ở đâu

Các tệp mà chương trình mở theo tên — `OPEN "KQ.TXT" FOR OUTPUT AS 1` — được
tạo **ngay cạnh chương trình đã lưu**, trong cùng thư mục, giống như ngày trước
chúng nằm cạnh chương trình trong thư mục DOS.

## In ấn

Máy tính ngày nay không còn cổng máy in. Khi chương trình in ra `LPT1`, `LPT2`,
`LPT3` hoặc `PRN` — ghi thẳng tên, hoặc qua một biến như các chương trình thời
đó thường làm — nội dung in được lưu cạnh chương trình thành tệp văn bản
`MAY-IN-LPT1.TXT`. Mở tệp đó bằng Notepad để đọc hoặc in ra.

## Chạy ở nơi khác

Có thể chép chương trình đã lưu sang máy khác, sang USB hay gửi qua email, và nó
vẫn chạy như vậy.

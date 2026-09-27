# Bắt đầu

Ứng dụng này biến các chương trình Turbo Basic của bạn thành chương trình chạy
được trên Windows ngày nay, không cần cửa sổ DOS và không cần dòng lệnh.

## Bốn bước

1. Bấm **Tạo chương trình mới** và đặt tên cho nó, ví dụ *Tính cọc*.
2. Bấm **Thêm tệp…** rồi chọn tệp `.BAS` của bạn. Nếu chương trình dùng
   `$INCLUDE`, hãy thêm cả các tệp được đưa vào (xem mục *Thêm tệp*).
3. Bấm nút xanh **Biên dịch chương trình**.
4. Bấm **Lưu chương trình…** và chọn nơi lưu.

Nếu có lỗi, các thông báo sẽ hiện ở khung phía dưới, chỉ đúng tệp và đúng dòng
của bạn. Xem mục *Lưu và chạy chương trình* để biết cách chạy chương trình đã
lưu.

## Chưa có chương trình nào để thử?

Ứng dụng có sẵn vài ví dụ, nằm trong thư mục **examples** ngay cạnh nơi cài đặt.
Hãy mở tệp **DOC-TRUOC.txt** trong đó để biết từng ví dụ dạy gì — từ INPUT và
PRINT USING cho tới hàm, tệp, máy in và đồ hoạ.

Bấm **Thêm tệp…** rồi chọn `examples\01-CO-BAN.BAS` là thử được ngay.

## Ứng dụng làm việc thế nào

Không trình biên dịch nào ngày nay đọc được Turbo Basic đúng như nó được viết.
Vì vậy ứng dụng trước hết **chuyển đổi** chương trình của bạn sang dạng BASIC mà
FreeBASIC hiểu — FreeBASIC là trình biên dịch đi kèm ứng dụng — rồi mới biên dịch bản đó.
Khi chuyển đổi, mỗi dòng vẫn giữ nguyên vị trí, nên thông báo nói về dòng 155
chính là dòng 155 trong tệp của bạn.

## Ứng dụng không bao giờ sửa tệp của bạn

Các tệp của bạn chỉ được **đọc**. Bản chuyển đổi được ghi vào thư mục làm việc
riêng của ứng dụng, và chỉ bản sao đó được biên dịch. Tệp gốc của bạn không hề
thay đổi, kể cả khi biên dịch thất bại.

Vì vậy ứng dụng này **không có chức năng sửa chương trình**. Muốn sửa, bạn hãy
mở tệp bằng Notepad hoặc trình soạn thảo quen thuộc.

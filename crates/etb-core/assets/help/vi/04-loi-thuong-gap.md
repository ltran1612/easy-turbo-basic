# Lỗi thường gặp

## Lỗi ở một dòng

Thông báo cho biết tên tệp và số dòng, kèm theo dòng đó đúng như bạn đã viết.
Hãy mở tệp bằng Notepad, tìm đến dòng đó và sửa lại.

Trình biên dịch dừng ở lỗi **đầu tiên** mà nó gặp. Sửa xong một lỗi, hãy biên
dịch lại: có thể còn lỗi khác ở phía dưới.

## Những phần không chuyển đổi được

Một vài thứ trong Turbo Basic chỉ chạy được trên máy DOS, và được báo ngay
trước khi biên dịch:

- **CALL INTERRUPT**, **REG** — gọi thẳng vào DOS hoặc BIOS.
- **CALL ABSOLUTE**, **INLINE**, **$INLINE** — mã máy hoặc hợp ngữ viết cho DOS.
- **ENDMEM**, **ERADR** — hỏi về bộ nhớ của DOS.

Những phần này của chương trình phải được viết lại bằng các lệnh BASIC thông
thường. Thông báo cho biết từng chỗ nằm ở dòng nào.

Một số thiết lập chỉ dành cho DOS thì được bỏ qua, kèm một ghi chú: `$STACK`,
`$SEGMENT`, `$SOUND`, `$COM`, `$EVENT` và `MEMSET` không có việc gì để làm trên
Windows.

## “Đây là lỗi của Easy Turbo Basic”

Nếu thông báo nói lỗi nằm ở Easy Turbo Basic chứ không phải ở chương trình của
bạn, thì đó không phải do bạn làm sai. Hãy bấm **Sao chép** ở phần chi tiết và
gửi cho người đã đưa bạn ứng dụng này.

## Trình biên dịch bị thiếu hoặc hỏng

Xem mục *Cảnh báo bảo mật*: phần mềm diệt virus đôi khi xoá nhầm một phần của
trình biên dịch.

# Cảnh báo bảo mật và phần mềm diệt virus

## “Windows protected your PC”

Lần đầu chạy bộ cài đặt — hoặc, trên một máy khác, chạy chương trình bạn đã
tạo rồi gửi sang bằng email hay đường dẫn tải về — Windows có thể hiện một hộp
thoại màu xanh ghi *Windows protected your PC*. Điều này **không** có nghĩa là
có virus. Nó chỉ có nghĩa là chưa nhiều người chạy chương trình này, nên
Windows chưa nhận ra nó.

Để tiếp tục:

1. Bấm vào dòng chữ nhỏ **More info**.
2. Bấm **Run anyway**.

## Phần mềm diệt virus xoá một phần trình biên dịch

Trình biên dịch gồm nhiều chương trình nhỏ, trong đó có `fbc.exe`,
`gcc.exe` và `ld.exe`. Đôi khi một số phần mềm diệt virus nhầm
chúng là thứ nguy hiểm và xoá đi — và điều tương tự cũng có thể xảy ra với
chương trình bạn đã tạo và lưu. Đây là một lỗi nhận nhầm đã được biết từ lâu.

Nếu ứng dụng báo *Trình biên dịch đi kèm bị thiếu hoặc hỏng*, thì nhiều khả
năng là vì lý do này. Để khôi phục trên Windows:

1. Mở **Windows Security**.
2. Vào **Virus & threat protection** → **Protection history**.
3. Tìm mục liên quan đến Easy Turbo Basic và chọn **Restore** (Khôi phục).
4. Mở lại ứng dụng.

Nếu không khôi phục được, hãy cài đặt lại ứng dụng.

> **Lưu ý:** ứng dụng này sẽ không bao giờ tự thêm ngoại lệ vào phần mềm diệt
> virus của bạn. Việc đó phải do chính bạn quyết định.

## Ứng dụng làm gì với các tệp của bạn

Ứng dụng chỉ **đọc** các tệp mà bạn chọn. Mọi thứ sinh ra khi biên dịch đều nằm
trong thư mục làm việc riêng của ứng dụng.

Chương trình bạn tạo ra thì khác: đó là chương trình của bạn, và nó có thể ghi
tệp vào bất cứ đâu mà mã của nó yêu cầu. Chỉ chạy những chương trình bạn tin
tưởng, như với mọi chương trình khác.

# Thêm tệp

## Một chương trình, một tệp

Phần lớn chương trình Turbo Basic chỉ có một tệp `.BAS`. Hãy thêm nó bằng nút
**Thêm tệp…**, và đó chính là chương trình.

## Chương trình gồm nhiều tệp

Nếu chương trình của bạn đưa các tệp khác vào bằng `$INCLUDE`, hãy thêm cả các
tệp đó vào danh sách, **sau** tệp chương trình:

- **Tệp đầu tiên** trong danh sách là chương trình. Dùng **Lên** và **Xuống**
  để đổi tệp nào đứng đầu.
- Các tệp còn lại là những tệp được chương trình đưa vào.

Một tệp được đưa vào mà không có trong danh sách vẫn được tìm trong các thư mục
chứa những tệp có trong danh sách, giống cách Turbo Basic đã tìm. Thêm nó vào
danh sách chỉ là cách chắc chắn nhất.

## Tên viết hoa hay viết thường

DOS không phân biệt `CONST.BAS` với `const.bas`, và ứng dụng này cũng vậy:
`$INCLUDE "const"` vẫn tìm thấy `CONST.BAS`.

## Tệp không phải chương trình

Có những tệp nhìn tên thì giống chương trình nhưng không phải văn bản: chương
trình GW-BASIC được lưu ở dạng thu gọn (không có `,A`), chương trình QuickBASIC
lưu ở dạng nhị phân, một tài liệu, hay một tệp dữ liệu. Ứng dụng nhận ra những
tệp này trước khi biên dịch, cho biết tệp đó là gì, và cách lấy bản văn bản nếu
có cách.

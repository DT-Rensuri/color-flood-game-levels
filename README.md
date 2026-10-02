# Level content repository

Thư mục này là catalog dữ liệu màn chơi được phục vụ qua GitHub. App tải
`manifest.json`, sau đó tải từng file JSON theo `url`, kiểm tra `size` và
`sha256`, rồi lưu cache để chơi offline.

## Cấu trúc

```text
content/
  manifest.json
assets/levels/
  level_1.json
  ...
```

Manifest hiện trỏ tới `assets/levels` trong cùng repository để dữ liệu không bị
nhân đôi. Trước khi publish, thay `OWNER/REPOSITORY` trong
`content/manifest.json` bằng GitHub owner và repository thật.

## Chạy với GitHub

Truyền URL raw của manifest khi build hoặc run:

```text
LEVEL_MANIFEST_URL=https://raw.githubusercontent.com/OWNER/REPOSITORY/main/content/manifest.json
```

Với Flutter, truyền giá trị này bằng `--dart-define=LEVEL_MANIFEST_URL=...`.
Không commit token hoặc URL cần xác thực; repository content nên public để
GitHub Raw CDN phục vụ được trên mobile.

## Thêm hoặc sửa màn chơi

1. Thêm/sửa JSON trong `assets/levels` theo schema hiện tại.
2. Tính lại SHA-256 và byte size của file sau khi lưu.
3. Thêm/cập nhật entry trong `content/manifest.json`.
4. Tăng `catalogVersion` khi catalog thay đổi.
5. Push cả manifest và level JSON lên cùng branch/tag.

Nếu manifest hoặc một level lỗi, app giữ bundled level và cache hợp lệ gần
nhất thay vì làm hỏng toàn bộ catalog.
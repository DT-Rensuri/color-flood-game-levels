# Level content repository

Repo này là nguồn dữ liệu level cho `color_flood_game`. Tool Rust sẽ tạo level
và tự động rebuild `manifest.json`, nên không cần copy hash bằng tay.

Thư mục này là catalog dữ liệu màn chơi được phục vụ qua GitHub. App tải
`manifest.json`, sau đó tải từng file JSON theo `url`, kiểm tra `size` và
`sha256`, rồi lưu cache để chơi offline.

## Cấu trúc

```text
manifest.json
levels/
  level_1.json
  ...
```

Manifest trỏ tới `levels` trong repository GitHub này. Số entry trong manifest
luôn bằng số file `levels/level_<number>.json` thực tế.

## Chạy với GitHub

Truyền URL raw của manifest khi build hoặc run:

```text
LEVEL_MANIFEST_URL=https://raw.githubusercontent.com/OWNER/REPOSITORY/main/content/manifest.json
```

Với Flutter, truyền giá trị này bằng `--dart-define=LEVEL_MANIFEST_URL=...`.
Không commit token hoặc URL cần xác thực; repository content nên public để
GitHub Raw CDN phục vụ được trên mobile.

## Tạo level và manifest

Chạy từ thư mục repo này:

```text
cargo run -- --seed 1234
```

Tool tự tìm số level lớn nhất đang có rồi ghi level tiếp theo. Ví dụ đang có
`level_1.json` đến `level_10.json` thì lệnh trên ghi `levels/level_11.json`.
Sau đó tool validate toàn bộ level trong `levels/`,
rồi cập nhật `manifest.json` với `id`, `order`, URL GitHub, SHA-256 và byte
size. Có thể đổi vị trí bằng `--levels-dir`, `--manifest` hoặc đổi URL bằng
`--base-url`.

Dùng `--level N` nếu cần chủ động ghi một số level cụ thể; mặc định luôn là
level mới tiếp theo.

### Các option của generator

| Option | Mặc định | Mô tả |
|---|---:|---|
| `--level N` | level tiếp theo | Level bắt đầu được tạo. |
| `--count N` | `1` | Số level liên tiếp cần tạo. `N` phải lớn hơn 0. |
| `--output PATH` | tự tạo trong `levels/` | Tên file output khi tạo đúng một level. Không dùng cùng `--count N` khi `N > 1`. |
| `--width N` | `10` | Chiều rộng grid. |
| `--height N` | `8` | Chiều cao grid. |
| `--repeats N` | `6` | Số lần đặt template trong mỗi level. |
| `--target-color N` | `4` | Màu mục tiêu, từ `1` đến `4`. |
| `--seed N` | ngẫu nhiên | Seed để tái tạo kết quả. Khi dùng `--count`, mỗi level dùng seed kế tiếp. |
| `--templates-dir PATH` | `templates` | Thư mục template. |
| `--levels-dir PATH` | `levels` | Thư mục chứa các level. |
| `--manifest PATH` | `manifest.json` | File manifest cần cập nhật. |
| `--base-url URL` | URL GitHub mặc định | URL gốc dùng trong manifest. |
| `--catalog-version N` | `1` | Phiên bản catalog ghi vào manifest. |
| `--manifest-only` | tắt | Chỉ validate level hiện có và tạo lại manifest. |

Tạo một level:

```text
cargo run -- --level 51 --seed 5100
```

Tạo 10 level liên tiếp, bắt đầu từ level 51:

```text
cargo run -- --level 51 --count 10 --seed 5100
```

Lệnh trên tạo `level_51.json` đến `level_60.json`, sau đó cập nhật manifest
một lần. Nếu không truyền `--level`, generator sẽ bắt đầu từ level tiếp theo
đang có trong `levels/`.

Để chỉ validate các file đang có và tạo lại manifest, không sinh level mới:

```text
cargo run -- --manifest-only
```

## Schema level bắt buộc

Mỗi file phải có `level`, `maxMoves`, `targetColor`, `colors` và `grid`.

- Tên file phải là `level_<số>.json` và `level` phải trùng số đó.
- `maxMoves` và grid phải lớn hơn 0; grid phải chữ nhật.
- `colors` phải có key liên tục từ `1`, mã màu dạng `#RRGGBB` hoặc `#AARRGGBB`.
- `targetColor` và mọi ô trong grid phải nằm trong palette màu.

Nếu một file sai schema, tool dừng và không ghi manifest mới.

## Publish

1. Chạy tool để cập nhật manifest.
2. Kiểm tra `manifest.json` và các file trong `levels/`.
3. Push cả manifest và level JSON lên branch mà URL manifest sử dụng.
4. App Flutter tải manifest, kiểm tra SHA-256 rồi cache level để dùng offline.

Nếu manifest hoặc một level lỗi, app giữ bundled level và cache hợp lệ gần
nhất thay vì làm hỏng toàn bộ catalog.
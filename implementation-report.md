# Implementation Report

**Ngày:** 2026-09-27
**Phạm vi:** triển khai `spec.md` trong repository `code-mode-mcp-server`.

## Tóm tắt

Đã xây dựng gateway MCP bằng Rust với hai tool công khai (`tools_search`, `tools_execute`), WebUI quản trị, cấu hình YAML, policy lọc tool, registry/search, thực thi JavaScript trong Deno worker riêng và lưu audit metadata bằng SQLite. Các luồng build, CI, kiểm thử Rust/UI, smoke test với ba MCP client và benchmark release đã được chạy.

Phần lõi đã hoạt động, nhưng chưa có kiểm thử tích hợp cho mọi tình huống nghiệm thu nêu trong `spec.md`. Những khoảng trống kiểm chứng và giới hạn đã biết được ghi ở cuối báo cáo.

## Đã triển khai

### MCP gateway và upstream

- MCP surface chỉ công bố `tools_search` và `tools_execute`.
- Hỗ trợ upstream qua stdio và Streamable HTTP; catalog lấy namespace, tên, mô tả và schema từ cấu hình/metadata tool.
- Lọc catalog bằng trạng thái upstream, khả năng kết nối và policy trước khi quảng bá tool cho client.
- Tạo mô tả catalog XML có escape nội dung metadata. DTO tìm kiếm chỉ chứa dữ liệu tool cần thiết, không đưa score hoặc metadata nội bộ vào kết quả.
- Thêm API quản trị để xem/thử/refresh upstream, xem catalog, đánh giá policy và theo dõi execution.

### Cấu hình và giao diện quản trị

- `config.yaml` là nguồn cấu hình có thể chỉnh sửa; hỗ trợ parse, validate, đọc, ghi nguyên tử và reload.
- Ghi cấu hình qua API dùng ETag/`If-Match`; WebUI sửa và persist YAML. Reload file bên ngoài chỉ áp dụng khi validate/reload thành công.
- WebUI có các khu vực dashboard, upstreams, tools/search, configuration/policy và executions; Vite build assets vào `public/`.
- Bổ sung xác thực cho API quản trị, session WebUI có CSRF, cấu hình bearer token cho client/admin và các kiểm tra host/origin/TLS assertion khi triển khai từ xa.

### Policy, tìm kiếm và thực thi

- Hỗ trợ allow/deny, mặc định policy và wildcard; deny có ưu tiên. Tool bị chặn không xuất hiện trong catalog/search và broker kiểm tra policy lại trước dispatch.
- Có abstraction cho search provider và sandbox backend, cùng fake provider/backend để kiểm tra hợp đồng mà không đổi DTO công khai.
- Chạy mỗi execution trong Deno/V8 worker riêng. Cầu nối JavaScript cho phép gọi `tools.call("namespace.tool", args)`; không cấp API filesystem, network trực tiếp, imports, process, environment hoặc Node.
- Broker xác thực schema và policy trước dispatch; hỗ trợ gọi tool tuần tự/song song, deadline, hủy, giới hạn kích thước, số call, concurrency và queue.
- Worker được theo dõi bộ nhớ từ process cha. Đã sửa đường kết thúc worker để đóng IPC writer trước khi chờ tiến trình con, tránh deadlock sau khi worker gửi kết quả; đường đọc output cũng được làm bounded và cancellation-safe.
- SQLite lưu execution/call metadata đã giới hạn và redaction; không lưu source code, arguments hoặc tool payload. SSE phát metadata có replay giới hạn; execution còn dang dở khi restart được đánh dấu lỗi thay vì chạy lại.

### CI và tài liệu

- Thêm GitHub Actions trên Ubuntu 24.04, cố định Rust 1.93.1 và Node 22; pipeline chạy format, Clippy, Rust tests và WebUI build/typecheck.
- Cập nhật README với cách chạy, cấu hình upstream, deployment, giới hạn thực thi, API, tương thích client và benchmark.

## Quyết định triển khai

- Dùng Rust và `rmcp` 3.4.1 cho gateway MCP; giữ bề mặt MCP công khai ở đúng hai tool để catalog upstream không làm phình danh sách tool phía client.
- Dùng YAML làm nguồn cấu hình duy nhất; SQLite chỉ giữ dữ liệu audit/metrics, không trở thành nguồn cấu hình thứ hai.
- Tách execution khỏi gateway process bằng Deno worker mới cho mỗi lần chạy, với broker Rust nắm quyền policy/schema và điều phối tool call.
- Giữ secret dưới dạng tham chiếu tên biến môi trường trong YAML; không chuyển secret vào JavaScript worker.
- Mặc định giới hạn tài nguyên ở mức hữu hạn: 30 giây/execution, 4 execution đồng thời, queue 32, 50 tool call/execution, 256 KiB arguments/call, 2 MiB mỗi upstream result, 8 MiB tổng result và 128 KiB output trả về. Memory watchdog lấy mẫu mỗi 10 ms; README lưu ý độ vượt ngưỡng có thể đến một chu kỳ lấy mẫu.
- Cho truy cập loopback không cần token khi các token client/admin chưa được cấu hình; với triển khai từ xa, yêu cầu gateway đặt sau TLS reverse proxy và cấu hình token, host/origin rõ ràng.
- Giữ các benchmark phụ thuộc máy dưới dạng test thủ công bị ignore mặc định, để CI không phụ thuộc thời gian/memory của host chạy.

## Đã kiểm chứng

### Lệnh build và test

- `cargo fmt` đã chạy; workflow CI kiểm tra lại bằng `cargo fmt --check`.
- `cargo test`: 11 unit tests và 1 integration test thành công; 3 benchmark tests được ignore mặc định. Cả 3 benchmark đã được chạy riêng ở release profile.
- `cargo clippy --all-targets --all-features -- -D warnings`: thành công.
- `cargo build`: thành công.
- `npm run build --prefix web`: thành công.
- Integration test dùng RMCP SDK khởi tạo server stdio, xác nhận chính xác hai tool, chạy JavaScript trả JSON, timeout một vòng lặp vô hạn và xác nhận gateway tiếp tục nhận execution sau timeout.

### Tương thích client

Các lần smoke test chạy trên macOS 27.0 arm64 với Rust 1.93.1:

| Client | Phiên bản | Kết quả |
| --- | --- | --- |
| RMCP reference client | 3.4.1 | Initialize, list hai tool, execute JSON, timeout vòng lặp vô hạn và execute thành công tiếp theo |
| Codex CLI | 0.157.0 | Gọi `tools_execute`, nhận `{"result":{"answer":42}}` |
| Claude Code | 2.1.282 | Gọi `tools_execute`, nhận `{"answer":42}` |

Smoke test Codex và Claude dùng cấu hình tạm, không sửa thiết lập MCP lưu lâu dài. Hai client này được kiểm tra bằng JavaScript trả về giá trị đơn giản, không dispatch tới upstream thật.

### Benchmark release cục bộ

Đo trên macOS 27.0 arm64, Rust 1.93.1:

| Đo lường | Kết quả |
| --- | --- |
| Keyword search, 1.000 tools, exact query | Trung bình 419 µs / 50 lần |
| Keyword search, 1.000 tools, broad description query | Trung bình 345 µs / 50 lần |
| Keyword search, 10.000 tools, exact query | Trung bình 2.197 µs / 50 lần |
| Keyword search, 10.000 tools, broad description query | Trung bình 2.581 µs / 50 lần |
| Khởi động worker, 10 executions | Trung bình 12.010 µs; p50 9.442 µs; p95 35.470 µs |
| 4 executions đồng thời | 11 ms wall time; peak process-tree RSS 29,0 MiB |
| Ước lượng MCP context, 100 namespace và 5 kết quả search | 16.793 bytes, gồm wrapper descriptions, catalog, schemas và DTO mẫu |

Đây là baseline của máy đo cục bộ, không phải cam kết hiệu năng trên mọi môi trường.

## Chưa hoàn thành hoặc chưa được xác minh đầy đủ

- Chưa có E2E với mock upstream đếm số lần dispatch để chứng minh độc lập rằng deny chặn dispatch trên mọi đường đi.
- Chưa có bộ integration test đầy đủ cho nhiều upstream thật/giả cùng lúc, phối hợp tuần tự/song song, và toàn bộ trường hợp namespace có tiền tố trùng, pattern chồng lấn, tool name chứa dấu chấm hoặc dynamic tool name trong JavaScript.
- Chưa chạy compatibility E2E riêng với upstream Streamable HTTP; tương thích client được kiểm tra bằng tool trả kết quả cục bộ.
- Chưa có kiểm thử bao phủ đầy đủ các tình huống upstream disconnect, dữ liệu upstream quá cỡ, queue đầy, cancel race, worker crash và thay đổi policy/schema/sandbox đúng thời điểm execution đang chạy. Integration hiện có bao phủ timeout vòng lặp và phục hồi sau timeout.
- Chưa chạy canary leak test qua tất cả MCP output, logs, SQLite và SSE. Cấu trúc hiện tại giới hạn/redact audit metadata và không lưu payload, nhưng kiểm chứng đầu-cuối theo canary chưa được thực hiện.
- Chưa xác minh đầy đủ config reload thất bại giữ nguyên trạng thái đang chạy bằng một E2E test; tính nguyên tử được triển khai nhưng cần kiểm tra hành vi qua API và upstream.
- Periodic upstream refresh được khởi chạy trong chế độ HTTP server; chế độ `--stdio` hiện chưa chạy vòng refresh nền tương tự.
- HTTP Streamable upstream có giới hạn SSE event; chưa có kiểm chứng riêng cho giới hạn tổng response body được áp dụng trước khi thư viện MCP parse nội dung.
- WebUI đã được build/typecheck, nhưng báo cáo này không ghi nhận một lượt kiểm thử UI end-to-end bằng browser.

## Tệp chính

- `src/`: gateway, API, policy, config, registry/search, execution/sandbox và audit.
- `web/`: mã nguồn WebUI; `public/`: assets build.
- `tests/mcp_stdio.rs`: kiểm thử tích hợp với RMCP reference client.
- `tests/worker_benchmark.rs`: benchmark worker startup/RSS/concurrency.
- `.github/workflows/ci.yml`: pipeline CI.
- `README.md`: hướng dẫn vận hành, compatibility và benchmark.

`spec.md` được giữ nguyên; báo cáo này ghi lại trạng thái triển khai và các phần còn thiếu bằng chứng kiểm chứng.

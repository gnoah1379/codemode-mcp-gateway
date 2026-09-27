# Source Code Review Report

**Ngày review:** 2026-09-27
**Phạm vi:** Đối chiếu mã nguồn với [`spec.md`](spec.md) và [`implementation-report.md`](implementation-report.md); đánh giá logic, convention, kiến trúc và khả năng bảo trì.

**Cập nhật sau review:** Khi bổ sung CLI/service, mã nguồn đã được sửa để kiểm tra lại policy và schema sau khi chờ permit (mục 1), từ chối auth nếu đã cấu hình token nhưng biến môi trường bị thiếu (mục 2), và đọc file reload bên trong config commit lock (mục 3). Đã thêm test cho auth thiếu token và reload. Các phát hiện bên dưới ghi lại trạng thái tại thời điểm review ban đầu; các mục 4–7 và những khoảng trống kiểm thử khác chưa được xử lý trong thay đổi này.

## Kết luận

Project đã triển khai đúng phần lớn kiến trúc lõi: chỉ công bố hai MCP tool, dùng YAML typed config, tách registry/search/policy/execution, dùng `rmcp` cho MCP, `jsonschema` để kiểm tra arguments và chạy JavaScript trong worker process riêng. `implementation-report.md` mô tả khá trung thực các khoảng trống kiểm thử đã biết.

Tại thời điểm review ban đầu, project **chưa đạt đầy đủ spec** và có ba lỗi mức cao liên quan đến policy, xác thực và tính nhất quán cấu hình. Các lỗi này đã được chỉnh sửa sau đó như ghi chú ở trên; project vẫn còn các mục chưa hoàn thành hoặc chưa được kiểm chứng đầy đủ.

## Phát hiện theo mức độ ưu tiên

### 1. Cao — Policy mới có thể không chặn call đang chờ semaphore

**Bằng chứng:** `ExecutionContext::dispatch` lấy snapshot và đánh giá policy tại `src/execution.rs:639-651`, rồi chờ upstream/execution semaphore tại `src/execution.rs:753-762` trước khi gọi upstream tại `src/execution.rs:778-780`.

**Tác động:** Nếu admin thêm deny trong lúc call đang chờ permit, call có thể vẫn được dispatch theo policy cũ. Điều này không đáp ứng yêu cầu tại spec §3, §7 và §11 rằng policy mới phải chặn các dispatch tiếp theo.

**Đề xuất:** Sau khi lấy đủ permit, lấy snapshot hiện hành và kiểm tra lại policy, tool availability, generation và schema ngay trước `registry::call`. Thêm integration test dùng mock upstream đếm dispatch, giữ call trong hàng chờ rồi cập nhật deny.

### 2. Cao — Token đã cấu hình nhưng thiếu biến môi trường có thể mở auth trên loopback

**Bằng chứng:** `token_from_config` trả `None` khi biến môi trường thiếu hoặc rỗng (`src/api.rs:800-804`). `admin_auth` cho qua request loopback nếu `expected.is_none()` (`src/api.rs:647-649`); MCP cũng dùng nhánh tương tự (`src/api.rs:680-700`). `create_session` có cùng cách xử lý (`src/api.rs:535-550`). Kiểm tra loopback dựa vào header `Host` (`src/api.rs:788-793`).

**Tác động:** Một cấu hình đã khai báo `client_token_env` hoặc `admin_token_env` nhưng thiếu giá trị lúc chạy sẽ bị diễn giải như không bật auth trên loopback. Điều này có thể gây mở truy cập ngoài ý định, đặc biệt khi gateway được đưa ra ngoài qua proxy/tunnel.

**Đề xuất:** Phân biệt rõ “không cấu hình token” với “đã cấu hình nhưng không đọc được token”; trường hợp sau phải fail closed hoặc từ chối config. Không dùng riêng header `Host` làm bằng chứng nguồn request là loopback.

### 3. Cao — Reload đồng thời với PUT có thể làm YAML và runtime lệch nhau

**Bằng chứng:** `reload_file` đọc file trước khi gọi `replace_config` (`src/state.rs:180-183`), trong khi `replace_config` chỉ lấy `config_lock` khi bắt đầu commit (`src/state.rs:136-143`) và PUT ghi file trong lock (`src/state.rs:159-171`).

**Tác động:** Reload có thể đọc candidate cũ, PUT ghi và publish candidate mới, rồi reload lấy lock và publish lại candidate cũ. File vẫn là bản mới nhưng runtime chạy bản cũ, vi phạm nguyên tắc `config.yaml` là nguồn cấu hình duy nhất.

**Đề xuất:** Đọc file và commit reload trong cùng vùng tuần tự hóa; tránh gọi lại hàm tự lấy cùng mutex. Thêm test chạy PUT/reload đồng thời và so sánh file, revision, snapshot.

### 4. Trung bình — Lỗi từ broker bị biến thành `JAVASCRIPT_ERROR`

**Bằng chứng:** Worker chuyển lỗi `tools.call` thành exception có `code` (`src/execution.rs:1308-1310`) và trả code đó khi script không bắt lỗi (`src/execution.rs:1327-1333`). Tuy nhiên `sanitize_script_error` chỉ giữ ba code, đổi mọi code còn lại thành `JAVASCRIPT_ERROR` (`src/execution.rs:1132-1149`).

**Tác động:** Một call bị deny, sai schema, quá quota hoặc upstream unavailable có thể trả lỗi công khai chung `JAVASCRIPT_ERROR`, thay vì `POLICY_DENIED`, `INVALID_ARGUMENTS`, v.v. như hợp đồng `tools_execute` trong spec §6. Script bắt lỗi bên trong vẫn nhìn thấy code đúng, nên hai đường xử lý có semantics khác nhau.

**Đề xuất:** Gắn nguồn lỗi broker bằng kiểu/marker nội bộ và giữ whitelist code broker hợp lệ; vẫn chuẩn hóa lỗi do chính script tạo để tránh giả mạo thông tin. Kiểm tra bằng MCP integration test cho cả lỗi được bắt và không được bắt.

### 5. Trung bình — Retention chỉ chạy khi startup, bộ nhớ lịch sử tăng liên tục

**Bằng chứng:** SQLite chỉ xóa execution quá hạn khi tạo `GatewayState` (`src/state.rs:109-115`). `ExecutionManager` lưu records trong `HashMap`, thêm khi chạy và không có eviction định kỳ (`src/execution.rs:67-75`, `src/execution.rs:323-340`).

**Tác động:** Server chạy liên tục không thực thi retention 7 ngày theo spec §10; số record trong RAM và database tiếp tục tăng đến khi restart hoặc hết dung lượng. Giới hạn `max_page_count` chỉ khiến ghi DB thất bại khi đầy, không thay thế retention.

**Đề xuất:** Lập tác vụ cleanup định kỳ cho SQLite và in-memory records, dùng cùng chính sách retention và giới hạn số record. Ghi nhận lỗi persistence thay vì bỏ qua hoàn toàn.

### 6. Trung bình — Refresh upstream chưa đáp ứng lifecycle trong spec

**Bằng chứng:** Client upstream dùng handler `()` khi `serve` (`src/registry.rs:397-422`), nên không xử lý notification tool-list-changed. Refresh nền chỉ được khởi chạy trong HTTP mode (`src/api.rs:75-94`), còn `--stdio` đi thẳng vào `mcp::serve_stdio` (`src/main.rs:36-40`). API test một upstream lại chạy `registry::prepare` cho toàn bộ config (`src/api.rs:298-315`), và refresh theo tên cũng rebuild toàn bộ (`src/api.rs:316-342`).

**Tác động:** Catalog có thể cũ trong stdio mode cho đến khi thao tác reload/refresh thủ công. Test/refresh một upstream tốn thời gian và kết nối của tất cả upstream; `prepare` thực hiện discovery tuần tự, mỗi upstream có timeout 10 giây (`src/registry.rs:185-219`). Spec §8 yêu cầu notification, reconnect và fallback refresh.

**Đề xuất:** Đăng ký handler notification từ SDK; chạy fallback refresh trong cả hai transport mode; tách discovery/refresh theo namespace và tái sử dụng client không đổi khi phù hợp.

### 7. Thấp — Search byte budget dùng sai số cố định

**Bằng chứng:** `src/mcp.rs:169-188` đo serialized `CallToolResult` rồi cộng thêm 128 byte. Kết quả rỗng trả sớm tại `src/mcp.rs:166-167` mà không qua phép đo.

**Tác động:** Tool có envelope thực tế vừa budget vẫn có thể bị báo `SEARCH_RESULT_TOO_LARGE`; việc áp dụng budget không nhất quán. Điều này lệch yêu cầu chọn kết quả đứng đầu vừa budget tại spec §5.3 và §6.4.

**Đề xuất:** Đo chính xác response MCP sẽ trả ở cùng lớp serialize; áp dụng cùng một kiểm tra cho cả `[]`.

## Đánh giá convention, pattern và thư viện

- **Điểm tốt:** Rust formatting và Clippy sạch; config dùng struct typed với `deny_unknown_fields`; policy, search và sandbox có abstraction riêng; MCP và JSON Schema dùng thư viện phù hợp thay vì tự triển khai giao thức/validator. Các giới hạn IPC, worker process và dữ liệu audit cho thấy đã cân nhắc rủi ro thực tế.
- **Logic tự viết có lý do:** Wildcard matcher nhỏ, đúng semantics hai thành phần của spec. Framing IPC tự viết để cưỡng chế giới hạn byte. Chưa thấy bằng chứng rằng thay bằng thư viện sẽ đơn giản hơn mà vẫn giữ đúng hợp đồng.
- **Nên giảm code tự quản ở phần nhạy cảm:** Auth, cookie/session và CSRF nằm trực tiếp trong `src/api.rs`, gồm cả parse cookie và so sánh token. Đây là phần nên dùng một lớp auth/session chuyên trách hoặc thư viện đã được kiểm chứng, với test đầy đủ cho token thiếu, hết hạn, proxy và CSRF. Lỗi ở phát hiện số 2 cho thấy rủi ro của logic phân nhánh hiện tại.
- **Khả năng bảo trì:** `src/execution.rs` khoảng 1.570 dòng và `src/api.rs` khoảng 834 dòng gộp nhiều trách nhiệm. `web/src/App.tsx` khoảng 333 dòng nhưng phần lớn biểu thức/JSX được nén vào các dòng rất dài và có `Record<string, any>`; điều này làm review, debug và test từng phần khó hơn. Nên tách execution manager, broker, worker IPC, audit; tách API auth/config/catalog; tách WebUI theo page/component/hook.
- **Độ rõ của lỗi:** Một số chỗ phân loại lỗi bằng cách tìm substring trong `anyhow::Error` (`src/execution.rs:840-860`) hoặc bỏ qua lỗi ghi audit (`src/execution.rs:1419-1426`). Enum lỗi nội bộ sẽ ổn định hơn và giúp giữ semantics từ broker đến MCP output.

## Mức đáp ứng spec

| Nhóm | Đánh giá |
| --- | --- |
| MCP surface, DTO search/execute, YAML typed config, policy cơ bản | Đã triển khai; cần sửa race và error semantics ở trên. |
| Sandbox worker, timeout vòng lặp vô hạn, giới hạn output/call | Có implementation và smoke test; các tình huống crash, oversized HTTP response, cancellation race chưa có E2E đầy đủ. |
| Upstream discovery/refresh | Có pagination và atomic snapshot; thiếu notification handler và refresh nền cho stdio mode. |
| Auth, config consistency, retention | Chưa đạt đầy đủ do các lỗi mức cao/trung bình nêu trên. |
| WebUI, SSE, audit/metrics | Có giao diện và API; chưa có browser E2E và canary leak test. |
| Compatibility | Đã smoke test client cho JavaScript trả kết quả cục bộ; chưa chứng minh end-to-end với nhiều upstream và Streamable HTTP. |

`implementation-report.md` đã nêu nhiều giới hạn kiểm chứng ở cuối tài liệu. Các phát hiện 1–5 ở đây là vấn đề suy ra trực tiếp từ đường code, không chỉ là thiếu test. Chưa chạy test tái hiện riêng cho từng race hoặc lỗi auth trong review này.

## Kiểm chứng đã chạy trong lượt review

- `cargo fmt --check`: pass.
- `cargo clippy --all-targets --all-features -- -D warnings`: pass.
- `cargo test`: 11 unit tests và 1 MCP integration test pass; 3 benchmark tests bị ignore theo thiết kế.
- `npm run build --prefix web`: TypeScript build và Vite build pass.

Các kiểm tra này xác nhận code hiện build được, nhưng chưa chứng minh các tiêu chí nghiệm thu còn thiếu trong spec §11. Ưu tiên tiếp theo là sửa phát hiện 1–3 và thêm integration test tái hiện từng lỗi.

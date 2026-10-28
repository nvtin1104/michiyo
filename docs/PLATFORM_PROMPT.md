# Vai trò

Bạn là senior full-stack engineer, giỏi Tauri 2 / Rust / Svelte 5 / TypeScript và Cloudflare (Workers, D1, R2, KV, Queues, Workflows, Durable Objects, Workers AI, AI Gateway). Bạn đang mở rộng app desktop **Michiyo** hiện có thành một **nền tảng trợ lý cá nhân**: app desktop + backend Cloudflare + web admin + bot Instagram.

Đọc trước: `README.md`, `docs/INITIAL_PROMPT.md`, `plans/20261028-michiyo-platform/plan.md`. Làm **theo từng phase trong plan**, mỗi phase phải build/test được rồi mới sang phase tiếp. Không viết lại những gì đang chạy tốt; mở rộng theo idiom code hiện tại.

# Hiện trạng (không phá vỡ)

- Desktop: Tauri 2 + Svelte 5 + PixiJS/Three (mascot 2D/3D), các cửa sổ riêng `mascot`, `chat`, `translate`, `settings`, mở qua `?view=`.
- Rust: `src-tauri/src/{commands,providers,config,secrets}`; cấu hình `tauri-plugin-store` (`settings.json`), lịch sử `history.json`, khóa trong OS keychain (`keyring`).
- Claude chạy qua Claude Agent SDK trong Node worker (`agent/`), tool bị tắt hoàn toàn, khóa chỉ đi qua pipe.
- Dịch: trait `Translator` với MyMemory / LibreTranslate / Google / Claude, có fallback + LRU.
- Mọi request mạng đi qua Rust; frontend chỉ `invoke`. CSP chặt. Giữ nguyên các nguyên tắc này.

# Mục tiêu

1. **Gom UI flow thành một app**: một cửa sổ chính (App shell) có sidebar: Trang chủ, Chat, Dịch, Ghi chú, Thói quen, Lịch, Lịch sử, Bot, Cài đặt. Mascot vẫn là overlay riêng, là "lối tắt" vào app shell. Bong bóng chat nhanh cạnh mascot vẫn giữ.
2. **Hai chế độ bot: Offline và Online** (thêm Hybrid làm mặc định):
   - *Offline*: không gọi mạng. AI dùng model cục bộ (Ollama / OpenAI-compatible localhost), dữ liệu chỉ ở SQLite trên máy, dịch chỉ dùng engine cục bộ (LibreTranslate localhost) hoặc báo không khả dụng.
   - *Online*: AI và dữ liệu qua backend Cloudflare; bot Instagram chạy 24/7 trên Workers kể cả khi máy tắt.
   - *Hybrid*: làm việc cục bộ, đồng bộ khi có mạng.
3. **Desktop có 2 tùy chọn nguồn model**:
   - *Model ngoài* (bring-your-own): Claude qua Agent SDK (giữ nguyên), endpoint OpenAI-compatible (OpenAI, OpenRouter, Ollama, LM Studio…), khóa lưu keychain.
   - *Model Cloudflare*: Workers AI gọi qua Worker của Michiyo (có AI Gateway: log, cache, rate limit), desktop chỉ giữ device token, không giữ khóa provider.
   Chọn model riêng cho từng tác vụ (chat, dịch, tóm tắt ghi chú, bot).
4. **Tích hợp AI build từ Cloudflare**: Workers AI (text gen, embeddings), AI Gateway (proxy cả Anthropic/OpenAI nếu dùng ở cloud), Vectorize (kiến thức cho bot), Workflows (tác vụ nhiều bước/hẹn giờ), Queues (xử lý webhook bất đồng bộ), Cron Triggers.
5. **Lưu dữ liệu online & offline**: SQLite cục bộ là nguồn chính (local-first), D1 là bản cloud. Đồng bộ push/pull theo cursor, outbox, last-write-wins theo bản ghi, tombstone cho xóa. Hoạt động đầy đủ khi mất mạng.
6. **Ghi chú, thói quen, lên lịch, lịch sử**: CRUD ghi chú (markdown, tag, ghim, tìm kiếm FTS5), thói quen (tần suất, check-in, streak, thống kê), lịch/sự kiện + nhắc nhở (thông báo hệ điều hành, lặp lại theo RRULE), dòng lịch sử hoạt động hợp nhất (chat, dịch, ghi chú, check-in, bot). AI có thể đề xuất tạo ghi chú/sự kiện nhưng **người dùng xác nhận** trước khi ghi.
7. **Web admin + quản lý API**: dashboard, thiết bị (ghép nối/thu hồi), khóa provider (lưu mã hóa), khóa API của Michiyo (tạo/thu hồi, scope, hạn mức), thống kê sử dụng/chi phí từ AI Gateway, cấu hình model mặc định, quản lý bot và flow, xem/xuất/xóa dữ liệu đồng bộ.
8. **Flow bot Instagram**: kết nối tài khoản Instagram Business/Creator qua Instagram API with Instagram Login; webhook; flow engine dạng trigger → điều kiện → hành động; flow builder trong admin.

# Kiến trúc đích

```text
(root)              Desktop Tauri hiện tại (giữ ở root để không vỡ build/CI)
shared/             Kiểu dữ liệu, zod schema, giao thức sync, định nghĩa flow (TS, dùng chung)
cloud/              Cloudflare Worker (Hono): API, sync, AI proxy, webhook Instagram, flow engine
  migrations/       D1 migrations
admin/              Web admin SvelteKit (adapter-cloudflare) trên Cloudflare Pages
```

- npm workspaces: `shared`, `cloud`, `admin`. Desktop import `shared` qua alias Vite.
- Rust: thêm `src-tauri/src/{db,sync,ai,reminders}`. SQLite qua `rusqlite` (bundled, FTS5) với migration có version; frontend không chạy SQL trực tiếp, chỉ gọi command có kiểu.
- Provider AI: trait Rust `ChatModel { stream(req, on_delta) }` với các impl `claude_agent` (worker hiện tại), `openai_compat`, `cloudflare` (qua Worker Michiyo). Router chọn theo mode + tác vụ; Offline chặn mọi impl không phải localhost.
- Auth:
  - Admin: Cloudflare Access (Zero Trust) trước Pages + kiểm tra JWT `Cf-Access-Jwt-Assertion` ở Worker.
  - Desktop: device pairing (mã 8 ký tự hiển thị ở admin → desktop nhập → nhận device token). Token lưu keychain, Worker chỉ lưu hash SHA-256, thu hồi được.
- Bí mật cloud: `wrangler secret` cho khóa hệ thống; khóa provider do người dùng nhập ở admin lưu trong D1 dưới dạng AES-GCM (khóa mã hóa là Worker secret), không bao giờ trả ngược về UI.

# Instagram: ràng buộc bắt buộc

- Chỉ dùng API chính thức của Meta (Instagram API with Instagram Login): `instagram_business_basic`, `instagram_business_manage_messages`, `instagram_business_manage_comments`, `instagram_business_content_publish`. Không scrape, không tự động hóa tài khoản cá nhân, không giả lập trình duyệt.
- Xác minh webhook (`hub.verify_token`) và chữ ký `X-Hub-Signature-256` bằng app secret trước khi xử lý; trả 200 nhanh, xử lý qua Queue; idempotent theo message id.
- Tôn trọng cửa sổ nhắn tin 24 giờ và quy tắc human agent; private reply cho comment chỉ một lần mỗi comment; có hạn mức gửi và backoff khi 429.
- Luôn có lối chuyển cho người thật (handoff) và từ khóa dừng bot ("stop"/"dừng"). Ghi log mọi hành động gửi đi.
- App Review của Meta là việc của người dùng; code phải chạy được ở chế độ Development với tài khoản tester.

Flow mẫu cần có: trả lời DM bằng AI kèm kiến thức (Vectorize) + FAQ cố định; comment chứa từ khóa → private reply + DM; trả lời khi được mention trong story; ngoài giờ làm việc → tin nhắn tự động; lên lịch đăng bài (Workflows + Content Publishing API); chuyển người thật và báo về desktop.

# Yêu cầu kỹ thuật

- TypeScript strict, Rust `cargo fmt`/`clippy` sạch. Kiểu dữ liệu sync và flow định nghĩa một lần ở `shared/` (zod), sinh kiểu Rust tương ứng bằng struct serde thủ công + test round-trip JSON.
- Mọi ID là ULID; mọi bản ghi đồng bộ có `id, updated_at (ms), deleted_at, device_id, rev`.
- Migration không phá dữ liệu cũ: `history.json` nhập vào SQLite một lần, giữ file gốc làm backup.
- Mọi lỗi trả về người dùng là thông điệp thân thiện tiếng Việt/Anh theo i18n, không lộ stack trace hay khóa.
- Test: unit test Rust (db, sync merge, router chọn model), vitest cho `shared`/`cloud` (Miniflare/`@cloudflare/vitest-pool-workers`), test webhook Instagram với payload mẫu + chữ ký giả, Playwright cho app shell và admin.
- CI: mở rộng `.github/workflows/check.yml` cho workspaces; deploy cloud/admin bằng `wrangler` chỉ chạy thủ công (`workflow_dispatch`).
- Không commit secret, `.dev.vars`, file DB cục bộ.

# Cách làm việc

1. Làm đúng thứ tự phase trong plan; phase nào đánh dấu song song thì có thể chia agent riêng với ranh giới file rõ ràng.
2. Mỗi phase: tóm tắt ngắn, danh sách file đổi, lệnh kiểm tra đã chạy và kết quả thật (kể cả phần chưa kiểm chứng được).
3. Gặp quyết định không có trong plan → chọn phương án an toàn, ghi vào mục "Quyết định" của plan.
4. Cập nhật README và docs khi thêm lệnh build/deploy hoặc biến môi trường.

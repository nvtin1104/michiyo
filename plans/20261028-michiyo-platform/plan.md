# Plan: Michiyo Platform

Ngày: 2026-10-28 · Prompt thực thi: `docs/PLATFORM_PROMPT.md`

## Tổng quan

| Phase | Nội dung | Phụ thuộc | Song song với |
|---|---|---|---|
| P0 | Nền tảng: workspaces, `shared/`, SQLite cục bộ, migrate lịch sử | — | — |
| P1 | App shell: gom Chat/Dịch/Cài đặt vào một cửa sổ | P0 | P3, P4 |
| P2 | Ghi chú, thói quen, lịch + nhắc nhở, lịch sử | P0, P1 | P4 |
| P3 | Router model: Model ngoài / Model Cloudflare, chế độ Offline/Online/Hybrid | P0 | P1, P4 |
| P4 | Backend Cloudflare: Worker API, D1, auth thiết bị, AI proxy | P0 | P1, P2, P3 |
| P5 | Đồng bộ online/offline | P2, P4 | P6 |
| P6 | Web admin + quản lý API | P4 | P5 |
| P7 | Bot Instagram: webhook, flow engine, flow builder | P4, P6 | — |
| P8 | Hoàn thiện: test E2E, CI/deploy, tài liệu | tất cả | — |

Đường găng: P0 → P4 → P6 → P7. Desktop (P1–P3) và cloud (P4) có thể giao cho hai luồng làm việc riêng sau P0.

---

## P0 — Nền tảng

**Mục tiêu:** có chỗ chứa code dùng chung và kho dữ liệu cục bộ thay cho JSON.

- `package.json`: thêm `"workspaces": ["shared", "cloud", "admin"]`; script `build:shared`.
- `shared/src/`:
  - `ids.ts` (ULID), `records.ts` (zod: `Note`, `Habit`, `HabitLog`, `Event`, `Reminder`, `Activity`, `Conversation`, `Message` — đều có `id, updated_at, deleted_at, device_id, rev`).
  - `sync.ts` (`PushRequest`, `PullResponse`, cursor), `flow.ts` (để trống, điền ở P7).
- `vite.config.ts` + `tsconfig.json`: alias `@michiyo/shared`.
- Rust `src-tauri/src/db/`:
  - `rusqlite` với feature `bundled` (có FTS5); file `michiyo.db` trong app data, WAL mode.
  - `migrations/0001_init.sql`: bảng cho mọi record trên + `outbox`, `sync_state`, FTS5 `notes_fts`.
  - Migration runner theo `PRAGMA user_version`.
  - Nhập `history.json` → `conversations`/`messages` một lần, đổi tên file thành `history.json.bak`.
- Command lịch sử chat chuyển sang đọc/ghi SQLite; UI Chat không đổi hành vi.

**Xong khi:** `npm run build`, `cargo test` (test migration + import) đạt; chat cũ vẫn hiện sau khi nâng cấp.

## P1 — App shell (gom UI flow)

**Mục tiêu:** một cửa sổ chính thay cho các cửa sổ rời Chat/Dịch/Cài đặt.

- `tauri.conf.json`: thêm cửa sổ `main` (có viền, resize, hiện trên taskbar, tạo khi dùng lần đầu như hiện tại).
- `src/shell/Shell.svelte`: sidebar + router hash (`#/chat`, `#/translate`, `#/notes`, `#/habits`, `#/calendar`, `#/history`, `#/bot`, `#/settings`); trang chủ tổng hợp: lịch hôm nay, thói quen chưa check-in, ghi chú ghim.
- Tái sử dụng `Chat.svelte`, `Translate.svelte`, `Settings.svelte` làm route; tách phần chỉ dành cho popup (định vị cạnh con trỏ, bong bóng mascot) ra props `compact`.
- Giữ: bong bóng chat nhanh cạnh mascot, popup dịch Ctrl+Alt+T. Thêm nút "Mở trong app" từ hai nơi này.
- Menu mascot/tray: "Mở Michiyo" + các mục cũ điều hướng vào route tương ứng. Phím tắt mới (mặc định `Ctrl+Alt+Space`) mở app shell.
- Thanh trạng thái: chế độ (Offline/Online/Hybrid), trạng thái đồng bộ, model đang dùng.
- Cập nhật `scripts/verify-ui.mjs` cho app shell.

**Xong khi:** mọi luồng cũ chạy qua route mới; `npm run test:ui`, `test:mascot`, `test:settings` đạt.

## P2 — Ghi chú, thói quen, lịch, lịch sử

- Rust command có kiểu cho từng module (`notes_*`, `habits_*`, `events_*`, `activity_list`); mỗi thao tác ghi vào bảng + `outbox` + `activity` trong một transaction.
- **Ghi chú**: markdown (dùng renderer + DOMPurify sẵn có), tag, ghim, tìm kiếm FTS5, "Tóm tắt bằng AI", "Lưu câu trả lời chat thành ghi chú".
- **Thói quen**: tần suất (hằng ngày / ngày trong tuần / N lần mỗi tuần), check-in, streak, biểu đồ heatmap, mascot `happy` khi check-in.
- **Lịch**: sự kiện, việc cần làm có hạn, lặp lại RRULE (crate `rrule`), xem ngày/tuần/tháng.
- **Nhắc nhở**: `tauri-plugin-notification`; scheduler Rust (`src-tauri/src/reminders/`) quét sự kiện sắp tới mỗi phút, chạy khi app ở tray; mascot báo kèm bong bóng; snooze.
- **Lịch sử**: dòng thời gian hợp nhất, lọc theo loại/ngày, xóa từng mục hoặc theo khoảng.
- **AI đề xuất hành động**: model trả JSON đề xuất (tạo ghi chú/sự kiện/thói quen); UI hiện thẻ xác nhận, chỉ ghi khi người dùng bấm. Không bật tool tự ghi trong Agent SDK.

**Xong khi:** CRUD đủ cả 4 module offline; test Rust cho streak, RRULE, scheduler; Playwright cho từng route.

## P3 — Router model và chế độ

- Rust `src-tauri/src/ai/`: trait `ChatModel`; impl:
  - `claude_agent` — bọc `providers/agent.rs` hiện có.
  - `openai_compat` — `POST {base}/chat/completions` streaming SSE; preset OpenAI, OpenRouter, Ollama (`http://127.0.0.1:11434/v1`), LM Studio.
  - `cloudflare` — gọi `POST /v1/ai/chat` của Worker Michiyo bằng device token (hoạt động sau P4; trước đó hiện "chưa kết nối cloud").
- Config mới: `mode: 'offline' | 'online' | 'hybrid'`, `modelSource: 'external' | 'cloudflare'`, `models: { chat, translate, summarize }` mỗi mục `{ provider, model }`.
- Router: Offline chỉ cho provider có host localhost; Translator Claude/Google/MyMemory bị loại khỏi fallback khi Offline.
- Cài đặt → tab **AI**: chọn nguồn, nhập endpoint/khóa, "Kiểm tra kết nối", liệt kê model (`/models`, `/api/tags` cho Ollama).
- `agent/worker.ts` không đổi giao thức; chỉ thêm định tuyến ở Rust.

**Xong khi:** test router (mock HTTP bằng `wiremock`) đạt; chat chạy được với Ollama cục bộ khi Offline.

## P4 — Backend Cloudflare

- `cloud/`: Hono trên Workers, `wrangler.jsonc` với binding `DB` (D1), `KV`, `R2`, `AI`, `VECTORIZE`, `QUEUE`, Workflows, Durable Object `RateLimiter`.
- `cloud/migrations/0001_init.sql`: `devices`, `pairing_codes`, `api_keys`, `provider_keys` (mã hóa), `usage`, các bảng record đồng bộ (cùng schema `shared`), `ig_accounts`, `flows`, `flow_runs`, `ig_messages`.
- Auth:
  - Middleware admin: kiểm tra JWT Cloudflare Access (aud + certs).
  - `POST /v1/pair/start` (admin tạo mã, hạn 10 phút) → `POST /v1/pair/complete` (desktop đổi mã lấy token).
  - Middleware device: `Authorization: Bearer`, so hash, cập nhật `last_seen`.
  - API key của Michiyo (`mk_…`) cho client bên ngoài: hash, scope, hạn mức theo ngày.
- AI: `POST /v1/ai/chat` (SSE) qua Workers AI binding với `gateway: { id }`; tùy chọn proxy Anthropic/OpenAI qua AI Gateway dùng `provider_keys`; `POST /v1/ai/embed`. Ghi `usage`.
- Mã hóa khóa provider: AES-GCM, khóa ở secret `ENCRYPTION_KEY`; không có endpoint đọc ngược khóa.
- Test: `@cloudflare/vitest-pool-workers` cho auth, pairing, AI proxy (mock binding).

**Xong khi:** `wrangler dev` chạy; desktop ghép nối được với Worker cục bộ và chat bằng Model Cloudflare.

## P5 — Đồng bộ online/offline

- Giao thức (`shared/sync.ts`):
  - `POST /v1/sync/push`: lô bản ghi từ `outbox` (tối đa 500), idempotent theo `(id, rev)`.
  - `GET /v1/sync/pull?cursor=`: bản ghi đổi sau cursor (cursor = `server_seq` tăng dần trong D1).
- Hợp nhất: last-write-wins theo `updated_at`, hòa thì so `device_id`; xóa là tombstone, dọn sau 30 ngày. Ghi chú bị sửa đồng thời: giữ bản thua thành "bản xung đột" thay vì mất.
- Rust `src-tauri/src/sync/`: vòng sync khi có mạng (khởi động, mỗi 60 giây, sau khi ghi có debounce), backoff mũ khi lỗi; Offline tắt hẳn; Hybrid/Online bật.
- Lựa chọn phạm vi đồng bộ trong Cài đặt (ghi chú / thói quen / lịch / lịch sử chat); lịch sử chat mặc định **không** đồng bộ.
- UI: chỉ báo trạng thái, "Đồng bộ ngay", số thay đổi chờ.

**Xong khi:** test merge (Rust + vitest) đạt; hai instance desktop với hai DB khác nhau hội tụ sau khi mất mạng rồi có lại.

## P6 — Web admin + quản lý API

- `admin/`: SvelteKit + `adapter-cloudflare`, gọi Worker qua service binding; giao diện thống nhất phong cách với desktop.
- Trang:
  - **Tổng quan**: thiết bị online, request AI hôm nay, chi phí ước tính, lỗi gần đây, tin nhắn bot.
  - **Thiết bị**: tạo mã ghép nối, đổi tên, thu hồi.
  - **API**: khóa provider (thêm/thay/xóa, kiểm tra kết nối, không hiển thị lại giá trị); khóa Michiyo (tạo — chỉ hiện một lần, scope, hạn mức, thu hồi); model mặc định cho từng tác vụ; liên kết log AI Gateway.
  - **Sử dụng**: biểu đồ theo ngày / model / thiết bị / khóa.
  - **Dữ liệu**: xem ghi chú/thói quen/lịch đã đồng bộ (chỉ đọc), xuất JSON, xóa toàn bộ.
  - **Bot**: tài khoản Instagram, flow, hội thoại, kiến thức (P7).
- Playwright cho luồng tạo khóa và ghép nối.

**Xong khi:** deploy Pages sau Cloudflare Access; mọi thao tác trên đều qua Worker có kiểm tra auth.

## P7 — Bot Instagram

- **Kết nối**: OAuth Instagram Login từ admin → đổi long-lived token → lưu mã hóa, tự refresh trước khi hết hạn (Cron).
- **Webhook** `GET/POST /v1/ig/webhook`: verify token, kiểm `X-Hub-Signature-256`, đẩy vào Queue, trả 200 ngay.
- **Flow engine** (`cloud/src/flows/`), định nghĩa trong `shared/flow.ts`:
  - Trigger: `dm_received`, `comment_created`, `story_mention`, `schedule`.
  - Điều kiện: từ khóa/regex, giờ làm việc, tag người dùng, lần đầu nhắn, có trong cửa sổ 24 giờ.
  - Hành động: `reply_text`, `reply_ai` (Workers AI + Vectorize, system prompt theo bot), `private_reply`, `send_quick_replies`, `tag_user`, `handoff`, `notify_desktop`, `wait` (Workflows), `publish_post`.
  - Trạng thái hội thoại trong Durable Object theo `ig_user_id` (khóa tuần tự, chống trả lời trùng); idempotent theo `mid`.
  - Bảo vệ: kiểm cửa sổ 24 giờ trước khi gửi, mỗi comment chỉ một private reply, rate limit theo tài khoản, từ khóa dừng, handoff tắt bot cho hội thoại đó.
- **Flow builder** trong admin: danh sách node dọc (trigger → điều kiện → hành động), kiểm tra hợp lệ bằng zod, bật/tắt, chế độ thử với payload mẫu, lịch sử chạy (`flow_runs`).
- **Kiến thức**: tải FAQ/tài liệu → chia đoạn → embed → Vectorize; R2 giữ file gốc.
- **Flow mẫu**: trả lời DM bằng AI + FAQ; comment từ khóa → private reply + DM; story mention cảm ơn; ngoài giờ; lên lịch đăng bài; chuyển người thật + báo desktop.
- **Desktop**: route `#/bot` xem hội thoại, nhận thông báo handoff, trả lời tay (đi qua Worker).
- Test: payload webhook mẫu + chữ ký giả, engine chạy từng flow mẫu với Graph API mock.

**Xong khi:** với tài khoản tester ở chế độ Development, cả 6 flow mẫu chạy đúng; không gửi tin ngoài cửa sổ 24 giờ.

## P8 — Hoàn thiện

- CI: job cho `shared`/`cloud`/`admin` (typecheck, vitest, build); job desktop giữ như cũ; deploy `wrangler` + Pages chỉ qua `workflow_dispatch`.
- Tài liệu: README (kiến trúc mới, chế độ, cách tự host phần cloud), `docs/cloud-setup.md` (tạo D1/KV/R2/Vectorize/AI Gateway, secrets, Access), `docs/instagram-setup.md` (tạo Meta app, quyền, webhook, App Review).
- Kiểm tra bảo mật: khóa không lọt vào log/URL/UI, CSP desktop thêm đúng host Worker, rate limit endpoint công khai.
- Kiểm hiệu năng: RAM idle desktop vẫn < 80 MB khi app shell đóng; scheduler nhắc nhở không làm CPU idle tăng.

---

## Rủi ro

- **Meta App Review** có thể kéo dài/bị từ chối; quyền messaging cần tài khoản Business/Creator đã liên kết. Giảm thiểu: xây và test ở Development mode trước.
- **Workers AI** chất lượng tiếng Việt kém hơn Claude: cho phép bot dùng Claude qua AI Gateway bằng khóa provider.
- **Đồng bộ LWW** có thể mất sửa đổi đồng thời: giữ bản xung đột cho ghi chú; các bảng khác chấp nhận LWW.
- **Kích thước bộ cài**: `rusqlite bundled` + notification thêm ~1–2 MB; theo dõi mục tiêu < 15 MB (chưa tính runtime Agent SDK).
- **Windows Application Control** đã chặn binary test Rust trên máy hiện tại: chạy test Rust trên CI.

## Câu hỏi mở (cần chủ dự án quyết định)

1. Dùng một mình hay nhiều người dùng? Plan hiện giả định **một chủ sở hữu**, nhiều thiết bị. Nếu nhiều người dùng cần thêm bảng `users`, phân quyền và thay Cloudflare Access bằng Better Auth.
2. "Bot offline" có phải là trợ lý chạy hoàn toàn cục bộ (như plan) hay là một bot Instagram chạy trên máy?
3. Có cần đồng bộ lịch sử chat lên cloud không (mặc định tắt vì riêng tư)?
4. Có muốn đồng bộ với Google Calendar không? (Chưa nằm trong plan.)

## Quyết định

- Giữ desktop ở root, thêm `shared/`, `cloud/`, `admin/` làm workspaces để không đổi đường dẫn build/CI hiện có.
- "modal ngoài / modal cloudflare" được hiểu là **model** ngoài / **model** Cloudflare.
- SQLite truy cập qua Rust (`rusqlite`), không dùng `tauri-plugin-sql`, để frontend không chạy SQL tùy ý.
- AI chỉ đề xuất hành động, người dùng xác nhận; tool của Agent SDK tiếp tục tắt.


## Tiến độ triển khai 2026-10-28

- P0 đang thực hiện: tạo workspace, schema dùng chung và migration SQLite nền tảng. Chưa kết nối SQLite vào app, chưa nhập lịch sử; P0 chưa hoàn tất. Các phase P1–P8 và Google Calendar chưa triển khai. Schema và giao thức sync hiện là dự thảo, cần hoàn thiện và kiểm tra tương thích trước khi dùng.
- Quyết định: một chủ sở hữu, nhiều thiết bị; bot offline là trợ lý cục bộ; lịch sử chat mặc định không sync. Google Calendar đồng bộ hai chiều qua cloud cho một tài khoản và một lịch được chọn.
- Kiểm tra: `npm run build` đạt trước khi thêm test schema; `npm test` đạt 19 test. `cargo fmt` bị Windows Application Control chặn (os error 4551). `cargo test --locked` không chạy được: Windows Application Control chặn build script của `tauri-plugin-global-shortcut` (os error 4551); migration Rust chưa được kiểm chứng trên máy này.

### Tiếp tục P0

- Đã nối Chat với SQLite qua lệnh Rust; lịch sử JSON được nhập một lần trong transaction, sau đó giữ lại bản sao. Nếu JSON hỏng, dữ liệu gốc được giữ và UI báo lỗi. Workspace `cloud` và `admin` vẫn là khung; các schema sync còn là dự thảo.
- Đã chạy `npm run build`, `npm test` (19 test), `npm run test:ui`, `npm run test:mascot`, `npm run test:settings`: đạt. Bốn test database Rust chạy trong Docker Linux độc lập: đạt. Windows Application Control vẫn chặn chạy build script Rust trên Windows; chưa xác minh nâng cấp trong ứng dụng Windows thật. P1–P8 và Google Calendar chưa triển khai.

- Kết quả cập nhật: `cargo test --locked` trong Docker Linux đạt 9/9 test; `cargo fmt --check` được kiểm tra bằng toolchain Docker. Windows native và dữ liệu nâng cấp thật vẫn chưa kiểm chứng.

### Tiến độ P1

- Đã thêm app shell với sidebar/hash routes, Chat/Dịch/Cài đặt, phím tắt mở app, mục tray/menu và lối mở từ popup. Các route tính năng sau hiển thị trạng thái đang phát triển. Trang chủ tổng hợp dữ liệu và trạng thái mode/sync thực tế còn phụ thuộc P2/P3/P5; P1 chưa hoàn tất toàn bộ.
- Kiểm tra: `npm run build`, `npm test` (19 test), `test:ui`, `test:mascot`, `test:settings` đều đạt; `cargo test --locked` trong Docker Linux đạt 9/9. Chưa kiểm tra cửa sổ, shortcut và nâng cấp trên Windows native.

### Tiếp tục P1: liên kết popup và app shell

- Giữ trang Chat đang chạy khi đổi route; chuyển nội dung soạn và bản dịch từ popup sang app, đồng bộ danh sách hội thoại giữa các cửa sổ và giữ luồng chọn văn bản của popup riêng.
- Kiểm tra: `npm run build`, `npm test` (19 test), `test:ui`, `test:mascot`, `test:settings` đạt; Playwright dùng IPC giả lập kiểm tra chuyển route khi streaming và chuyển bản nháp. `cargo test --locked` trên Docker Linux đạt 9/9. Chưa xác minh Windows native. P1 vẫn cần trang chủ dữ liệu và chỉ báo mode/sync khi P2/P3/P5 có dữ liệu; P2–P8 và Google Calendar chưa thực hiện.

### P2: ghi chú cục bộ

- Đã thêm tạo/sửa/xóa ghi chú, thẻ, ghim, FTS5, Markdown được sanitize, lưu câu trả lời chat và ghi chú ghim trên trang chủ. Bản ghi, hoạt động và outbox được cập nhật trong transaction; dùng rev để từ chối bản sửa cũ.
- Kiểm tra: `npm run build`, `npm test` (19 test), `test:notes`, `test:ui`, `test:mascot` đạt; `cargo test --locked` trên Docker Linux đạt 10/10. Kiểm thử UI dùng IPC giả lập; Windows native chưa kiểm chứng.
- P2 chưa hoàn tất: tóm tắt AI, thói quen, lịch, nhắc nhở, lịch sử hoạt động và đề xuất hành động chưa được triển khai. P3–P8 và Google Calendar vẫn chưa triển khai.

### P2: lịch sử hoạt động cục bộ

- Đã thêm dòng hoạt động cho ghi chú, chat và dịch; lọc loại/ngày, xóa từng mục hoặc theo khoảng ngày bằng tombstone và outbox. Hoạt động chat/dịch không lưu nội dung hội thoại hay văn bản dịch; danh sách hiện tối đa 1.000 mục và có thể thu hẹp khoảng ngày.
- Kiểm tra: build, 19 test JavaScript, `test:ui`, `test:mascot`, `test:settings`, `test:notes`, `test:activity` đạt; 13 test Rust đạt trên Docker Linux. UI dùng IPC giả lập, Windows native chưa kiểm chứng.
- P2 vẫn thiếu thói quen, lịch, nhắc nhở, tóm tắt AI và đề xuất hành động; hoạt động bot chưa có. Đồng bộ và các phase cloud/admin/bot/Google Calendar chưa triển khai.

### P2: thói quen

- Đã thêm thói quen (hằng ngày / ngày trong tuần / N lần mỗi tuần), check-in có thể hoàn tác, streak, heatmap 12 tuần và các lệnh `habits_*`. Check-in lặp lại không tạo bản ghi trùng; xóa thói quen tạo tombstone cho cả log. Mọi thao tác ghi vào bảng, outbox và hoạt động trong một transaction; hoạt động hiển thị trong Lịch sử.
- Kiểm tra: build, `test:habits`, `test:notes`, `test:activity`, `test:ui`, `test:mascot`, `test:settings` đạt; 15 test Rust (gồm streak, ngày/tuần, check-in) đạt trong Docker Linux với crate độc lập chứa module `db`. Chưa chạy `cargo test` đầy đủ hay kiểm chứng trên Windows native. Mascot `happy` khi check-in và thẻ thói quen ở trang chủ chưa làm.
- P2 vẫn thiếu lịch + RRULE, nhắc nhở, tóm tắt AI và đề xuất hành động.

### P2: lịch

- Đã thêm sự kiện và việc cần làm có hạn (đánh dấu hoàn thành), lặp lại, xem tháng/tuần/ngày, các lệnh `events_*`; ghi vào bảng + outbox + hoạt động trong một transaction. Thay vì crate `rrule`, tự cài tập con RRULE: FREQ (DAILY/WEEKLY/MONTHLY/YEARLY), INTERVAL, COUNT, UNTIL, BYDAY (tuần); quy tắc khác bị từ chối khi lưu. Ngày 31/29-2 bị bỏ qua ở tháng/năm không có ngày đó theo RFC 5545. Lặp lại tính theo múi giờ cố định của máy (không theo dõi DST). Cửa sổ truy vấn tối đa 400 ngày, 1.000 lần xuất hiện.
- Kiểm tra: build, `test:calendar`, `test:habits`, `test:notes`, `test:activity`, `test:ui`, `test:mascot`, `test:settings` đạt; 22 test Rust (7 mới: RRULE, cửa sổ, múi giờ, CRUD) đạt trong Docker Linux với crate độc lập. Chưa chạy `cargo test` đầy đủ hay kiểm chứng trên Windows native.
- Chưa làm: sửa/xóa riêng một lần lặp (ngoại lệ), nhắc nhở/thông báo, lịch hôm nay trên trang chủ. P2 còn thiếu nhắc nhở, tóm tắt AI, đề xuất hành động.

### P2: nhắc nhở

- Mỗi sự kiện/việc có thể có nhắc trước 0 phút–7 ngày (bảng `reminders`, một dòng mỗi sự kiện, đồng bộ qua outbox). Scheduler nền (`src-tauri/src/reminders.rs`) quét mỗi 30 giây, gửi thông báo hệ thống bằng `tauri-plugin-notification` và đánh dấu "chờ xử lý"; mỗi lần xuất hiện chỉ nhắc một lần, lần đã trễ quá 15 phút (ví dụ app đang tắt) bị bỏ qua, việc đã hoàn thành không nhắc. Sửa sự kiện sẽ đặt lại trạng thái nhắc.
- Trong app shell có thanh nhắc nhở với "Hoãn 10 phút" (nhắc lại đúng lần xuất hiện đó) và "Đã rõ"; trạng thái chờ xử lý lưu trong DB nên còn sau khi khởi động lại. Múi giờ của UI được lưu (`sync_state.tz_offset`) để scheduler tính lặp lại theo giờ địa phương; chỉ cập nhật khi mở app shell/lịch.
- Kiểm tra: build, 19 test JS, `test:calendar` (mở rộng cho nhắc nhở), `habits`, `notes`, `activity`, `ui`, `mascot`, `settings` đạt; `cargo test` đầy đủ của crate (gồm scheduler biên dịch) 31/31 đạt trong Docker Linux; `Cargo.lock` đã thêm `tauri-plugin-notification`. Chưa kiểm chứng thông báo thật hoặc build trên Windows native.
- Chưa làm: bong bóng/mascot `happy` khi nhắc (mascot chưa có cơ chế bong bóng riêng), nút hành động trên thông báo hệ thống, nhắc cho thói quen. P2 còn thiếu tóm tắt AI, đề xuất hành động, thẻ trang chủ.

### P2: trang chủ

- Trang chủ có ba thẻ: lịch hôm nay (sự kiện và việc, kể cả lần lặp), thói quen chưa check-in với nút check-in nhanh, và ghi chú ghim; tự cập nhật khi `events-changed`, `habits-changed`, `notes-changed`. Không có lệnh Rust mới.
- Kiểm tra: build, `test:home` (mới), `calendar`, `habits`, `notes`, `activity`, `ui`, `mascot`, `settings` đạt (IPC giả lập). Chưa kiểm chứng trên Windows native.
- P2 còn thiếu tóm tắt AI và đề xuất hành động (cần router P3).

### P3: router model và chế độ (phần chat)

- Đã thêm `src-tauri/src/ai/`: `plan()` chọn danh sách backend theo `mode` (Offline: chỉ model có host localhost, từ chối mọi thứ khác; Online: model chính; Hybrid: model chính rồi dự phòng model cục bộ nếu lỗi trước khi có chữ nào được gửi). Backend: Claude Agent SDK (giữ nguyên), OpenAI-compatible streaming SSE (preset OpenAI, OpenRouter, Ollama, LM Studio; HTTPS hoặc localhost, key lấy từ keychain, không lọt vào lỗi), Cloudflare (báo "chưa kết nối cloud" đến P4). Dịch ở chế độ Offline chỉ cho LibreTranslate trên localhost.
- Cài đặt có tab **AI** (chế độ, nguồn model, nhà cung cấp, địa chỉ, key, model cục bộ, liệt kê model qua `ai_models`); thanh trạng thái app shell hiện chế độ và model. `agent::run` nhận `FnMut`.
- Kiểm tra: build, `test:ai` (mới) và các test UI cũ, 19 test JS đạt; `cargo test` đầy đủ 39/39 trong Docker Linux (mới: router theo chế độ, SSE tách chunk giữa ký tự, wiremock cho streaming/lỗi/hủy/liệt kê model). Chưa chạy với Ollama thật hay trên Windows native.
- Chưa làm: model riêng cho `translate`/`summarize` (config hiện chỉ có model chat), dịch bằng model OpenAI-compatible, Cloudflare thật (P4). Tóm tắt AI và đề xuất hành động của P2 giờ có thể dựa trên router này.

### P2: tóm tắt AI và đề xuất hành động

- Ghi chú có nút "Tóm tắt bằng AI" (kết quả hiện riêng; "Chèn vào đầu ghi chú" chỉ điền vào trình sửa, người dùng bấm Lưu mới ghi) và "Gợi ý hành động". Model trả JSON, Rust (`ai/actions.rs`) kiểm tra từng mục (loại note/event/todo/habit, độ dài, ngày giờ hợp lệ, tần suất hợp lệ, tối đa 5; mục sai bị bỏ, không sửa) và UI hiện thẻ "Thêm"/"Bỏ qua"; chỉ khi bấm "Thêm" mới gọi `notes_save`/`events_save`/`habits_save`. Prompt coi văn bản là dữ liệu, không phải mệnh lệnh; tool Agent SDK vẫn tắt. Cả hai đi qua router nên tuân theo chế độ Offline/Online/Hybrid.
- Kiểm tra: build, `test:suggestions` (mới) cùng các test UI cũ, 19 test JS đạt; `cargo test` đầy đủ 42/42 trong Docker Linux (mới: parser đề xuất). Chưa thử với model thật hoặc Windows native.
- P2 coi như hoàn tất ở mức desktop. Còn lại ngoài phạm vi: ngoại lệ một lần lặp trong lịch, mascot `happy`/bong bóng khi nhắc và check-in, nhắc cho thói quen, đề xuất hành động từ cuộc trò chuyện chat.

### P4: backend Cloudflare (phần Worker + ghép nối desktop)

- `cloud/` là Worker Hono: `wrangler.jsonc` (D1, Workers AI, tùy chọn AI Gateway), `migrations/0001_init.sql` (`devices`, `pairing_codes`, `api_keys`, `provider_keys`, `usage`, `rate_limits`). Xác thực: admin bằng JWT Cloudflare Access (RS256, kiểm chữ ký, iss, aud, hạn) hoặc `DEV_ADMIN_TOKEN` chỉ cho máy dev; `POST /v1/pair/start` (admin, mã 8 ký tự hạn 10 phút) → `POST /v1/pair/complete` (một lần, giới hạn 10 lần/10 phút/IP) → token thiết bị `md_…` chỉ lưu hash; khóa `mk_…` có scope và hạn mức ngày, chỉ hiện một lần; thu hồi có hiệu lực ngay.
- AI: `POST /v1/ai/chat` (và alias `/v1/ai/chat/completions`) phát SSE kiểu OpenAI từ Workers AI (danh sách model cho phép, giới hạn kích thước, lỗi upstream được che), `POST /v1/ai/embed`, ghi `usage`, `GET /v1/admin/usage`. Khóa provider mã hóa AES-GCM (IV ngẫu nhiên mỗi giá trị), không có route đọc ngược; chỉ `getProviderKey` dùng nội bộ cho các phase sau.
- Desktop: lệnh `cloud_pair` đổi mã lấy token và lưu vào keychain; backend `Cloudflare` trong router dùng lại `stream_chat` (base `{cloudUrl}/v1/ai`); Offline chỉ chấp nhận Worker chạy trên localhost; Settings → AI có ô địa chỉ, model, ghép nối/ngắt kết nối.
- Kiểm tra: `tsc` cho `cloud` sạch; 16 test Worker (vitest, D1 giả lập bằng `node:sqlite`, AI giả lập) cùng 19 test cũ = 35 đạt; `wrangler deploy --dry-run` đóng gói được; `cargo test` 42/42 trong Docker Linux; build và 10 script UI đạt.
- Khác với plan: test dùng D1 giả lập thay vì `@cloudflare/vitest-pool-workers` (chưa chạy workerd); chưa dùng KV/R2/Vectorize/Queue/Workflows/Durable Object (rate limit đang dùng D1; các binding đó thuộc P7); proxy Anthropic/OpenAI qua AI Gateway chưa làm (khóa provider mới chỉ được lưu); các bảng đồng bộ để P5, bảng Instagram để P7.
- Chưa kiểm chứng: `wrangler dev` với Workers AI thật (cần đăng nhập Cloudflare), triển khai thật, ghép nối thật từ app Windows. Cần chủ dự án tạo D1, đặt `ENCRYPTION_KEY`, cấu hình Access (`docs/cloud-setup.md` thuộc P8).

### P5: đồng bộ online/offline

- Worker: `migrations/0002_sync.sql` (`sync_records`, mỗi bản ghi một dòng, `server_seq` tăng dần làm cursor), `POST /v1/sync/push` (≤ 500 bản ghi, kiểm tra bằng zod của `shared`, trả `accepted`/`rejected`), `GET /v1/sync/pull?cursor=` (phân trang 500, `more`), chỉ cho thiết bị đã ghép nối. Hợp nhất last-write-wins theo `(updated_at, device_id)`, ghi trùng là no-op nên thử lại an toàn; tombstone bị dọn sau 30 ngày bằng cron hằng ngày (thiết bị offline quá 30 ngày phải đồng bộ lại từ đầu); admin có `GET /v1/admin/sync/summary` và `DELETE /v1/admin/sync`.
- Desktop: `db/sync.rs` gom bản ghi mới nhất của mỗi mục trong outbox theo phạm vi (ghi chú / thói quen / lịch + nhắc nhở / hoạt động), gỡ outbox sau khi server nhận, và hợp nhất bản ghi kéo về. Ghi chú thua cuộc (bị server từ chối, hoặc đang chờ gửi mà bản từ xa mới hơn) được giữ thành ghi chú "… (xung đột)" thay vì mất; các bảng khác chấp nhận LWW. Đổi phạm vi sẽ kéo lại từ cursor 0 (áp dụng idempotent) để dữ liệu của phạm vi mới bật không bị lỡ. Vòng lặp `sync/mod.rs`: mỗi 15 giây kiểm tra, gửi ngay khi có thay đổi chờ, kéo ít nhất mỗi 60 giây, backoff mũ tới 15 phút khi lỗi; Offline, chưa có địa chỉ cloud hoặc chưa ghép nối thì tắt hẳn. Lệnh `sync_status`, `sync_now`.
- UI: Cài đặt → tab "Đồng bộ" (trạng thái, số thay đổi chờ, lần cuối, lỗi, "Đồng bộ ngay", chọn phạm vi); thanh trạng thái app shell hiện trạng thái thật.
- Sửa trong lúc làm: truy vấn gom outbox ban đầu có thể gửi nội dung cũ của một ghi chú đã sửa nhiều lần; test hai thiết bị phát hiện và đã sửa bằng truy vấn lấy đúng bản có `rev` lớn nhất.
- Kiểm tra: 6 test Worker mới (LWW, tiebreak, idempotent, tombstone, phân trang, xác thực) + toàn bộ 41 test JS đạt, `tsc` cloud sạch; 48 test Rust đạt trong Docker Linux (gồm mô phỏng hai thiết bị qua một server giả cùng luật: hội tụ, tìm kiếm FTS bên nhận, xóa lan truyền, xung đột, phạm vi, cursor); build và 11 script UI đạt.
- Chưa làm / chưa kiểm chứng: lịch sử trò chuyện chưa có đường đồng bộ (và mặc định không đồng bộ theo yêu cầu); chưa chạy hai bản desktop thật với Worker thật qua mạng (vòng HTTP của `sync/mod.rs` chưa có test tích hợp); trạng thái nhắc nhở (đã bắn/chờ/hoãn) đồng bộ theo LWW nên thiết bị khác thấy cùng trạng thái; chưa có cảnh báo khi thiết bị offline quá 30 ngày.

# Michiyo 🌱

Trợ lý desktop dạng linh vật pixel nguyên bản: chat Claude, dịch văn bản theo ngữ cảnh, cài đặt và tray. Stack: **Tauri 2 / Rust / Svelte 5 / TypeScript / Vite / PixiJS 8**. Ưu tiên Windows 10/11 x64.

## Chạy nhanh

Cần Node.js 22.12+ (đã kiểm tra với Node 24), Rust stable MSVC, Microsoft Edge WebView2 Runtime, **Visual Studio Build Tools 2022: Desktop development with C++ + Windows SDK**. Cài Rust từ [rustup](https://rustup.rs/), Build Tools từ [Microsoft](https://visualstudio.microsoft.com/visual-cpp-build-tools/).

```powershell
npm ci
npm run tauri dev
```

Script `tauri` tự thêm `$HOME/.cargo/bin` vào PATH của tiến trình, không sửa profile shell. Nếu báo `link.exe not found`, hãy cài workload C++ phía trên rồi mở terminal mới. VS Code không cung cấp linker này.

Xem giao diện bằng trình duyệt:

```powershell
npm run dev
# http://127.0.0.1:1420
# http://127.0.0.1:1420/?view=mascot
```

Bản trình duyệt là preview giao diện, không giả lập kết quả AI; keychain, mạng, clipboard, tray và shortcut cần desktop.

## Sử dụng

- Mascot luôn trên cùng, nền trong suốt, không xuất hiện trên taskbar. Kéo nhân vật để di chuyển và thả ở vị trí bất kỳ; vị trí được lưu nguyên, không tự hít vào cạnh màn hình. Các phép tính vị trí dùng tọa độ vật lý và work area, kích thước cửa sổ dùng đơn vị logic để hỗ trợ DPI. Menu chuột phải là cửa sổ custom nhỏ gọn, hỗ trợ phím mũi tên/Enter/Escape và tự ẩn khi mất focus.
- Chuột trái vào mascot mở bong bóng nhỏ ngay cạnh nhân vật: nhập trực tiếp và xem câu trả lời streaming tại đó. Thiếu key sẽ báo trong bong bóng, không tự mở Cài đặt. Chuột phải mở menu Trò chuyện, Dịch văn bản, Cài đặt, Ẩn mascot và Thoát. Kéo bằng chuột trái để di chuyển. Tray có Hiện/Ẩn, Cài đặt, Thoát. Đóng bong bóng sẽ ẩn cửa sổ, giữ tác vụ/hội thoại.
- **Ctrl+Alt+T**: bôi đen văn bản ở app khác rồi nhấn để dịch. Popup tự mở cạnh con trỏ, đổi phía và giới hạn trong màn hình. **Ctrl+Alt+C** mở Chat; **Ctrl+Alt+M** hiện/ẩn mascot. Đổi phím ở Cài đặt → Phím tắt.
- Cài đặt → API: nhập khóa, **Lưu key**, sau đó Kiểm tra kết nối. Ô trống + Lưu key xóa khóa đã lưu. Khóa đã lưu không được đọc ngược lên UI; chỉ trả cờ có/chưa có khóa. Giá trị vừa nhập chỉ tồn tại tạm trong ô nhập và được xóa sau khi lưu.
- Chat hỗ trợ markdown được sanitize, code block, sao chép câu trả lời, dừng stream, hội thoại mới và xóa hội thoại hiện tại. Enter gửi, Shift+Enter xuống dòng. Model mặc định `claude-haiku-5-5`, có thể chọn `claude-sonnet-5-5`; quyền dùng model phụ thuộc tài khoản Claude.
- Dịch mặc định MyMemory, không cần key; LibreTranslate, Google Cloud v2 và Claude được chọn theo thứ tự trong Cài đặt. Fallback có thể tắt. Nút Engine khác dịch lại riêng với engine đã chọn; Hỏi thêm chuyển nguồn và kết quả sang ô soạn Chat.
- Ngôn ngữ được phát hiện cục bộ bằng `whatlang`; nguồn trùng ngôn ngữ đích thì dùng đích thay thế (mặc định Việt → Anh, còn lại → Việt). Đoạn rất ngắn hoặc nhiều ngôn ngữ có thể nhận diện sai: chọn nguồn thủ công. Lĩnh vực/giọng văn được gửi trong system prompt của provider Claude.
- Dịch khi copy mặc định **tắt**. Khi bật, văn bản clipboard mới sẽ gửi tới engine đã chọn. Clipboard nguồn và bản dịch là nội dung cá nhân; chỉ bật khi phù hợp với công việc.

## Build và kiểm tra

```powershell
npm run build
npm test
npm run build:agent
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --manifest-path src-tauri/Cargo.toml --locked
npm run tauri build
```

Bộ cài NSIS ở `src-tauri/target/release/bundle/nsis/`. Máy đích cần WebView2. Bộ cài chưa ký số; ký bằng chứng chỉ của bên phân phối khi phát hành. Workflow `.github/workflows/check.yml` build/test trên Windows và giữ artifact bộ cài; workflow chưa được chạy từ phiên làm việc này.

Kiểm tra giao diện trên Chrome đã cài (mở `npm run dev` ở terminal khác):

```powershell
npm run test:ui
```

Script kiểm tra nhập Chat, các tab/API key ẩn, phím tắt, lỗi khi gọi native từ preview và canvas Pixi. Ảnh QA nằm trong `artifacts/` (không commit).

**Trạng thái kiểm chứng trên máy triển khai:** build frontend, kiểm tra Svelte/TypeScript và 12 test frontend/Agent SDK đạt. `cargo check` đạt; `cargo test --locked` biên dịch được binary test, nhưng Windows Application Control chặn thực thi binary mới (os error 4551), nên chưa xác nhận các test Rust mới chạy thành công. `npm run test:mascot` kiểm tra bằng Chromium + IPC giả lập: phân biệt chuột trái/phải, không tràn viewport ở DPI 125%, focus ô nhập và hiện câu trả lời trong bong bóng. Kiểm tra này không thay cho native smoke test về trong suốt, menu hệ điều hành và nhiều màn hình. Chưa tạo bộ cài release; không có API key nên chưa kiểm tra chat/dịch có phí với dịch vụ thật.

## Kiến trúc

```text
src/
  mascot/          Pixi, animation, state machine và test
  chat/            Chat UI, markdown, lịch sử, streaming events
  translate/       Popup dịch, copy, hỏi thêm
  settings/        6 tab cài đặt
  lib/             IPC, cấu hình mặc định, i18n, validation
src-tauri/src/
  commands/        Chat, dịch, shortcut, cửa sổ, capture, nạp skin
  providers/       Translator trait, 4 engine, Rust ↔ Agent SDK bridge, HTTP mock tests
  config/          tauri-plugin-store
  secrets/         OS keychain qua keyring
agent/             Worker TypeScript gọi Claude Agent SDK + test
public/skins/      Skin mẫu nguyên bản
scripts/           Sinh sprite, Tauri wrapper, UI verification
```

Claude chạy qua **Claude Agent SDK / Claude Code** chính thức (`@anthropic-ai/claude-agent-sdk`, cố định phiên bản trong lockfile), không còn gọi Messages API bằng reqwest. Luồng: Svelte → Rust command → Node worker qua stdin/stdout riêng → `query()` của SDK → Claude Code native. Chat bật `includePartialMessages: true`, chỉ chuyển `text_delta` về UI, hoàn tất ở `result`; không phát lặp bản trả lời đầy đủ. Dịch Claude và Kiểm tra kết nối dùng cùng SDK. Nút Kiểm tra kết nối Claude tạo một lượt hỏi ngắn, có thể phát sinh phí API.

Khóa lấy từ OS keychain, chỉ chuyển qua pipe riêng tới worker, không đưa vào command line hay trả về WebView. CLI nhận khóa bằng environment riêng. Worker không kế thừa credential/base URL/debug environment của Claude Code ngoài app. `tools: []`, `allowedTools: []`, `skills: []`, `settingSources: []`, không MCP/plugin/hook, quyền tool luôn từ chối; không cấp quyền đọc file/chạy lệnh cho mô hình. Mỗi yêu cầu dùng thư mục tạm riêng, `persistSession: false`; ngữ cảnh chat lấy từ lịch sử của Michiyo, không dùng phiên Claude Code bên ngoài. Đây là giới hạn tính năng ở SDK, không phải sandbox hệ điều hành.

Timeout SDK 120 giây (Rust có deadline 130 giây). Stop gửi abort qua pipe; Windows Job Object dọn cả worker lẫn CLI con khi tiến trình kết thúc, tránh request mồ côi. SDK có thể có chính sách retry nội bộ riêng; app không tự tạo query mới sau khi đã có token. Không trả lỗi thô/stack trace của SDK lên frontend.

MyMemory/LibreTranslate/Google vẫn dùng Rust reqwest: timeout tổng 60 giây, connect timeout 10 giây, không theo redirect mang khóa sang host khác, retry 1 lần cho lỗi mạng/429/5xx. LRU 128 bản dịch nằm trong RAM; cache key gồm văn bản, cặp ngôn ngữ, cấu hình và engine.

`npm run build:agent` bundle worker và chuẩn bị Node + Claude Code runtime trong `src-tauri/resources/claude-agent/` (gitignored). Lệnh build/dev của Tauri tự chuẩn bị runtime; build riêng bằng Cargo cần chạy bước này trước. Bản cài đóng gói runtime vào resources, người dùng cuối không cần tự cài Node. Có license Node/Claude kèm theo. Xem [Agent SDK](https://code.claude.com/docs/en/agent-sdk/overview) và [streaming](https://code.claude.com/docs/en/agent-sdk/streaming-output). Luồng hiện tại vẫn dùng API key đã lưu trong app, không tích hợp đăng nhập claude.ai/Pro/Max.

Frontend không gọi mạng provider. CSP production chỉ cho tài nguyên nội bộ, ảnh data/blob và IPC; dev thêm WebSocket của Vite. Markdown được DOMPurify sanitize. Dữ liệu cấu hình không chứa API key. Windows dùng Credential Manager, macOS dùng Keychain; lỗi keychain được báo thay vì lưu plaintext.

`settings.json`, `history.json`, `skins/` nằm trong thư mục app data của `app.michiyo.desktop` (Windows thường `%APPDATA%/app.michiyo.desktop`). Lịch sử và cấu hình được lưu cục bộ dạng JSON, không mã hóa; dùng nút Xóa để xóa hội thoại nhạy cảm. Khóa API nằm riêng trong OS keychain.

Phím tắt được parse, kiểm tra trùng và đăng ký với hệ điều hành. Nếu đăng ký thất bại, phục hồi bộ phím cũ; frontend hoàn tác cấu hình đã lưu. Thay đổi áp dụng ngay. Cửa sổ Chat/Dịch/Cài đặt được tạo khi dùng lần đầu, tránh mở sẵn nhiều WebView lúc idle.

## Tạo skin

Skin gồm hai file, không cần sửa code:

```text
<app-data>/skins/my-gardener/
  manifest.json
  spritesheet.png
```

Vào Cài đặt → Giao diện để xem đường dẫn, nhấn **Quét skin**, chọn skin. Tên thư mục không chứa dấu chấm hoặc dấu phân cách đường dẫn. PNG tối đa 8 MB; manifest tối đa 64 KB. Sprite sheet chia đều theo `frameWidth`/`frameHeight`, đánh số từ trái sang phải, từ trên xuống dưới:

```json
{
  "name": "My gardener",
  "frameWidth": 32,
  "frameHeight": 32,
  "states": {
    "idle": { "frames": [0, 1], "fps": 8 },
    "hover": { "frames": [2], "fps": 8 },
    "listening": { "frames": [3], "fps": 8 },
    "thinking": { "frames": [4, 5], "fps": 8 },
    "talking": { "frames": [6, 7], "fps": 10 },
    "happy": { "frames": [8], "fps": 8 },
    "error": { "frames": [9], "fps": 8 },
    "sleep": { "frames": [10], "fps": 1 }
  }
}
```

Cần đủ tám state; frame index phải nằm trong PNG. Skin không hợp lệ fallback về skin mặc định. Bản mặc định do `scripts/create-skin.py` vẽ bằng Pillow, không dựa trên nhân vật có bản quyền. Có thể tái sinh bằng `python scripts/create-skin.py` sau khi cài Pillow. Font tiêu đề VT323 được đóng gói cục bộ, có license OFL trong `public/fonts/`; nội dung dùng font hệ thống hỗ trợ tiếng Việt.

Renderer dùng nearest sampling, nền alpha, sprite 8–12 FPS, tween opacity ngắn bằng requestAnimationFrame, chớp mắt ngẫu nhiên và nhìn theo chuột ở skin mẫu. Reduced motion tôn trọng cả thiết lập và `prefers-reduced-motion`; sleep giảm còn 1 FPS. Ẩn mascot hoặc document hidden dừng render. Hit test dùng alpha mask 160×160 của frame hiện tại và ánh xạ theo DPI/scale; Rust kiểm tra con trỏ để có thể bắt chuột lại sau khi bật xuyên chuột. Đây là polling 60 ms, cần kiểm tra độ trễ khi thao tác nhanh trên máy đích.

## Giới hạn hiện tại và macOS

- **Live2D là tùy chọn nâng cao chưa triển khai**, control đang khóa và ghi rõ trạng thái. Không đưa `pixi-live2d-display` phụ thuộc Pixi đời cũ vào renderer Pixi v8. Khi bổ sung cần canvas/adapter riêng tương thích, người dùng cung cấp runtime Cubism và model hợp pháp; tuân thủ license Cubism SDK/model, không tự phân phối SDK độc quyền.
- MyMemory giới hạn 500 byte mỗi đoạn; đoạn dài hơn nhận thông báo và chuyển sang engine tiếp theo nếu bật fallback. MyMemory có quota; public LibreTranslate có thể yêu cầu key, bản self-hosted có thể không cần. Không có cam kết các instance công cộng luôn miễn phí hoặc sẵn sàng.
- Capture bằng giả lập Copy chỉ hoạt động khi app nguồn cho phép copy, và app chạy quyền user không tự đọc được cửa sổ elevated. Clipboard cũ được phục hồi cho **text hoặc ảnh**; rich HTML, RTF, file list và định dạng clipboard riêng của ứng dụng chưa được giữ nguyên. Không có selection sẽ báo lỗi thay vì dịch lại nội dung clipboard cũ.
- Clipboard watcher hiện polling 900 ms khi bật. Tự dịch clipboard có thể gặp văn bản mới do chính app khác ghi vào; không suy đoán ngữ cảnh tài liệu ngoài phần đã chọn. Hướng dịch dùng mô hình nhận diện ngôn ngữ cục bộ nên có sai số với đoạn ngắn.
- UI có Việt/Anh cho nhãn thao tác chính; thông báo lỗi backend và một số nội dung trợ giúp hiện dùng tiếng Việt. i18n nằm trong `src/lib/i18n.ts`.
- Mục tiêu installer <15 MB **không còn phù hợp sau khi chuyển Agent SDK**: riêng Claude Code Windows CLI hiện khoảng 297 MB và Node khoảng 92 MB (chưa nén). Kích thước bộ cài release chưa đo. Worker/CLI chỉ chạy khi gọi Claude, không giữ daemon lúc idle. Mục tiêu RAM idle <80 MB và CPU gần 0% **chưa được đo/xác nhận**. Cần đo trên release, tính cả tiến trình WebView2. Khởi động chỉ có mascot; panel đã mở được giữ để bảo toàn hội thoại khi đóng. WebGL/WebView2 và số panel ảnh hưởng mạnh tới RAM.
- macOS là giai đoạn sau, chưa kiểm tra. Cần quyền **Accessibility** cho Enigo giả lập Cmd+C; phải xử lý thêm quyền và hành vi app đang focus. Cửa sổ trong suốt cần cấu hình `macos-private-api` của Tauri, ảnh hưởng phân phối App Store. Đổi bundle target sang `.app`/`.dmg`, cấu hình signing/notarization và kiểm tra lại tray, shortcut, keychain, nhiều màn hình.

Trước khi phát hành Windows, kiểm tra native: kéo/thả và click-through trên hai màn hình DPI khác nhau; vị trí sau khi rút màn hình; shortcut xung đột; clipboard text/ảnh; autostart và tray; keychain sau khởi động lại; dừng stream, mất mạng, 401/429 và fallback; đo release RAM/CPU/bộ cài. Đây là các kiểm chứng còn thiếu, không phải kết quả đã đạt.

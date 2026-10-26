# Vai trò
Bạn là senior desktop engineer, giỏi Tauri, Rust, TypeScript và animation 2D. Hãy xây dựng hoàn chỉnh app dưới đây, chạy được ngay, code sạch, có README hướng dẫn build.

# Mục tiêu
App "trợ lý cá nhân" dạng linh vật (mascot) nhỏ nằm ở góc màn hình laptop, luôn hiển thị trên cùng. Giai đoạn 1 chỉ có 2 tính năng: **Chat** và **Dịch theo ngữ cảnh**. Có trang **Cài đặt**.

# Tech stack (bắt buộc, ưu tiên nhẹ)
- **Tauri 2** (Rust backend) + **TypeScript + Vite** frontend, framework UI: **Svelte** (nhẹ, ít runtime).
- Render mascot: **PixiJS v8** với sprite sheet pixel art (`scaleMode: 'nearest'`, không làm mờ pixel).
- Tùy chọn nâng cao: hỗ trợ model **Live2D** qua `pixi-live2d-display` (bật/tắt trong cài đặt; tuân thủ license Live2D Cubism SDK).
- Lưu cấu hình: `tauri-plugin-store`. Lưu API key: **OS keychain** (crate `keyring`), KHÔNG lưu plaintext.
- Plugin Tauri: `global-shortcut`, `clipboard-manager`, `autostart`, `tray-icon`, `single-instance`.
- Mục tiêu: RAM khi idle < 80MB, bộ cài < 15MB, CPU idle gần 0% (animation giảm FPS khi idle).

# Cửa sổ & hành vi mascot
- Cửa sổ mascot: không viền (`decorations: false`), nền trong suốt, `alwaysOnTop`, `skipTaskbar`, kích thước khoảng 160×160.
- Mặc định neo góc dưới phải; kéo thả được; nhớ vị trí; tự hít vào cạnh màn hình gần nhất; hỗ trợ đa màn hình và DPI cao.
- Click-through vùng trong suốt (chỉ bắt chuột trên pixel của mascot).
- Click mascot → mở bong bóng/panel chat (cửa sổ thứ hai, gắn cạnh mascot, tự đổi hướng mở nếu sát mép màn hình).
- Icon khay hệ thống: Hiện/Ẩn, Cài đặt, Thoát. Tùy chọn khởi động cùng hệ thống.

# Animation (phong cách anime pixel)
- State machine cho mascot: `idle` (thở, chớp mắt ngẫu nhiên), `hover` (nhìn theo chuột), `listening` (khi đang gõ), `thinking` (khi chờ API), `talking` (khi stream câu trả lời), `happy`, `error`, `sleep` (sau N phút không tương tác).
- Chuyển state có tween mượt (easing), không giật. Animation pixel ~8–12 FPS cho đúng chất pixel, di chuyển/tween 60 FPS.
- Kiến trúc tách asset: mỗi "skin" là 1 thư mục gồm `spritesheet.png` + `manifest.json` (khai báo frame, state, FPS). Người dùng thả thư mục skin mới vào là dùng được.
- Tạo sẵn 1 skin mẫu bằng pixel art đơn giản do bạn tự vẽ (có thể sinh bằng code/canvas), nhân vật nguyên bản, không dùng nhân vật có bản quyền.
- Tôn trọng `prefers-reduced-motion`; dừng render khi cửa sổ bị ẩn.

# Tính năng 1: Chat
- Panel chat gọn, phong cách pixel (font pixel như "Press Start 2P" hoặc "VT323" cho tiêu đề, font dễ đọc hỗ trợ tiếng Việt cho nội dung).
- Gọi **Claude API** (Messages API, streaming SSE) từ **Rust backend**, frontend không bao giờ chạm vào API key.
- Model cấu hình được, mặc định `claude-haiku-5-5`, lựa chọn thêm `claude-sonnet-5-5`.
- Hiển thị markdown + code block, nút copy, nút dừng sinh, xóa hội thoại, lịch sử lưu cục bộ (giới hạn N hội thoại).
- System prompt tùy chỉnh trong Cài đặt.
- Nếu chưa có API key → mascot hiện trạng thái nhắc và nút mở Cài đặt.

# Tính năng 2: Dịch theo ngữ cảnh
- **Phím tắt toàn cục** (mặc định `Ctrl+Alt+T`, đổi được): lấy text đang bôi đen ở bất kỳ app nào (giả lập Ctrl+C, đọc clipboard, rồi khôi phục clipboard cũ) → hiện popup dịch cạnh con trỏ chuột.
- Tùy chọn: dịch khi copy (theo dõi clipboard), mặc định TẮT.
- **Tự phát hiện ngôn ngữ**: Việt → Anh, ngôn ngữ khác → Việt (cặp ngôn ngữ cấu hình được).
- Popup dịch: bản dịch, nút copy, nút "dịch lại bằng engine khác", nút "giải thích/ hỏi thêm" chuyển sang Chat.
- "Theo ngữ cảnh": khi dùng Claude, gửi kèm chủ đề/lĩnh vực (ví dụ: HR, tuyển dụng, kỹ thuật) và giọng văn (trang trọng/thân mật) cấu hình trong Cài đặt, yêu cầu Claude chỉ trả về bản dịch, giữ thuật ngữ và định dạng.
- Kiến trúc **provider** chung (trait trong Rust: `translate(text, src, tgt, ctx) -> Result`), có các provider:
  1. **Miễn phí, không cần key**: MyMemory API (mặc định), thêm LibreTranslate (URL instance + key tùy chọn).
  2. **Google Cloud Translation API** (v2, cần API key).
  3. **Claude API** (dùng chung key với Chat).
- Thứ tự ưu tiên + **fallback tự động** cấu hình được (ví dụ: Claude → Google → miễn phí). Hiển thị nhỏ engine đã dùng.
- Cache bản dịch gần đây (LRU) để tránh gọi lại.

# Trang Cài đặt (cửa sổ riêng, giao diện pixel thống nhất)
Các tab:
- **Chung**: ngôn ngữ giao diện (Việt/Anh), khởi động cùng hệ thống, vị trí mặc định, kích thước mascot (1x/2x/3x), độ trong suốt.
- **API**: Claude API key, Google API key, LibreTranslate URL/key, mỗi mục có nút "Kiểm tra kết nối" và ẩn/hiện key.
- **Chat**: model, system prompt, max tokens, số hội thoại lưu.
- **Dịch**: cặp ngôn ngữ, thứ tự engine + fallback, lĩnh vực/giọng văn, dịch khi copy (on/off).
- **Phím tắt**: đổi phím tắt dịch, mở chat, ẩn/hiện mascot; phát hiện xung đột.
- **Giao diện**: chọn skin, bật Live2D, tốc độ animation, giảm chuyển động, thời gian vào trạng thái ngủ.
- Thay đổi áp dụng ngay, không cần khởi động lại; có nút khôi phục mặc định.

# Yêu cầu kỹ thuật
- Mọi request mạng đi qua Rust (`reqwest`), có timeout, retry nhẹ, thông báo lỗi thân thiện (mascot chuyển `error`).
- CSP chặt; frontend chỉ gọi `invoke` tới command Rust.
- Hỗ trợ Windows 10/11 trước, macOS sau (ghi rõ chỗ khác biệt: quyền Accessibility để đọc text bôi đen trên macOS).
- Cấu trúc thư mục rõ ràng: `src-tauri/src/{providers,commands,config,secrets}`, `src/{mascot,chat,translate,settings,lib}`.
- Có unit test cho provider dịch (mock HTTP) và state machine mascot.

# Thứ tự thực hiện
1. Khung Tauri + cửa sổ mascot trong suốt, kéo thả, tray.
2. PixiJS + state machine + skin mẫu.
3. Trang Cài đặt + lưu cấu hình + keychain.
4. Provider dịch + phím tắt toàn cục + popup dịch.
5. Chat với Claude streaming.
6. Hoàn thiện, test, README (cài đặt, build, cách tạo skin mới).

Sau mỗi bước, chạy được và tóm tắt ngắn những gì đã làm trước khi sang bước tiếp.
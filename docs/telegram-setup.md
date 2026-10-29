# Telegram webhook thử nghiệm

Bot hỗ trợ /start và trả lời AI trong chat riêng được cho phép. Flow builder và xử lý dead-letter do lỗi lưu database chưa hoàn tất; cần kiểm thử thật trước production.

1. Tạo bot thử nghiệm bằng BotFather và giữ token bot trong kho secret; không commit, không ghi token vào log hay chia sẻ URL Bot API chứa token.
2. Tạo D1 mới và trống, queue `michiyo-telegram` và dead-letter queue `michiyo-telegram-dead`; cấu hình ID D1, Access và các binding trong `cloud/wrangler.jsonc`. Áp dụng baseline trước khi triển khai. Không áp dụng baseline lên D1 cũ; các thiết bị phải ghép nối lại.
3. Đặt token bot bằng secret `TELEGRAM_BOT_TOKEN` và cấu hình `TELEGRAM_ALLOWED_CHAT_IDS` bằng các ID chat riêng cách nhau bởi dấu phẩy. Danh sách trống tắt trả lời. Giới hạn mặc định là 20 lượt AI mỗi chat mỗi giờ.
4. Tạo một secret ngẫu nhiên mạnh gồm 1–256 ký tự `A-Z`, `a-z`, `0-9`, `_` hoặc `-`; đặt cùng giá trị vào secret Worker `TELEGRAM_WEBHOOK_SECRET` và tham số `secret_token` của Bot API `setWebhook`. Webhook URL là `https://<worker-host>/v1/telegram/webhook`.
5. Cấu hình cron theo `wrangler.jsonc`, kiểm tra update thử nghiệm đến inbox và kiểm tra queue/dead-letter trên dashboard. Worker chỉ trả 200 sau khi queue nhận update.

Inbox chứa payload Telegram gốc, có thể có dữ liệu cá nhân; cron xóa sau 30 ngày cộng tối đa một chu kỳ. Queue chết có chính sách lưu riêng của Cloudflare và cần được vận hành riêng. Admin cho phép xem metadata các update lỗi và thử lại lỗi trước khi gửi; không trả payload qua API quản trị.

Tham khảo [Telegram Bot API: setWebhook](https://core.telegram.org/bots/api#setwebhook) và [Update](https://core.telegram.org/bots/api#update).

Mỗi update chỉ bắt đầu gửi tối đa một lần. Khi gửi thất bại hoặc không nhận được xác nhận, trạng thái `sending` cần được kiểm tra thủ công; không tự gửi lại vì Telegram có thể đã nhận. Lỗi AI trước khi gửi được queue thử lại.

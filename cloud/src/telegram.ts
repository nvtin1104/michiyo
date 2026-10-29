import type { Env } from './env';
import { allowedModels, DEFAULT_MODEL } from './env';

const MAX_BYTES = 96_000;
const encoder = new TextEncoder();

function same(a: string, b: string): boolean {
  const x = encoder.encode(a), y = encoder.encode(b);
  let difference = x.length ^ y.length;
  for (let i = 0; i < Math.max(x.length, y.length); i++) difference |= (x[i] ?? 0) ^ (y[i] ?? 0);
  return difference === 0;
}

async function readBounded(request: Request): Promise<Uint8Array | null> {
  if (!request.body) return new Uint8Array();
  const reader = request.body.getReader();
  const chunks: Uint8Array[] = [];
  let length = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    length += value.length;
    if (length > MAX_BYTES) { await reader.cancel(); return null; }
    chunks.push(value);
  }
  const bytes = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length; }
  return bytes;
}

export async function receiveWebhook(request: Request, env: Env): Promise<Response> {
  if (!env.TELEGRAM_WEBHOOK_SECRET || !env.TELEGRAM_QUEUE) return new Response('Unavailable', { status: 503 });
  if (!same(request.headers.get('X-Telegram-Bot-Api-Secret-Token') ?? '', env.TELEGRAM_WEBHOOK_SECRET))
    return new Response('Forbidden', { status: 403 });
  const bytes = await readBounded(request);
  if (!bytes) return new Response('Payload too large', { status: 413 });
  let payload: string, updateId: number;
  try {
    payload = new TextDecoder('utf-8', { fatal: true, ignoreBOM: false }).decode(bytes);
    const update: unknown = JSON.parse(payload);
    if (!update || typeof update !== 'object' || !Number.isSafeInteger((update as { update_id?: unknown }).update_id) || (update as { update_id: number }).update_id < 0) throw new Error('invalid');
    updateId = (update as { update_id: number }).update_id;
  } catch { return new Response('Invalid update', { status: 400 }); }
  try { await env.TELEGRAM_QUEUE.send({ updateId, payload, receivedAt: Date.now() }); }
  catch { console.error('telegram_enqueue_failed'); return new Response('Unavailable', { status: 503 }); }
  return new Response('OK', { status: 200 });
}

export async function storeDelivery(env: Env, delivery: { updateId: number; payload: string; receivedAt: number }): Promise<void> {
  await env.DB.prepare('INSERT OR IGNORE INTO telegram_webhook_inbox (update_id, received_at, payload) VALUES (?, ?, ?)')
    .bind(delivery.updateId, delivery.receivedAt, delivery.payload).run();
}

/** At most one send is attempted per update. An uncertain send is retained for operator review. */
export async function processDelivery(env: Env, delivery: { updateId: number; payload: string; receivedAt: number }): Promise<void> {
  await storeDelivery(env, delivery);
  if (!env.TELEGRAM_BOT_TOKEN || !env.TELEGRAM_ALLOWED_CHAT_IDS?.trim()) return;
  const now = Date.now();
  // Processing may be retried after a worker crash, but never after a send begins.
  const claimed = await env.DB.prepare(`UPDATE telegram_webhook_inbox SET status = 'processing', updated_at = ?
    WHERE update_id = ? AND (status = 'received' OR (status = 'processing' AND updated_at < ?)) RETURNING payload`)
    .bind(now, delivery.updateId, now - 120_000).first<{ payload: string }>();
  if (!claimed) return;
  let message: { text?: unknown; chat?: { id?: unknown; type?: unknown } } | undefined;
  try { message = JSON.parse(claimed.payload).message; } catch { /* malformed inbox entry */ }
  const chat = message?.chat;
  const permitted = new Set(env.TELEGRAM_ALLOWED_CHAT_IDS.split(',').map((s) => s.trim()).filter((s) => /^-?\d+$/.test(s)));
  const chatId = Number.isSafeInteger(chat?.id) ? String(chat?.id) : '';
  const input = typeof message?.text === 'string' ? message.text.trim() : '';
  if (chat?.type !== 'private' || !permitted.has(chatId) || !input || input.length > 4000) {
    await env.DB.prepare("UPDATE telegram_webhook_inbox SET status = 'ignored', updated_at = ? WHERE update_id = ? AND status = 'processing'")
      .bind(Date.now(), delivery.updateId).run();
    return;
  }
  let reply: string;
  try {
    if (/^\/start(?:@\w+)?(?:\s|$)/i.test(input)) reply = 'Xin chào! Tôi là Michiyo. Hãy gửi câu hỏi để bắt đầu.';
    else {
      const bucket = `telegram:${chatId}:${Math.floor(Date.now() / 3_600_000)}`;
      const quota = await env.DB.prepare(`INSERT INTO rate_limits (bucket, count, expires_at) VALUES (?, 1, ?)
        ON CONFLICT(bucket) DO UPDATE SET count = count + 1 WHERE count < 20 RETURNING count`)
        .bind(bucket, Date.now() + 7_200_000).first<{ count: number }>();
      if (!quota) {
        await env.DB.prepare("UPDATE telegram_webhook_inbox SET status = 'ignored', updated_at = ? WHERE update_id = ? AND status = 'processing'")
          .bind(Date.now(), delivery.updateId).run();
        return;
      }
      const model = env.DEFAULT_CHAT_MODEL || DEFAULT_MODEL;
      if (!allowedModels(env).includes(model)) throw new Error('invalid model');
      const result = await env.AI.run(model as keyof AiModels, {
        messages: [{ role: 'system', content: 'Bạn là Michiyo, trợ lý thân thiện và ngắn gọn.' }, { role: 'user', content: input }],
        max_tokens: 512, stream: false,
      } as never, env.AI_GATEWAY_ID ? { gateway: { id: env.AI_GATEWAY_ID } } : undefined) as { response?: string };
      reply = typeof result.response === 'string' && result.response.trim() ? result.response.slice(0, 4000) : 'Xin lỗi, tôi chưa thể trả lời.';
    }
  } catch {
    await env.DB.prepare("UPDATE telegram_webhook_inbox SET status = 'received', updated_at = ? WHERE update_id = ? AND status = 'processing'")
      .bind(Date.now(), delivery.updateId).run();
    throw new Error('telegram_processing_failed');
  }
  const ready = await env.DB.prepare("UPDATE telegram_webhook_inbox SET status = 'sending', updated_at = ? WHERE update_id = ? AND status = 'processing' RETURNING update_id")
    .bind(Date.now(), delivery.updateId).first<{ update_id: number }>();
  if (!ready) return;
  try {
    const response = await fetch(`https://api.telegram.org/bot${env.TELEGRAM_BOT_TOKEN}/sendMessage`, {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ chat_id: chatId, text: reply }), signal: AbortSignal.timeout(10_000),
    });
    const result = await response.json().catch(() => null) as { ok?: boolean } | null;
    if (!response.ok || !result?.ok) throw new Error('telegram_send_failed');
    await env.DB.prepare("UPDATE telegram_webhook_inbox SET status = 'sent', updated_at = ? WHERE update_id = ? AND status = 'sending'")
      .bind(Date.now(), delivery.updateId).run();
  } catch {
    // Telegram might have accepted the request before a timeout. Do not resend automatically.
    console.error('telegram_send_uncertain');
  }
}

/** Inbox payloads are retained for at most 30 days plus the cron interval. */
export async function purgeExpiredDeliveries(env: Env, now = Date.now()): Promise<void> {
  await env.DB.prepare('DELETE FROM telegram_webhook_inbox WHERE received_at < ?').bind(now - 30 * 86_400_000).run();
  await env.DB.prepare('DELETE FROM rate_limits WHERE expires_at < ?').bind(now).run();
}

/** Re-enqueue pending or interrupted processing; never re-send ambiguous outbound requests. */
export async function recoverPendingDeliveries(env: Env, now = Date.now()): Promise<void> {
  if (!env.TELEGRAM_QUEUE || !env.TELEGRAM_BOT_TOKEN || !env.TELEGRAM_ALLOWED_CHAT_IDS?.trim()) return;
  const { results } = await env.DB.prepare(`SELECT update_id, payload, received_at FROM telegram_webhook_inbox
    WHERE status IN ('received', 'processing') AND updated_at < ? ORDER BY received_at LIMIT 100`)
    .bind(now - 120_000).all<{ update_id: number; payload: string; received_at: number }>();
  for (const row of results) await env.TELEGRAM_QUEUE.send({ updateId: row.update_id, payload: row.payload, receivedAt: row.received_at });
}

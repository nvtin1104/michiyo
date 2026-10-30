import { newId, TelegramBotConfig, type Event, type Habit, type Note } from '../../shared/src';
import type { Env } from './env';
import { allowedModels, DEFAULT_MODEL } from './env';
import { push } from './sync';

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

function envAllowedChatIds(env: Env): string[] {
  return (env.TELEGRAM_ALLOWED_CHAT_IDS ?? '')
    .split(',')
    .map((s) => s.trim())
    .filter((s) => /^-?\d+$/.test(s));
}

export async function getBotConfig(env: Env): Promise<TelegramBotConfig & { updated_at: number }> {
  const row = await env.DB.prepare(
    'SELECT enabled, allowed_chat_ids, system_prompt, model, hourly_limit, allow_workspace_queries, updated_at FROM telegram_config WHERE id = 1',
  ).first<{
    enabled: number;
    allowed_chat_ids: string;
    system_prompt: string;
    model: string;
    hourly_limit: number;
    allow_workspace_queries: number;
    updated_at: number;
  }>();
  if (!row) {
    return {
      ...TelegramBotConfig.parse({
        allowed_chat_ids: envAllowedChatIds(env),
        model: env.DEFAULT_CHAT_MODEL || DEFAULT_MODEL,
      }),
      updated_at: 0,
    };
  }
  let parsedIds: string[] = [];
  try {
    const raw = JSON.parse(row.allowed_chat_ids);
    if (Array.isArray(raw)) parsedIds = raw.filter((x): x is string => typeof x === 'string' && /^-?\d+$/.test(x));
  } catch { /* fallback */ }
  const effectiveIds = parsedIds.length > 0 ? parsedIds : envAllowedChatIds(env);
  return {
    enabled: Boolean(row.enabled),
    allowed_chat_ids: effectiveIds,
    system_prompt: row.system_prompt || 'Bạn là Michiyo, trợ lý thân thiện và ngắn gọn.',
    model: row.model || env.DEFAULT_CHAT_MODEL || DEFAULT_MODEL,
    hourly_limit: row.hourly_limit || 20,
    allow_workspace_queries: Boolean(row.allow_workspace_queries),
    updated_at: row.updated_at,
  };
}

export async function saveBotConfig(env: Env, input: unknown): Promise<TelegramBotConfig & { updated_at: number }> {
  const parsed = TelegramBotConfig.safeParse(input);
  if (!parsed.success) throw new Error('invalid_config');
  const cfg = parsed.data;
  if (cfg.model && !allowedModels(env).includes(cfg.model)) {
    throw new Error('model_not_allowed');
  }
  const now = Date.now();
  await env.DB.prepare(
    `INSERT INTO telegram_config (id, enabled, allowed_chat_ids, system_prompt, model, hourly_limit, allow_workspace_queries, updated_at)
     VALUES (1, ?, ?, ?, ?, ?, ?, ?)
     ON CONFLICT(id) DO UPDATE SET
       enabled = excluded.enabled,
       allowed_chat_ids = excluded.allowed_chat_ids,
       system_prompt = excluded.system_prompt,
       model = excluded.model,
       hourly_limit = excluded.hourly_limit,
       allow_workspace_queries = excluded.allow_workspace_queries,
       updated_at = excluded.updated_at`,
  )
    .bind(
      cfg.enabled ? 1 : 0,
      JSON.stringify(cfg.allowed_chat_ids),
      cfg.system_prompt,
      cfg.model,
      cfg.hourly_limit,
      cfg.allow_workspace_queries ? 1 : 0,
      now,
    )
    .run();
  return getBotConfig(env);
}

async function handleWorkspaceCommand(env: Env, input: string): Promise<string | null> {
  if (/^\/today(?:@\w+)?(?:\s|$)/i.test(input)) {
    const { results } = await env.DB.prepare(
      "SELECT data FROM sync_records WHERE type = 'event' AND deleted_at IS NULL ORDER BY updated_at DESC LIMIT 100",
    ).all<{ data: string }>();
    const now = new Date();
    const startOfDay = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
    const endOfDay = startOfDay + 86_400_000;
    const items = results
      .map((r) => {
        try { return JSON.parse(r.data) as Event; } catch { return null; }
      })
      .filter((e): e is Event => e !== null && !e.done && e.starts_at >= startOfDay && e.starts_at < endOfDay)
      .sort((a, b) => a.starts_at - b.starts_at)
      .slice(0, 10);
    if (!items.length) return 'Hôm nay không có sự kiện hoặc việc cần làm nào trên Data Server.';
    return '📅 Lịch hôm nay:\n' + items.map((e) => `• ${e.title} (${e.kind === 'todo' ? 'Việc cần làm' : 'Sự kiện'})`).join('\n');
  }

  if (/^\/habits(?:@\w+)?(?:\s|$)/i.test(input)) {
    const { results } = await env.DB.prepare(
      "SELECT data FROM sync_records WHERE type = 'habit' AND deleted_at IS NULL ORDER BY updated_at DESC LIMIT 20",
    ).all<{ data: string }>();
    const habits = results
      .map((r) => {
        try { return JSON.parse(r.data) as Habit; } catch { return null; }
      })
      .filter((h): h is Habit => h !== null);
    if (!habits.length) return 'Chưa có thói quen nào được đồng bộ lên Data Server.';
    return '🌱 Thói quen:\n' + habits.map((h) => `• ${h.title} (${h.frequency})`).join('\n');
  }

  const notesMatch = input.match(/^\/notes(?:@\w+)?(?:\s+(.*))?$/i);
  if (notesMatch) {
    const query = (notesMatch[1] ?? '').trim().toLowerCase();
    const { results } = await env.DB.prepare(
      "SELECT data FROM sync_records WHERE type = 'note' AND deleted_at IS NULL ORDER BY updated_at DESC LIMIT 50",
    ).all<{ data: string }>();
    const notes = results
      .map((r) => {
        try { return JSON.parse(r.data) as Note; } catch { return null; }
      })
      .filter((n): n is Note => n !== null && (!query || n.title.toLowerCase().includes(query) || n.body.toLowerCase().includes(query)))
      .slice(0, 5);
    if (!notes.length) return query ? `Không tìm thấy ghi chú nào khớp với "${query}".` : 'Chưa có ghi chú nào trên Data Server.';
    return '📝 Ghi chú:\n' + notes.map((n) => `• ${n.pinned ? '📌 ' : ''}${n.title}`).join('\n');
  }

  const createNoteMatch = input.match(/^\/note(?:@\w+)?\s+([\s\S]+)$/i);
  if (createNoteMatch) {
    const rawText = createNoteMatch[1]!.trim();
    if (!rawText) return 'Hãy nhập nội dung ghi chú: /note <nội dung>';
    const lines = rawText.split('\n');
    const title = lines[0]!.trim().slice(0, 120) || 'Ghi chú từ Telegram';
    const body = lines.length > 1 ? lines.slice(1).join('\n').trim() : rawText;
    const now = Date.now();
    const note: Note = {
      id: newId(),
      updated_at: now,
      deleted_at: null,
      device_id: 'telegram-bot',
      rev: 1,
      title,
      body,
      tags: ['telegram'],
      pinned: false,
    };
    await push(env, { records: [{ type: 'note', record: note }] });
    return `Đã lưu ghi chú "${title}" vào Data Server.`;
  }

  return null;
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
  if (!env.TELEGRAM_BOT_TOKEN) return;
  const config = await getBotConfig(env);
  if (!config.enabled || config.allowed_chat_ids.length === 0) return;
  const now = Date.now();
  // Processing may be retried after a worker crash, but never after a send begins.
  const claimed = await env.DB.prepare(`UPDATE telegram_webhook_inbox SET status = 'processing', updated_at = ?
    WHERE update_id = ? AND (status = 'received' OR (status = 'processing' AND updated_at < ?)) RETURNING payload`)
    .bind(now, delivery.updateId, now - 120_000).first<{ payload: string }>();
  if (!claimed) return;
  let message: { text?: unknown; chat?: { id?: unknown; type?: unknown } } | undefined;
  try { message = JSON.parse(claimed.payload).message; } catch { /* malformed inbox entry */ }
  const chat = message?.chat;
  const permitted = new Set(config.allowed_chat_ids);
  const chatId = Number.isSafeInteger(chat?.id) ? String(chat?.id) : '';
  const input = typeof message?.text === 'string' ? message.text.trim() : '';
  if (chat?.type !== 'private' || !permitted.has(chatId) || !input || input.length > 4000) {
    await env.DB.prepare("UPDATE telegram_webhook_inbox SET status = 'ignored', updated_at = ? WHERE update_id = ? AND status = 'processing'")
      .bind(Date.now(), delivery.updateId).run();
    return;
  }
  let reply: string;
  try {
    if (/^\/start(?:@\w+)?(?:\s|$)/i.test(input)) {
      reply = 'Xin chào! Tôi là Michiyo. Hãy gửi câu hỏi hoặc dùng /today, /habits, /notes, /note <nội dung> để làm việc với Data Server.';
    } else {
      const workspaceReply = config.allow_workspace_queries ? await handleWorkspaceCommand(env, input) : null;
      if (workspaceReply !== null) {
        reply = workspaceReply;
      } else {
        const bucket = `telegram:${chatId}:${Math.floor(Date.now() / 3_600_000)}`;
        const quota = await env.DB.prepare(`INSERT INTO rate_limits (bucket, count, expires_at) VALUES (?, 1, ?)
          ON CONFLICT(bucket) DO UPDATE SET count = count + 1 WHERE count < ? RETURNING count`)
          .bind(bucket, Date.now() + 7_200_000, config.hourly_limit).first<{ count: number }>();
        if (!quota) {
          await env.DB.prepare("UPDATE telegram_webhook_inbox SET status = 'ignored', updated_at = ? WHERE update_id = ? AND status = 'processing'")
            .bind(Date.now(), delivery.updateId).run();
          return;
        }
        const model = config.model || env.DEFAULT_CHAT_MODEL || DEFAULT_MODEL;
        if (!allowedModels(env).includes(model)) throw new Error('invalid model');
        const result = await env.AI.run(model as keyof AiModels, {
          messages: [{ role: 'system', content: config.system_prompt }, { role: 'user', content: input }],
          max_tokens: 512, stream: false,
        } as never, env.AI_GATEWAY_ID ? { gateway: { id: env.AI_GATEWAY_ID } } : undefined) as { response?: string };
        reply = typeof result.response === 'string' && result.response.trim() ? result.response.slice(0, 4000) : 'Xin lỗi, tôi chưa thể trả lời.';
      }
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
  if (!env.TELEGRAM_QUEUE || !env.TELEGRAM_BOT_TOKEN) return;
  const config = await getBotConfig(env);
  if (!config.enabled || config.allowed_chat_ids.length === 0) return;
  const { results } = await env.DB.prepare(`SELECT update_id, payload, received_at FROM telegram_webhook_inbox
    WHERE status IN ('received', 'processing') AND updated_at < ? ORDER BY received_at LIMIT 100`)
    .bind(now - 120_000).all<{ update_id: number; payload: string; received_at: number }>();
  for (const row of results) await env.TELEGRAM_QUEUE.send({ updateId: row.update_id, payload: row.payload, receivedAt: row.received_at });
}

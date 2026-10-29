import { Hono, type Context } from 'hono';
import { verifyAccessJwt } from './access';
import { decrypt, encrypt, normalizeCode, pairingCode, randomToken, sha256Hex } from './crypto';
import { allowedModels, DEFAULT_MODEL, EMBED_MODEL, type Env } from './env';
import { toOpenAiStream } from './stream';
import { pull, push, SyncError } from './sync';
import { receiveWebhook } from './telegram';

type Principal = { kind: 'device'; id: string } | { kind: 'key'; id: string; scopes: string[]; dailyLimit: number | null };
type Vars = { principal: Principal; admin: string };
type Ctx = Context<{ Bindings: Env; Variables: Vars }>;

const PROVIDERS = ['anthropic', 'openai', 'openrouter'];
const SCOPES = ['ai:chat', 'ai:embed'];
const DAY_MS = 86_400_000;

export const app = new Hono<{ Bindings: Env; Variables: Vars }>();

const fail = (c: Ctx, status: 400 | 401 | 403 | 404 | 429 | 502, error: string) => c.json({ error }, status);

/** Run after the response is sent; falls back to a detached promise outside a Worker (tests). */
function defer(c: Ctx, work: Promise<unknown>): void {
  const safe = work.catch(() => { console.error('background_operation_failed'); });
  try {
    c.executionCtx.waitUntil(safe);
  } catch {
    void safe;
  }
}

function bearer(c: Ctx): string | null {
  const header = c.req.header('Authorization') ?? '';
  return header.startsWith('Bearer ') ? header.slice(7).trim() : null;
}

async function rateLimited(env: Env, bucket: string, limit: number, windowMs: number): Promise<boolean> {
  const now = Date.now();
  const key = `${bucket}:${Math.floor(now / windowMs)}`;
  await env.DB.prepare('DELETE FROM rate_limits WHERE expires_at < ?').bind(now).run();
  const row = await env.DB.prepare(
    'INSERT INTO rate_limits (bucket, count, expires_at) VALUES (?, 1, ?) ON CONFLICT(bucket) DO UPDATE SET count = count + 1 RETURNING count',
  )
    .bind(key, now + windowMs)
    .first<{ count: number }>();
  return (row?.count ?? 0) > limit;
}

// ---- admin: Cloudflare Access JWT, or the local-only dev token -------------------------------

async function requireAdmin(c: Ctx, next: () => Promise<void>) {
  const env = c.env;
  const jwt = c.req.header('Cf-Access-Jwt-Assertion');
  if (jwt && env.ACCESS_TEAM_DOMAIN && env.ACCESS_AUD) {
    const email = await verifyAccessJwt(jwt, { team: env.ACCESS_TEAM_DOMAIN, aud: env.ACCESS_AUD });
    if (email) {
      c.set('admin', email);
      return next();
    }
  }
  const token = bearer(c);
  if (token && env.ENVIRONMENT === 'development' && env.DEV_ADMIN_TOKEN && (await sha256Hex(token)) === (await sha256Hex(env.DEV_ADMIN_TOKEN))) {
    c.set('admin', 'dev');
    return next();
  }
  return fail(c, 401, 'unauthorized');
}

// ---- device / API-key auth --------------------------------------------------------------------

async function authenticate(c: Ctx): Promise<Principal | null> {
  const token = bearer(c);
  if (!token) return null;
  const hash = await sha256Hex(token);
  if (token.startsWith('md_')) {
    const device = await c.env.DB.prepare('SELECT id FROM devices WHERE token_hash = ? AND revoked_at IS NULL').bind(hash).first<{ id: string }>();
    if (!device) return null;
    defer(c, c.env.DB.prepare('UPDATE devices SET last_seen = ? WHERE id = ?').bind(Date.now(), device.id).run());
    return { kind: 'device', id: device.id };
  }
  if (token.startsWith('mk_')) {
    const key = await c.env.DB.prepare('SELECT id, scopes, daily_limit FROM api_keys WHERE key_hash = ? AND revoked_at IS NULL')
      .bind(hash)
      .first<{ id: string; scopes: string; daily_limit: number | null }>();
    if (!key) return null;
    return { kind: 'key', id: key.id, scopes: JSON.parse(key.scopes), dailyLimit: key.daily_limit };
  }
  return null;
}

function requireScope(scope: string) {
  return async (c: Ctx, next: () => Promise<void>) => {
    const principal = await authenticate(c);
    if (!principal) return fail(c, 401, 'unauthorized');
    if (principal.kind === 'key') {
      if (!principal.scopes.includes(scope)) return fail(c, 403, 'forbidden');

    }
    c.set('principal', principal);
    return next();
  };
}

// Reserve a request before invoking AI. Every admitted attempt counts, including
// upstream failure or a cancelled stream; validation failures do not count.
async function reserveUsage(c: Ctx, route: string, model: string, inputChars: number): Promise<number | null> {
  const p = c.get('principal');
  const now = Date.now();
  const keyId = p.kind === 'key' ? p.id : null;
  const limit = p.kind === 'key' ? p.dailyLimit : null;
  const row = await c.env.DB.prepare(
    `INSERT INTO usage (at, device_id, key_id, route, model, input_chars, output_chars)
     SELECT ?, ?, ?, ?, ?, ?, 0 WHERE ? IS NULL OR
       (SELECT COUNT(*) FROM usage WHERE key_id = ? AND at >= ?) < ? RETURNING id`,
  ).bind(now, p.kind === 'device' ? p.id : null, keyId, route, model, inputChars, limit, keyId, now - DAY_MS, limit)
    .first<{ id: number }>();
  return row?.id ?? null;
}

function finishUsage(c: Ctx, id: number, outputChars: number) {
  return c.env.DB.prepare('UPDATE usage SET output_chars = ? WHERE id = ?').bind(outputChars, id).run();
}

// ---- pairing ----------------------------------------------------------------------------------

app.get('/v1/health', (c) => c.json({ ok: true }));
app.post('/v1/telegram/webhook', (c) => receiveWebhook(c.req.raw, c.env));

app.post('/v1/pair/start', requireAdmin, async (c) => {
  const body = await c.req.json().catch(() => ({}));
  const name = typeof body.name === 'string' && body.name.trim() ? body.name.trim().slice(0, 60) : 'Thiết bị mới';
  const code = pairingCode();
  const expiresAt = Date.now() + 10 * 60_000;
  await c.env.DB.prepare('INSERT INTO pairing_codes (code_hash, name, expires_at) VALUES (?, ?, ?)')
    .bind(await sha256Hex(code), name, expiresAt)
    .run();
  return c.json({ code, expires_at: expiresAt });
});

app.post('/v1/pair/complete', async (c) => {
  const ip = c.req.header('CF-Connecting-IP') ?? 'unknown';
  if (await rateLimited(c.env, `pair:${ip}`, 10, 10 * 60_000)) return fail(c, 429, 'too_many_attempts');
  const body = await c.req.json().catch(() => ({}));
  if (typeof body.code !== 'string') return fail(c, 400, 'invalid_code');
  const row = await c.env.DB.prepare('DELETE FROM pairing_codes WHERE code_hash = ? AND expires_at > ? RETURNING name')
    .bind(await sha256Hex(normalizeCode(body.code)), Date.now())
    .first<{ name: string }>();
  if (!row) return fail(c, 400, 'invalid_code');
  const name = typeof body.device_name === 'string' && body.device_name.trim() ? body.device_name.trim().slice(0, 60) : row.name;
  const token = randomToken('md_');
  const id = crypto.randomUUID();
  await c.env.DB.prepare('INSERT INTO devices (id, name, token_hash, created_at) VALUES (?, ?, ?, ?)')
    .bind(id, name, await sha256Hex(token), Date.now())
    .run();
  return c.json({ device_id: id, name, token });
});

app.get('/v1/me', requireScope('ai:chat'), (c) => c.json({ principal: c.get('principal').kind, id: c.get('principal').id }));

// ---- sync (devices only) ----------------------------------------------------------------------

async function requireDevice(c: Ctx, next: () => Promise<void>) {
  const principal = await authenticate(c);
  if (principal?.kind !== 'device') return fail(c, 401, 'unauthorized');
  c.set('principal', principal);
  return next();
}

app.post('/v1/sync/push', requireDevice, async (c) => {
  try {
    return c.json(await push(c.env, await c.req.json().catch(() => null)));
  } catch (error) {
    if (error instanceof SyncError) return fail(c, 400, error.message as never);
    throw error;
  }
});

app.get('/v1/sync/pull', requireDevice, async (c) => {
  try {
    return c.json(await pull(c.env, c.req.query('cursor')));
  } catch (error) {
    if (error instanceof SyncError) return fail(c, 400, error.message as never);
    throw error;
  }
});

// ---- AI ---------------------------------------------------------------------------------------

type ChatMessage = { role: 'system' | 'user' | 'assistant'; content: string };

function parseMessages(value: unknown): ChatMessage[] | null {
  if (!Array.isArray(value) || value.length === 0 || value.length > 200) return null;
  let total = 0;
  const out: ChatMessage[] = [];
  for (const m of value) {
    if (!m || !['system', 'user', 'assistant'].includes(m.role) || typeof m.content !== 'string') return null;
    total += m.content.length;
    out.push({ role: m.role, content: m.content });
  }
  return total <= 1_000_000 ? out : null;
}

const gateway = (env: Env) => (env.AI_GATEWAY_ID ? { gateway: { id: env.AI_GATEWAY_ID } } : undefined);

async function chat(c: Ctx) {
  const body = await c.req.json().catch(() => null);
  const messages = parseMessages(body?.messages);
  if (!messages) return fail(c, 400, 'invalid_messages');
  const system = typeof body.system === 'string' && body.system ? [{ role: 'system' as const, content: body.system.slice(0, 20_000) }] : [];
  const model = typeof body.model === 'string' && body.model ? body.model : c.env.DEFAULT_CHAT_MODEL || DEFAULT_MODEL;
  if (!allowedModels(c.env).includes(model)) return fail(c, 400, 'model_not_allowed');
  const maxTokens = Math.min(Math.max(Number.isInteger(body.max_tokens) ? body.max_tokens : 1024, 1), 4096);
  const all = [...system, ...messages];
  const inputChars = all.reduce((n, m) => n + m.content.length, 0);
  const input = { messages: all, max_tokens: maxTokens };
  const usageId = await reserveUsage(c, 'chat', model, inputChars);
  if (usageId === null) return fail(c, 429, 'daily_limit_reached');
  try {
    if (body.stream === false) {
      const result = (await c.env.AI.run(model as keyof AiModels, { ...input, stream: false } as never, gateway(c.env))) as { response?: string };
      const text = result.response ?? '';
      defer(c, finishUsage(c, usageId, text.length));
      return c.json({ object: 'chat.completion', model, choices: [{ index: 0, message: { role: 'assistant', content: text }, finish_reason: 'stop' }] });
    }
    const source = (await c.env.AI.run(model as keyof AiModels, { ...input, stream: true } as never, gateway(c.env))) as unknown as ReadableStream<Uint8Array>;
    const stream = toOpenAiStream(source, model, (chars) => defer(c, finishUsage(c, usageId, chars)));
    return new Response(stream, { headers: { 'Content-Type': 'text/event-stream', 'Cache-Control': 'no-store' } });
  } catch {
    return fail(c, 502, 'upstream_error');
  }
}

app.post('/v1/ai/chat', requireScope('ai:chat'), chat);
// OpenAI-compatible path so `base = <worker>/v1/ai` works with any client that appends /chat/completions.
app.post('/v1/ai/chat/completions', requireScope('ai:chat'), chat);

app.post('/v1/ai/embed', requireScope('ai:embed'), async (c) => {
  const body = await c.req.json().catch(() => null);
  const input: unknown = body?.input;
  const texts = typeof input === 'string' ? [input] : input;
  if (!Array.isArray(texts) || texts.length === 0 || texts.length > 100 || texts.some((t) => typeof t !== 'string' || !t || t.length > 8000))
    return fail(c, 400, 'invalid_input');
  const usageId = await reserveUsage(c, 'embed', EMBED_MODEL, texts.reduce((n: number, t: string) => n + t.length, 0));
  if (usageId === null) return fail(c, 429, 'daily_limit_reached');
  try {
    const result = (await c.env.AI.run(EMBED_MODEL as keyof AiModels, { text: texts } as never, gateway(c.env))) as { data?: number[][] };
    return c.json({ model: EMBED_MODEL, data: (result.data ?? []).map((embedding, index) => ({ index, embedding })) });
  } catch {
    return fail(c, 502, 'upstream_error');
  }
});

// ---- admin API --------------------------------------------------------------------------------

const admin = new Hono<{ Bindings: Env; Variables: Vars }>();
admin.use('*', requireAdmin);

admin.get('/devices', async (c) => {
  const { results } = await c.env.DB.prepare('SELECT id, name, created_at, last_seen, revoked_at FROM devices ORDER BY created_at DESC').all();
  return c.json({ devices: results });
});

admin.patch('/devices/:id', async (c) => {
  const body = await c.req.json().catch(() => null);
  if (typeof body?.name !== 'string' || !body.name.trim() || body.name.trim().length > 60) return fail(c, 400, 'invalid_request');
  const result = await c.env.DB.prepare('UPDATE devices SET name = ? WHERE id = ? AND revoked_at IS NULL')
    .bind(body.name.trim(), c.req.param('id')).run();
  return result.meta.changes ? c.json({ ok: true }) : fail(c, 404, 'not_found');
});

admin.delete('/devices/:id', async (c) => {
  const r = await c.env.DB.prepare('UPDATE devices SET revoked_at = ? WHERE id = ? AND revoked_at IS NULL').bind(Date.now(), c.req.param('id')).run();
  return r.meta.changes ? c.json({ ok: true }) : fail(c, 404, 'not_found');
});

admin.post('/api-keys', async (c) => {
  const body = await c.req.json().catch(() => ({}));
  const name = typeof body.name === 'string' && body.name.trim() ? body.name.trim().slice(0, 60) : null;
  const scopes: unknown = body.scopes ?? ['ai:chat'];
  const limit: unknown = body.daily_limit ?? null;
  if (!name || !Array.isArray(scopes) || !scopes.length || scopes.some((s) => !SCOPES.includes(s))) return fail(c, 400, 'invalid_request');
  if (limit !== null && (!Number.isInteger(limit) || (limit as number) < 1)) return fail(c, 400, 'invalid_request');
  const key = randomToken('mk_');
  const id = crypto.randomUUID();
  await c.env.DB.prepare('INSERT INTO api_keys (id, name, key_hash, prefix, scopes, daily_limit, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)')
    .bind(id, name, await sha256Hex(key), key.slice(0, 7), JSON.stringify(scopes), limit, Date.now())
    .run();
  // The only time the secret is ever returned.
  return c.json({ id, name, key, scopes, daily_limit: limit });
});

admin.get('/api-keys', async (c) => {
  const { results } = await c.env.DB.prepare('SELECT id, name, prefix, scopes, daily_limit, created_at, revoked_at FROM api_keys ORDER BY created_at DESC').all<{ scopes: string }>();
  return c.json({ api_keys: results.map((k) => ({ ...k, scopes: JSON.parse(k.scopes) })) });
});

admin.delete('/api-keys/:id', async (c) => {
  const r = await c.env.DB.prepare('UPDATE api_keys SET revoked_at = ? WHERE id = ? AND revoked_at IS NULL').bind(Date.now(), c.req.param('id')).run();
  return r.meta.changes ? c.json({ ok: true }) : fail(c, 404, 'not_found');
});

admin.put('/provider-keys/:provider', async (c) => {
  const provider = c.req.param('provider');
  const body = await c.req.json().catch(() => ({}));
  if (!PROVIDERS.includes(provider) || typeof body.key !== 'string' || !body.key.trim() || body.key.length > 500) return fail(c, 400, 'invalid_request');
  await c.env.DB.prepare('INSERT INTO provider_keys (provider, ciphertext, updated_at) VALUES (?, ?, ?) ON CONFLICT(provider) DO UPDATE SET ciphertext = excluded.ciphertext, updated_at = excluded.updated_at')
    .bind(provider, await encrypt(c.env.ENCRYPTION_KEY, body.key.trim()), Date.now())
    .run();
  return c.json({ provider, set: true });
});

// Lists which providers have a key; values are never returned.
admin.get('/provider-keys', async (c) => {
  const { results } = await c.env.DB.prepare('SELECT provider, updated_at FROM provider_keys ORDER BY provider').all();
  return c.json({ provider_keys: results });
});

admin.delete('/provider-keys/:provider', async (c) => {
  const r = await c.env.DB.prepare('DELETE FROM provider_keys WHERE provider = ?').bind(c.req.param('provider')).run();
  return r.meta.changes ? c.json({ ok: true }) : fail(c, 404, 'not_found');
});

admin.get('/usage', async (c) => {
  const days = Math.min(Math.max(Number.parseInt(c.req.query('days') ?? '7', 10) || 7, 1), 90);
  const { results } = await c.env.DB.prepare(
    "SELECT strftime('%Y-%m-%d', at / 1000, 'unixepoch') AS day, route, model, COUNT(*) AS requests, SUM(input_chars) AS input_chars, SUM(output_chars) AS output_chars FROM usage WHERE at >= ? GROUP BY day, route, model ORDER BY day DESC, model",
  )
    .bind(Date.now() - days * DAY_MS)
    .all();
  return c.json({ usage: results });
});

admin.get('/telegram/summary', async (c) => {
  const { results } = await c.env.DB.prepare('SELECT status, COUNT(*) AS count FROM telegram_webhook_inbox GROUP BY status').all();
  return c.json({ statuses: results });
});

admin.get('/telegram/issues', async (c) => {
  const { results } = await c.env.DB.prepare(`SELECT update_id, status, received_at, updated_at FROM telegram_webhook_inbox
    WHERE status = 'failed' OR (status = 'sending' AND updated_at < ?) ORDER BY updated_at DESC LIMIT 100`)
    .bind(Date.now() - 60_000).all();
  c.header('Cache-Control', 'no-store');
  return c.json({ issues: results });
});

admin.post('/telegram/issues/:id/retry', async (c) => {
  const id = c.req.param('id');
  if (!/^(0|[1-9]\d*)$/.test(id) || !Number.isSafeInteger(Number(id))) return fail(c, 400, 'invalid_update');
  const row = await c.env.DB.prepare("UPDATE telegram_webhook_inbox SET status = 'received', updated_at = ? WHERE update_id = ? AND status = 'failed' RETURNING update_id")
    .bind(Date.now(), Number(id)).first();
  return row ? c.json({ ok: true }) : fail(c, 404, 'not_found');
});

admin.post('/telegram/issues/:id/resolve', async (c) => {
  const id = c.req.param('id');
  if (!/^(0|[1-9]\d*)$/.test(id) || !Number.isSafeInteger(Number(id))) return fail(c, 400, 'invalid_update');
  const now = Date.now();
  const row = await c.env.DB.prepare("UPDATE telegram_webhook_inbox SET status = 'resolved', updated_at = ? WHERE update_id = ? AND (status = 'failed' OR (status = 'sending' AND updated_at < ?)) RETURNING update_id")
    .bind(now, Number(id), now - 60_000).first();
  return row ? c.json({ ok: true }) : fail(c, 404, 'not_found');
});

// Read-only counts for the admin "Dữ liệu" page.
admin.get('/sync/summary', async (c) => {
  const { results } = await c.env.DB.prepare('SELECT type, COUNT(*) AS records, SUM(deleted_at IS NOT NULL) AS tombstones FROM sync_records GROUP BY type ORDER BY type').all();
  return c.json({ summary: results });
});
admin.get('/sync/records', async (c) => {
  const type = c.req.query('type');
  if (!type || !['note', 'habit', 'habit_log', 'event', 'reminder'].includes(type)) return fail(c, 400, 'invalid_type');
  const raw = c.req.query('cursor') ?? '0';
  if (!/^(0|[1-9]\d*)$/.test(raw) || !Number.isSafeInteger(Number(raw))) return fail(c, 400, 'invalid_cursor');
  const { results } = await c.env.DB.prepare(
    'SELECT server_seq, data FROM sync_records WHERE type = ? AND deleted_at IS NULL AND server_seq > ? ORDER BY server_seq LIMIT 101',
  ).bind(type, Number(raw)).all<{ server_seq: number; data: string }>();
  const page = results.slice(0, 100);
  c.header('Cache-Control', 'no-store');
  return c.json({ records: page.map((row) => JSON.parse(row.data)), cursor: String(page.at(-1)?.server_seq ?? Number(raw)), more: results.length > 100 });
});

app.route('/v1/admin', admin);

/** For server-side use by later phases (bot, AI Gateway proxy); no HTTP route exposes it. */
export async function getProviderKey(env: Env, provider: string): Promise<string | null> {
  const row = await env.DB.prepare('SELECT ciphertext FROM provider_keys WHERE provider = ?').bind(provider).first<{ ciphertext: string }>();
  return row ? decrypt(env.ENCRYPTION_KEY, row.ciphertext) : null;
}

app.notFound((c) => c.json({ error: 'not_found' }, 404));
app.onError((_error, c) => c.json({ error: 'internal_error' }, 500));

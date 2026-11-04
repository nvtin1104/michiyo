import { Hono, type Context } from 'hono';
import { verifyAccessJwt } from './access';
import { decrypt, encrypt, normalizeCode, pairingCode, randomToken, sha256Hex } from './crypto';
import { allowedModels, DEFAULT_MODEL, EMBED_MODEL, type Env } from './env';
import { toOpenAiStream } from './stream';
import { pull, push, SyncError } from './sync';
import { getBotConfig, receiveWebhook, saveBotConfig } from './telegram';

type Principal = { kind: 'device'; id: string } | { kind: 'key'; id: string; scopes: string[]; dailyLimit: number | null };
type Vars = { principal: Principal; admin: string };
type Ctx = Context<{ Bindings: Env; Variables: Vars }>;

const PROVIDERS = ['anthropic', 'openai', 'openrouter'];
const SCOPES = ['ai:chat', 'ai:embed'];
const DAY_MS = 86_400_000;

export const app = new Hono<{ Bindings: Env; Variables: Vars }>();

const fail = (c: Ctx, status: 400 | 401 | 403 | 404 | 409 | 429 | 502, error: string) => c.json({ error }, status);

const SESSION_COOKIE = 'michiyo_admin';
const SESSION_MS = 12 * 60 * 60_000;
const LOGIN_CODE_MS = 60_000;

function cookie(c: Ctx, name: string): string | null {
  for (const part of (c.req.header('Cookie') ?? '').split(';')) {
    const index = part.indexOf('=');
    if (index > 0 && part.slice(0, index).trim() === name) return part.slice(index + 1).trim() || null;
  }
  return null;
}

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

// ---- admin: Cloudflare Access JWT, owner device, web session cookie, or the local-only dev token

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
  if (token?.startsWith('md_')) {
    const owner = await env.DB.prepare('SELECT id FROM devices WHERE token_hash = ? AND revoked_at IS NULL AND owner = 1')
      .bind(await sha256Hex(token))
      .first<{ id: string }>();
    if (owner) {
      defer(c, env.DB.prepare('UPDATE devices SET last_seen = ? WHERE id = ?').bind(Date.now(), owner.id).run());
      c.set('admin', `owner:${owner.id}`);
      return next();
    }
  }
  if (token && env.ENVIRONMENT === 'development' && env.DEV_ADMIN_TOKEN && (await sha256Hex(token)) === (await sha256Hex(env.DEV_ADMIN_TOKEN))) {
    c.set('admin', 'dev');
    return next();
  }
  const session = cookie(c, SESSION_COOKIE);
  if (session) {
    const row = await env.DB.prepare('SELECT 1 AS ok FROM admin_sessions WHERE id_hash = ? AND expires_at > ?')
      .bind(await sha256Hex(session), Date.now())
      .first<{ ok: number }>();
    if (row) {
      c.set('admin', 'web');
      return next();
    }
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

// The desktop app deploys this Worker with a fresh BOOTSTRAP_SECRET and exchanges it once for
// the owner device token, which can then administer the Worker (pairing, web admin login).
app.post('/v1/pair/bootstrap', async (c) => {
  const ip = c.req.header('CF-Connecting-IP') ?? 'unknown';
  if (await rateLimited(c.env, `bootstrap:${ip}`, 10, 10 * 60_000)) return fail(c, 429, 'too_many_attempts');
  if (!c.env.BOOTSTRAP_SECRET) return fail(c, 404, 'not_found');
  const body = await c.req.json().catch(() => ({}));
  const hash = await sha256Hex(typeof body?.secret === 'string' ? body.secret : '');
  if (typeof body?.secret !== 'string' || hash !== (await sha256Hex(c.env.BOOTSTRAP_SECRET))) return fail(c, 401, 'unauthorized');
  const claimed = await c.env.DB.prepare('INSERT INTO bootstrap_used (secret_hash, used_at) VALUES (?, ?) ON CONFLICT(secret_hash) DO NOTHING RETURNING secret_hash')
    .bind(hash, Date.now())
    .first();
  if (!claimed) return c.json({ error: 'bootstrap_used' }, 409);
  const name = typeof body.device_name === 'string' && body.device_name.trim() ? body.device_name.trim().slice(0, 60) : 'Máy tính chính';
  const token = randomToken('md_');
  const id = crypto.randomUUID();
  await c.env.DB.prepare('INSERT INTO devices (id, name, token_hash, created_at, owner) VALUES (?, ?, ?, ?, 1)')
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
  const { results } = await c.env.DB.prepare('SELECT id, name, created_at, last_seen, revoked_at, owner FROM devices ORDER BY created_at DESC').all();
  return c.json({ devices: results });
});

// One-time code (60 s) the desktop app puts in `<worker>/login#code=…` to open the web admin.
admin.post('/web-login', async (c) => {
  const now = Date.now();
  await c.env.DB.prepare('DELETE FROM login_codes WHERE expires_at <= ?').bind(now).run();
  const code = randomToken('ml_');
  const expiresAt = now + LOGIN_CODE_MS;
  await c.env.DB.prepare('INSERT INTO login_codes (code_hash, expires_at) VALUES (?, ?)').bind(await sha256Hex(code), expiresAt).run();
  c.header('Cache-Control', 'no-store');
  return c.json({ code, expires_at: expiresAt });
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

admin.get('/telegram/config', async (c) => {
  const config = await getBotConfig(c.env);
  c.header('Cache-Control', 'no-store');
  return c.json({
    config,
    environment: {
      token_configured: Boolean(c.env.TELEGRAM_BOT_TOKEN),
      webhook_secret_configured: Boolean(c.env.TELEGRAM_WEBHOOK_SECRET),
      queue_configured: Boolean(c.env.TELEGRAM_QUEUE),
      allowed_models: allowedModels(c.env),
    },
  });
});

admin.put('/telegram/config', async (c) => {
  try {
    const config = await saveBotConfig(c.env, await c.req.json().catch(() => null));
    return c.json({ config });
  } catch (e) {
    const msg = e instanceof Error ? e.message : 'invalid_config';
    return fail(c, 400, msg);
  }
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

// ---- web admin: session cookie and the same-origin /api bridge --------------------------------

/** Writes from the admin page must come from this origin (the cookie is also SameSite=Strict). */
function sameOrigin(c: Ctx): boolean {
  return c.req.header('Origin') === new URL(c.req.url).origin;
}

const sessionCookie = (value: string, maxAge: number) =>
  `${SESSION_COOKIE}=${value}; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=${maxAge}`;

app.post('/api/session', async (c) => {
  if (!sameOrigin(c)) return fail(c, 403, 'forbidden');
  const ip = c.req.header('CF-Connecting-IP') ?? 'unknown';
  if (await rateLimited(c.env, `session:${ip}`, 10, 10 * 60_000)) return fail(c, 429, 'too_many_attempts');
  const body = await c.req.json().catch(() => ({}));
  if (typeof body?.code !== 'string' || !body.code) return fail(c, 400, 'invalid_code');
  const now = Date.now();
  const row = await c.env.DB.prepare('DELETE FROM login_codes WHERE code_hash = ? AND expires_at > ? RETURNING code_hash')
    .bind(await sha256Hex(body.code), now)
    .first();
  if (!row) return fail(c, 400, 'invalid_code');
  await c.env.DB.prepare('DELETE FROM admin_sessions WHERE expires_at <= ?').bind(now).run();
  const session = randomToken('ms_');
  await c.env.DB.prepare('INSERT INTO admin_sessions (id_hash, expires_at) VALUES (?, ?)').bind(await sha256Hex(session), now + SESSION_MS).run();
  c.header('Set-Cookie', sessionCookie(session, SESSION_MS / 1000));
  c.header('Cache-Control', 'no-store');
  return c.json({ ok: true });
});

app.post('/api/logout', async (c) => {
  if (!sameOrigin(c)) return fail(c, 403, 'forbidden');
  const session = cookie(c, SESSION_COOKIE);
  if (session) await c.env.DB.prepare('DELETE FROM admin_sessions WHERE id_hash = ?').bind(await sha256Hex(session)).run();
  c.header('Set-Cookie', sessionCookie('', 0));
  c.header('Cache-Control', 'no-store');
  return c.json({ ok: true });
});

// Only these routes are reachable from the admin page; each maps `/api/<path>` to `/v1/<path>`.
const API_ALLOWED: Record<string, RegExp[]> = {
  GET: [/^admin\/(devices|api-keys|provider-keys|usage|sync\/(summary|records)|telegram\/(summary|issues|config))$/],
  POST: [/^pair\/start$/, /^admin\/api-keys$/, /^admin\/telegram\/issues\/\d+\/(retry|resolve)$/],
  PUT: [/^admin\/provider-keys\/(anthropic|openai|openrouter)$/, /^admin\/telegram\/config$/],
  PATCH: [/^admin\/devices\/[a-zA-Z0-9-]+$/],
  DELETE: [/^admin\/(devices|api-keys)\/[a-zA-Z0-9-]+$/, /^admin\/provider-keys\/(anthropic|openai|openrouter)$/],
};

app.all('/api/*', async (c) => {
  const url = new URL(c.req.url);
  const path = url.pathname.slice('/api/'.length);
  const method = c.req.method;
  if (!API_ALLOWED[method]?.some((pattern) => pattern.test(path))) return fail(c, 404, 'not_found');
  if (method !== 'GET' && !sameOrigin(c)) return fail(c, 403, 'forbidden');
  // Forward only what admin auth needs; no bearer credential is accepted from the browser.
  const headers = new Headers();
  for (const name of ['Cookie', 'Cf-Access-Jwt-Assertion', 'Content-Type', 'CF-Connecting-IP']) {
    const value = c.req.header(name);
    if (value) headers.set(name, value);
  }
  let ctx: ExecutionContext | undefined;
  try {
    ctx = c.executionCtx;
  } catch {
    ctx = undefined;
  }
  const upstream = await app.fetch(
    new Request(`${url.origin}/v1/${path}${url.search}`, { method, headers, body: method === 'GET' ? undefined : await c.req.text() }),
    c.env,
    ctx,
  );
  return new Response(upstream.body, {
    status: upstream.status,
    headers: { 'Cache-Control': 'no-store', 'Content-Type': upstream.headers.get('Content-Type') ?? 'application/json' },
  });
});

/** For server-side use by later phases (bot, AI Gateway proxy); no HTTP route exposes it. */
export async function getProviderKey(env: Env, provider: string): Promise<string | null> {
  const row = await env.DB.prepare('SELECT ciphertext FROM provider_keys WHERE provider = ?').bind(provider).first<{ ciphertext: string }>();
  return row ? decrypt(env.ENCRYPTION_KEY, row.ciphertext) : null;
}

app.notFound((c) => {
  // Everything outside the API is the admin SPA (static assets binding) when deployed with it.
  const path = c.req.path;
  if (c.env.ASSETS && !path.startsWith('/v1/') && !path.startsWith('/api/')) return c.env.ASSETS.fetch(c.req.raw);
  return c.json({ error: 'not_found' }, 404);
});
app.onError((_error, c) => c.json({ error: 'internal_error' }, 500));

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { clearCertCache, verifyAccessJwt } from '../src/access';
import { app, getProviderKey } from '../src/app';
import { decrypt, encrypt, toBase64Url } from '../src/crypto';
import type { Env } from '../src/env';
import { toOpenAiStream } from '../src/stream';
import { createD1 } from './d1';

const ADMIN = { Authorization: 'Bearer dev-admin-token' };
const key = toBase64Url(new Uint8Array(32).fill(7)).replace(/-/g, '+').replace(/_/g, '/') + '=';

function workersAiStream(parts: string[]): ReadableStream<Uint8Array> {
  const encoder = new TextEncoder();
  const body = parts.map((p) => `data: ${JSON.stringify({ response: p })}\n\n`).join('') + 'data: [DONE]\n\n';
  return new ReadableStream({
    start(controller) {
      controller.enqueue(encoder.encode(body));
      controller.close();
    },
  });
}

let env: Env;
let aiCalls: { model: string; input: any; options: any }[];
const pending: Promise<unknown>[] = [];
const ctx = { waitUntil: (p: Promise<unknown>) => void pending.push(p), passThroughOnException() {} } as unknown as ExecutionContext;

const call = (path: string, init: RequestInit = {}) => app.request(path, init, env, ctx);
const post = (path: string, body: unknown, headers: Record<string, string> = {}) =>
  call(path, { method: 'POST', headers: { 'Content-Type': 'application/json', ...headers }, body: JSON.stringify(body) });

async function pair(): Promise<string> {
  const { code } = (await (await post('/v1/pair/start', { name: 'Laptop' }, ADMIN)).json()) as { code: string };
  const done = await post('/v1/pair/complete', { code });
  return ((await done.json()) as { token: string }).token;
}

beforeEach(() => {
  aiCalls = [];
  env = {
    DB: createD1(),
    ENCRYPTION_KEY: key,
    DEV_ADMIN_TOKEN: 'dev-admin-token',
    ALLOWED_CHAT_MODELS: '@cf/meta/llama-3.1-8b-instruct',
    AI: {
      run: async (model: string, input: any, options: any) => {
        aiCalls.push({ model, input, options });
        return input.stream ? workersAiStream(['Xin ', 'chào']) : { response: 'Xin chào' };
      },
    } as unknown as Ai,
  };
  clearCertCache();
});

describe('pairing and device auth', () => {
  it('requires admin to start pairing', async () => {
    expect((await post('/v1/pair/start', {})).status).toBe(401);
    expect((await post('/v1/pair/start', {}, { Authorization: 'Bearer wrong' })).status).toBe(401);
    env.DEV_ADMIN_TOKEN = undefined;
    expect((await post('/v1/pair/start', {}, ADMIN)).status).toBe(401);
  });

  it('exchanges a code once for a device token and stores only hashes', async () => {
    const { code } = (await (await post('/v1/pair/start', { name: 'Laptop' }, ADMIN)).json()) as { code: string };
    expect(code).toMatch(/^[A-Z2-9]{8}$/);
    const done = await post('/v1/pair/complete', { code: code.toLowerCase().replace(/^(.{4})/, '$1-') });
    expect(done.status).toBe(200);
    const device = (await done.json()) as { token: string; name: string };
    expect(device.token).toMatch(/^md_/);
    expect(device.name).toBe('Laptop');
    expect((await post('/v1/pair/complete', { code })).status).toBe(400); // single use
    const stored = JSON.stringify((await env.DB.prepare('SELECT * FROM devices').all()).results);
    expect(stored).not.toContain(device.token);
    const me = await call('/v1/me', { headers: { Authorization: `Bearer ${device.token}` } });
    expect(me.status).toBe(200);
  });

  it('rejects expired codes and throttles guessing', async () => {
    const { code } = (await (await post('/v1/pair/start', {}, ADMIN)).json()) as { code: string };
    await env.DB.prepare('UPDATE pairing_codes SET expires_at = 1').run();
    expect((await post('/v1/pair/complete', { code })).status).toBe(400);
    let last = 0;
    for (let i = 0; i < 12; i++) last = (await post('/v1/pair/complete', { code: 'AAAAAAAA' }, { 'CF-Connecting-IP': '9.9.9.9' })).status;
    expect(last).toBe(429);
  });

  it('revokes a device immediately', async () => {
    const token = await pair();
    const { devices } = (await (await call('/v1/admin/devices', { headers: ADMIN })).json()) as { devices: { id: string }[] };
    expect((await call(`/v1/admin/devices/${devices[0]!.id}`, { method: 'DELETE', headers: ADMIN })).status).toBe(200);
    expect((await call('/v1/me', { headers: { Authorization: `Bearer ${token}` } })).status).toBe(401);
  });
});

describe('AI proxy', () => {
  const messages = [{ role: 'user', content: 'chào' }];

  it('needs credentials', async () => {
    expect((await post('/v1/ai/chat', { messages })).status).toBe(401);
    expect((await post('/v1/ai/chat', { messages }, { Authorization: 'Bearer md_forged' })).status).toBe(401);
  });

  it('streams OpenAI-style chunks and records usage', async () => {
    const token = await pair();
    env.AI_GATEWAY_ID = 'gw';
    const res = await post('/v1/ai/chat/completions', { messages, system: 'Bạn là Michiyo', max_tokens: 50 }, { Authorization: `Bearer ${token}` });
    expect(res.headers.get('Content-Type')).toContain('text/event-stream');
    const text = await res.text();
    await Promise.all(pending);
    const deltas = [...text.matchAll(/data: (\{.*\})/g)].map((m) => JSON.parse(m[1]!).choices[0].delta.content).filter(Boolean);
    expect(deltas.join('')).toBe('Xin chào');
    expect(text.trimEnd().endsWith('data: [DONE]')).toBe(true);
    expect(aiCalls[0]!.input.messages[0]).toEqual({ role: 'system', content: 'Bạn là Michiyo' });
    expect(aiCalls[0]!.input.max_tokens).toBe(50);
    expect(aiCalls[0]!.options).toEqual({ gateway: { id: 'gw' } });
    const row = await env.DB.prepare('SELECT route, model, input_chars, output_chars FROM usage').first<Record<string, unknown>>();
    expect(row).toMatchObject({ route: 'chat', input_chars: 'Bạn là Michiyo'.length + 4, output_chars: 8 });
  });

  it('supports non-streaming replies and validates input', async () => {
    const auth = { Authorization: `Bearer ${await pair()}` };
    const ok = await post('/v1/ai/chat', { messages, stream: false }, auth);
    expect(((await ok.json()) as any).choices[0].message.content).toBe('Xin chào');
    for (const bad of [{}, { messages: [] }, { messages: [{ role: 'root', content: 'x' }] }, { messages: [{ role: 'user', content: 5 }] }])
      expect((await post('/v1/ai/chat', bad, auth)).status).toBe(400);
    expect((await post('/v1/ai/chat', { messages, model: '@cf/other/model' }, auth)).status).toBe(400);
  });

  it('hides upstream failures', async () => {
    const auth = { Authorization: `Bearer ${await pair()}` };
    env.AI = { run: async () => { throw new Error('secret upstream detail'); } } as unknown as Ai;
    const res = await post('/v1/ai/chat', { messages }, auth);
    expect(res.status).toBe(502);
    expect(await res.text()).not.toContain('secret');
  });
});

describe('Michiyo API keys', () => {
  async function createKey(body: object) {
    const res = await post('/v1/admin/api-keys', body, ADMIN);
    return { res, json: (await res.json()) as any };
  }

  it('shows the secret once, enforces scopes, quota and revocation', async () => {
    const { json } = await createKey({ name: 'bot', scopes: ['ai:chat'], daily_limit: 2 });
    expect(json.key).toMatch(/^mk_/);
    const auth = { Authorization: `Bearer ${json.key}` };
    const messages = [{ role: 'user', content: 'hi' }];
    expect((await post('/v1/ai/embed', { input: 'x' }, auth)).status).toBe(403);
    for (let i = 0; i < 2; i++) {
      await (await post('/v1/ai/chat', { messages, stream: false }, auth)).text();
      await Promise.all(pending);
    }
    expect((await post('/v1/ai/chat', { messages, stream: false }, auth)).status).toBe(429);
    const listed = JSON.stringify(await (await call('/v1/admin/api-keys', { headers: ADMIN })).json());
    expect(listed).not.toContain(json.key);
    expect(listed).toContain(json.key.slice(0, 7));
    await call(`/v1/admin/api-keys/${json.id}`, { method: 'DELETE', headers: ADMIN });
    expect((await post('/v1/ai/chat', { messages }, auth)).status).toBe(401);
  });

  it('rejects malformed requests and embeds with the right scope', async () => {
    expect((await createKey({ name: '', scopes: ['ai:chat'] })).res.status).toBe(400);
    expect((await createKey({ name: 'x', scopes: ['admin'] })).res.status).toBe(400);
    expect((await createKey({ name: 'x', scopes: ['ai:chat'], daily_limit: 0 })).res.status).toBe(400);
    const { json } = await createKey({ name: 'rag', scopes: ['ai:embed'] });
    env.AI = { run: async () => ({ data: [[0.1, 0.2]] }) } as unknown as Ai;
    const res = await post('/v1/ai/embed', { input: ['a'] }, { Authorization: `Bearer ${json.key}` });
    expect(((await res.json()) as any).data[0]).toEqual({ index: 0, embedding: [0.1, 0.2] });
    expect((await post('/v1/ai/embed', { input: [] }, { Authorization: `Bearer ${json.key}` })).status).toBe(400);
  });
});

describe('provider keys', () => {
  it('encrypts at rest and never returns the value', async () => {
    const put = await call('/v1/admin/provider-keys/openai', { method: 'PUT', headers: { ...ADMIN, 'Content-Type': 'application/json' }, body: JSON.stringify({ key: 'sk-live-123' }) });
    expect(put.status).toBe(200);
    const raw = await env.DB.prepare('SELECT ciphertext FROM provider_keys').first<{ ciphertext: string }>();
    expect(raw!.ciphertext).not.toContain('sk-live-123');
    expect(JSON.stringify(await (await call('/v1/admin/provider-keys', { headers: ADMIN })).json())).not.toContain('ciphertext');
    expect(await getProviderKey(env, 'openai')).toBe('sk-live-123');
    expect(await getProviderKey(env, 'anthropic')).toBeNull();
    const bad = await call('/v1/admin/provider-keys/evil', { method: 'PUT', headers: { ...ADMIN, 'Content-Type': 'application/json' }, body: JSON.stringify({ key: 'x' }) });
    expect(bad.status).toBe(400);
    expect((await call('/v1/admin/provider-keys/openai', { method: 'DELETE', headers: ADMIN })).status).toBe(200);
  });

  it('uses a fresh IV and detects tampering', async () => {
    const a = await encrypt(key, 'same');
    expect(a).not.toBe(await encrypt(key, 'same'));
    expect(await decrypt(key, a)).toBe('same');
    const [v, iv, data] = a.split('.');
    await expect(decrypt(key, `${v}.${iv}.${data!.slice(0, -2)}AA`)).rejects.toBeDefined();
  });
});

describe('usage report', () => {
  it('groups requests per day and model', async () => {
    const auth = { Authorization: `Bearer ${await pair()}` };
    await (await post('/v1/ai/chat', { messages: [{ role: 'user', content: 'a' }], stream: false }, auth)).text();
    await Promise.all(pending);
    const { usage } = (await (await call('/v1/admin/usage?days=1', { headers: ADMIN })).json()) as { usage: any[] };
    expect(usage[0]).toMatchObject({ route: 'chat', requests: 1, output_chars: 8 });
  });
});

describe('Cloudflare Access JWT', () => {
  async function setup() {
    const pair = (await crypto.subtle.generateKey({ name: 'RSASSA-PKCS1-v1_5', modulusLength: 2048, publicExponent: new Uint8Array([1, 0, 1]), hash: 'SHA-256' }, true, ['sign', 'verify'])) as CryptoKeyPair;
    const jwk = { ...(await crypto.subtle.exportKey('jwk', pair.publicKey)), kid: 'k1' };
    const fetcher = (async () => new Response(JSON.stringify({ keys: [jwk] }))) as unknown as typeof fetch;
    const enc = (o: object) => toBase64Url(new TextEncoder().encode(JSON.stringify(o)));
    const sign = async (claims: object, header: object = { alg: 'RS256', kid: 'k1' }) => {
      const signed = `${enc(header)}.${enc(claims)}`;
      const sig = await crypto.subtle.sign('RSASSA-PKCS1-v1_5', pair.privateKey, new TextEncoder().encode(signed));
      return `${signed}.${toBase64Url(new Uint8Array(sig))}`;
    };
    return { fetcher, sign };
  }
  const opts = { team: 'me.cloudflareaccess.com', aud: 'aud-1' };
  const good = { aud: ['aud-1'], iss: 'https://me.cloudflareaccess.com', exp: Date.now() / 1000 + 600, email: 'me@example.com' };

  it('accepts a valid token and rejects bad audience, issuer, expiry, signature and algorithm', async () => {
    const { fetcher, sign } = await setup();
    expect(await verifyAccessJwt(await sign(good), opts, fetcher)).toBe('me@example.com');
    expect(await verifyAccessJwt(await sign({ ...good, aud: ['other'] }), opts, fetcher)).toBeNull();
    expect(await verifyAccessJwt(await sign({ ...good, iss: 'https://evil.example' }), opts, fetcher)).toBeNull();
    expect(await verifyAccessJwt(await sign({ ...good, exp: 1 }), opts, fetcher)).toBeNull();
    expect(await verifyAccessJwt(await sign(good, { alg: 'none', kid: 'k1' }), opts, fetcher)).toBeNull();
    const token = await sign(good);
    expect(await verifyAccessJwt(token.slice(0, -4) + 'AAAA', opts, fetcher)).toBeNull();
    expect(await verifyAccessJwt('a.b', opts, fetcher)).toBeNull();
  });

  it('authorizes admin routes through the Access header', async () => {
    const { fetcher, sign } = await setup();
    vi.stubGlobal('fetch', fetcher);
    env.ACCESS_TEAM_DOMAIN = opts.team;
    env.ACCESS_AUD = opts.aud;
    const res = await call('/v1/admin/devices', { headers: { 'Cf-Access-Jwt-Assertion': await sign(good) } });
    expect(res.status).toBe(200);
    expect((await call('/v1/admin/devices', { headers: { 'Cf-Access-Jwt-Assertion': 'x.y.z' } })).status).toBe(401);
    vi.unstubAllGlobals();
  });
});

describe('stream conversion', () => {
  it('survives events split across chunks and finishes once', async () => {
    const encoder = new TextEncoder();
    const raw = encoder.encode(`data: ${JSON.stringify({ response: 'chào ✦' })}\n\ndata: [DONE]\n\n`);
    let finished = 0;
    for (let split = 1; split < raw.length; split++) {
      const source = new ReadableStream<Uint8Array>({
        start(c) {
          c.enqueue(raw.slice(0, split));
          c.enqueue(raw.slice(split));
          c.close();
        },
      });
      const text = await new Response(toOpenAiStream(source, 'm', () => finished++)).text();
      expect(text.match(/chào ✦/g)?.length).toBe(1);
      expect(text.match(/\[DONE\]/g)?.length).toBe(1);
    }
    expect(finished).toBe(raw.length - 1);
  });
});

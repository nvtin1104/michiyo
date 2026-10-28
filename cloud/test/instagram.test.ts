import { beforeEach, describe, expect, it, vi } from 'vitest';
import { app } from '../src/app';
import worker from '../src/index';
import type { Env } from '../src/env';
import { createD1 } from './d1';

let env: Env;
let send: ReturnType<typeof vi.fn>;
const body = JSON.stringify({ object: 'instagram', entry: [{ id: 'account', messaging: [{ message: { mid: 'm1', text: 'hello' } }] }] });
async function signature(payload: string, secret = 'app-secret') {
  const encoder = new TextEncoder();
  const key = await crypto.subtle.importKey('raw', encoder.encode(secret), { name: 'HMAC', hash: 'SHA-256' }, false, ['sign']);
  const bytes = new Uint8Array(await crypto.subtle.sign('HMAC', key, encoder.encode(payload)));
  return `sha256=${Array.from(bytes, (n) => n.toString(16).padStart(2, '0')).join('')}`;
}
const call = (path: string, init?: RequestInit) => app.request(path, init, env);
beforeEach(() => {
  send = vi.fn(async () => {});
  env = { DB: createD1(), AI: {} as Ai, ENCRYPTION_KEY: '', IG_VERIFY_TOKEN: 'verify-secret', IG_APP_SECRET: 'app-secret', IG_QUEUE: { send } as unknown as Queue<{ payload: string; receivedAt: number }> };
});

describe('Instagram webhook intake', () => {
  it('verifies the subscription challenge without accepting a wrong token', async () => {
    expect((await call('/v1/ig/webhook?hub.mode=subscribe&hub.verify_token=wrong&hub.challenge=123')).status).toBe(403);
    const response = await call('/v1/ig/webhook?hub.mode=subscribe&hub.verify_token=verify-secret&hub.challenge=123');
    expect(await response.text()).toBe('123');
  });
  it('checks the signature over exact raw bytes and queues only valid payloads', async () => {
    const sig = await signature(body);
    const post = (payload: string, signed: string) => call('/v1/ig/webhook', { method: 'POST', headers: { 'X-Hub-Signature-256': signed }, body: payload });
    expect((await post(body + ' ', sig)).status).toBe(403);
    expect((await post(body, 'sha256=bad')).status).toBe(403);
    expect(send).not.toHaveBeenCalled();
    expect((await post(body, sig)).status).toBe(200);
    expect(send).toHaveBeenCalledWith(expect.objectContaining({ payload: body, receivedAt: expect.any(Number) }));
    const invalid = '{"object":"other","entry":[]}';
    expect((await post(invalid, await signature(invalid))).status).toBe(400);
    expect(send).toHaveBeenCalledTimes(1);
  });
  it('rejects oversized bodies and reports a failed enqueue', async () => {
    const post = (payload: string, signed: string) => call('/v1/ig/webhook', { method: 'POST', headers: { 'X-Hub-Signature-256': signed }, body: payload });
    const large = 'x'.repeat(96001);
    expect((await post(large, await signature(large))).status).toBe(413);
    send.mockRejectedValueOnce(new Error('queue down'));
    expect((await post(body, await signature(body))).status).toBe(503);
  });
  it('stores each identical delivery once and retries when storage fails', async () => {
    const ack = vi.fn(), retry = vi.fn();
    const message = { body: { payload: body, receivedAt: 123 }, ack, retry };
    await worker.queue({ messages: [message, message] } as unknown as MessageBatch<{ payload: string; receivedAt: number }>, env);
    expect(ack).toHaveBeenCalledTimes(2);
    const row = await env.DB.prepare('SELECT COUNT(*) AS count FROM ig_webhook_inbox').first<{ count: number }>();
    expect(row?.count).toBe(1);
    env.DB = { prepare: () => { throw new Error('database unavailable'); } } as unknown as D1Database;
    await worker.queue({ messages: [message] } as unknown as MessageBatch<{ payload: string; receivedAt: number }>, env);
    expect(retry).toHaveBeenCalledOnce();
  });
});

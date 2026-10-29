import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { app } from '../src/app';
import worker from '../src/index';
import type { Env } from '../src/env';
import { createD1 } from './d1';

let env: Env;
let send: ReturnType<typeof vi.fn>;
const payload = JSON.stringify({ update_id: 123, message: { text: 'hello', chat: { id: 42 } } });
const post = (body: string, secret = 'telegram-secret') => app.request('/v1/telegram/webhook', {
  method: 'POST', headers: { 'X-Telegram-Bot-Api-Secret-Token': secret }, body,
}, env);

beforeEach(() => {
  send = vi.fn(async () => {});
  env = { DB: createD1(), AI: {} as Ai, ENCRYPTION_KEY: '', TELEGRAM_WEBHOOK_SECRET: 'telegram-secret', TELEGRAM_QUEUE: { send } as unknown as Env['TELEGRAM_QUEUE'] };
});
afterEach(() => vi.unstubAllGlobals());

describe('Telegram webhook intake', () => {
  it('authenticates, validates and enqueues updates', async () => {
    expect((await post(payload, 'wrong')).status).toBe(403);
    expect((await post('{"update_id":"bad"}')).status).toBe(400);
    expect((await post('x'.repeat(96001))).status).toBe(413);
    expect(send).not.toHaveBeenCalled();
    expect((await post(payload)).status).toBe(200);
    expect(send).toHaveBeenCalledWith(expect.objectContaining({ updateId: 123, payload }));
    send.mockRejectedValueOnce(new Error('queue down'));
    expect((await post(payload)).status).toBe(503);
  });

  it('deduplicates update ids and retries database failures', async () => {
    const ack = vi.fn(), retry = vi.fn();
    const message = { body: { updateId: 123, payload, receivedAt: 123 }, ack, retry };
    await worker.queue({ messages: [message, message] } as unknown as MessageBatch<typeof message.body>, env);
    expect(ack).toHaveBeenCalledTimes(2);
    const row = await env.DB.prepare('SELECT COUNT(*) AS count FROM telegram_webhook_inbox').first<{ count: number }>();
    expect(row?.count).toBe(1);
    env.DB = { prepare: () => { throw new Error('database unavailable'); } } as unknown as D1Database;
    await worker.queue({ messages: [message] } as unknown as MessageBatch<typeof message.body>, env);
    expect(retry).toHaveBeenCalledOnce();
  });

  it('removes expired inbox payloads on schedule', async () => {
    const now = Date.now();
    await env.DB.prepare('INSERT INTO telegram_webhook_inbox (update_id, received_at, payload) VALUES (?, ?, ?)').bind(1, now - 31 * 86400000, '{}').run();
    await env.DB.prepare('INSERT INTO telegram_webhook_inbox (update_id, received_at, payload) VALUES (?, ?, ?)').bind(2, now, '{}').run();
    await worker.scheduled({} as ScheduledController, env);
    const rows = await env.DB.prepare('SELECT update_id FROM telegram_webhook_inbox').all<{ update_id: number }>();
    expect(rows.results).toEqual([{ update_id: 2 }]);
  });

  it('responds once to an allowed private chat even if delivered twice', async () => {
    env.TELEGRAM_BOT_TOKEN = 'test-token';
    env.TELEGRAM_ALLOWED_CHAT_IDS = '42';
    const fetchMock = vi.fn(async (_url: string, _init: RequestInit) => Response.json({ ok: true }));
    vi.stubGlobal('fetch', fetchMock);
    const ack = vi.fn(), retry = vi.fn();
    const update = JSON.stringify({ update_id: 7, message: { text: '/start', chat: { id: 42, type: 'private' } } });
    const message = { body: { updateId: 7, payload: update, receivedAt: Date.now() }, ack, retry };
    await worker.queue({ messages: [message, message] } as unknown as MessageBatch<typeof message.body>, env);
    expect(fetchMock).toHaveBeenCalledOnce();
    expect(JSON.parse(fetchMock.mock.calls[0]![1]!.body as string).chat_id).toBe('42');
    expect((await env.DB.prepare('SELECT status FROM telegram_webhook_inbox WHERE update_id = 7').first<{ status: string }>())?.status).toBe('sent');
  });

  it('ignores unauthorized chats and does not retry an ambiguous send', async () => {
    env.TELEGRAM_BOT_TOKEN = 'test-token'; env.TELEGRAM_ALLOWED_CHAT_IDS = '42';
    const fetchMock = vi.fn(async () => { throw new Error('timeout'); });
    vi.stubGlobal('fetch', fetchMock);
    const ack = vi.fn(), retry = vi.fn();
    const make = (id: number, chat: number) => ({ body: { updateId: id, payload: JSON.stringify({ update_id: id, message: { text: '/start', chat: { id: chat, type: 'private' } } }), receivedAt: Date.now() }, ack, retry });
    const denied = make(8, 43), uncertain = make(9, 42);
    await worker.queue({ messages: [denied, uncertain, uncertain] } as unknown as MessageBatch<typeof denied.body>, env);
    expect(fetchMock).toHaveBeenCalledOnce();
    expect(retry).not.toHaveBeenCalled();
    expect((await env.DB.prepare('SELECT status FROM telegram_webhook_inbox WHERE update_id = 9').first<{ status: string }>())?.status).toBe('sending');
  });

  it('retries an AI failure before sending and then responds', async () => {
    env.TELEGRAM_BOT_TOKEN = 'test-token'; env.TELEGRAM_ALLOWED_CHAT_IDS = '42';
    const ai = vi.fn().mockRejectedValueOnce(new Error('unavailable')).mockResolvedValueOnce({ response: 'Xin chào' });
    env.AI = { run: ai } as unknown as Ai;
    const fetchMock = vi.fn(async () => Response.json({ ok: true }));
    vi.stubGlobal('fetch', fetchMock);
    const ack = vi.fn(), retry = vi.fn();
    const message = { body: { updateId: 10, payload: JSON.stringify({ update_id: 10, message: { text: 'Hi', chat: { id: 42, type: 'private' } } }), receivedAt: Date.now() }, ack, retry };
    await worker.queue({ messages: [message] } as unknown as MessageBatch<typeof message.body>, env);
    expect(retry).toHaveBeenCalledOnce();
    expect(fetchMock).not.toHaveBeenCalled();
    await worker.queue({ messages: [message] } as unknown as MessageBatch<typeof message.body>, env);
    expect(fetchMock).toHaveBeenCalledOnce();
    expect(ack).toHaveBeenCalledOnce();
  });

  it('caps AI requests per chat before invoking the model', async () => {
    env.TELEGRAM_BOT_TOKEN = 'test-token'; env.TELEGRAM_ALLOWED_CHAT_IDS = '42';
    const ai = vi.fn(async () => ({ response: 'ok' }));
    env.AI = { run: ai } as unknown as Ai;
    vi.stubGlobal('fetch', vi.fn(async () => Response.json({ ok: true })));
    const ack = vi.fn(), retry = vi.fn();
    const messages = Array.from({ length: 21 }, (_, i) => ({ body: { updateId: i, payload: JSON.stringify({ update_id: i, message: { text: 'Hi', chat: { id: 42, type: 'private' } } }), receivedAt: Date.now() }, ack, retry }));
    await worker.queue({ messages } as unknown as MessageBatch<typeof messages[0]['body']>, env);
    expect(ai).toHaveBeenCalledTimes(20);
    expect((await env.DB.prepare('SELECT status FROM telegram_webhook_inbox WHERE update_id = 20').first<{ status: string }>())?.status).toBe('ignored');
  });

  it('recovers interrupted processing but does not repeat an uncertain send', async () => {
    env.TELEGRAM_BOT_TOKEN = 'test-token'; env.TELEGRAM_ALLOWED_CHAT_IDS = '42';
    const now = Date.now();
    for (const [id, status] of [[1, 'received'], [2, 'processing'], [3, 'sending']] as const) {
      await env.DB.prepare('INSERT INTO telegram_webhook_inbox (update_id, received_at, payload, status, updated_at) VALUES (?, ?, ?, ?, ?)')
        .bind(id, now, '{}', status, now - 180_000).run();
    }
    await worker.scheduled({} as ScheduledController, env);
    expect(send.mock.calls.map((call: any[]) => call[0].updateId).sort()).toEqual([1, 2]);
  });

  it('exposes failed and uncertain deliveries to admins without exposing payloads', async () => {
    env.DEV_ADMIN_TOKEN = 'admin';
    env.ENVIRONMENT = 'development';
    const now = Date.now();
    await env.DB.prepare('INSERT INTO telegram_webhook_inbox (update_id, received_at, payload, status, updated_at) VALUES (?, ?, ?, ?, ?)')
      .bind(31, now, '{"private":"secret"}', 'failed', now).run();
    await env.DB.prepare('INSERT INTO telegram_webhook_inbox (update_id, received_at, payload, status, updated_at) VALUES (?, ?, ?, ?, ?)')
      .bind(32, now, '{}', 'sending', now - 120_000).run();
    const get = () => app.request('/v1/admin/telegram/issues', {}, env);
    expect((await get()).status).toBe(401);
    const auth = { Authorization: 'Bearer admin' };
    const result = await app.request('/v1/admin/telegram/issues', { headers: auth }, env);
    const body = await result.text();
    expect(body).not.toContain('secret');
    expect(JSON.parse(body).issues).toHaveLength(2);
    expect((await app.request('/v1/admin/telegram/issues/32/retry', { method: 'POST', headers: auth }, env)).status).toBe(404);
    expect((await app.request('/v1/admin/telegram/issues/31/retry', { method: 'POST', headers: auth }, env)).status).toBe(200);
    expect((await app.request('/v1/admin/telegram/issues/32/resolve', { method: 'POST', headers: auth }, env)).status).toBe(200);
  });

  it('marks repeated AI failures for manual retry', async () => {
    env.TELEGRAM_BOT_TOKEN = 'test-token'; env.TELEGRAM_ALLOWED_CHAT_IDS = '42';
    env.AI = { run: async () => { throw new Error('unavailable'); } } as unknown as Ai;
    const ack = vi.fn(), retry = vi.fn();
    const message = { attempts: 5, body: { updateId: 55, payload: JSON.stringify({ update_id: 55, message: { text: 'Hi', chat: { id: 42, type: 'private' } } }), receivedAt: Date.now() }, ack, retry };
    await worker.queue({ messages: [message] } as unknown as MessageBatch<typeof message.body>, env);
    expect(ack).toHaveBeenCalledOnce(); expect(retry).not.toHaveBeenCalled();
    expect((await env.DB.prepare('SELECT status FROM telegram_webhook_inbox WHERE update_id = 55').first<{ status: string }>())?.status).toBe('failed');
  });
});

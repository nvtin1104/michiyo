import { describe, expect, it } from 'vitest';
import { CloudClient, newId, Note, PushRequest, TelegramBotConfig } from './index';

describe('shared records', () => {
  const note = { id: newId(), updated_at: 123, deleted_at: null, device_id: newId(), rev: 1, title: 'Test', body: 'Body', tags: [], pinned: false };
  it('creates ULIDs and validates records and tombstones', () => {
    expect(note.id).toMatch(/^[0-9A-HJKMNP-TV-Z]{26}$/);
    expect(Note.parse(note)).toEqual(note);
    expect(Note.parse({ ...note, deleted_at: 456 }).deleted_at).toBe(456);
    expect(Note.safeParse({ ...note, rev: -1 }).success).toBe(false);
  });
  it('limits each push batch to 500 records', () => {
    const item = { type: 'note', record: note };
    expect(PushRequest.safeParse({ records: Array(500).fill(item) }).success).toBe(true);
    expect(PushRequest.safeParse({ records: Array(501).fill(item) }).success).toBe(false);
  });
  it('validates telegram bot config defaults and constraints', () => {
    const cfg = TelegramBotConfig.parse({});
    expect(cfg.enabled).toBe(true);
    expect(cfg.hourly_limit).toBe(20);
    expect(cfg.allow_workspace_queries).toBe(true);
    expect(TelegramBotConfig.safeParse({ allowed_chat_ids: ['not-a-number'] }).success).toBe(false);
    expect(TelegramBotConfig.safeParse({ allowed_chat_ids: ['123456', '-987654'] }).success).toBe(true);
  });
  it('pushes and pulls sync records through CloudClient', async () => {
    const calls: { url: string; method?: string; auth?: string }[] = [];
    const client = new CloudClient({
      baseUrl: 'https://cloud.example.com/',
      token: 'md_test',
      fetchImpl: (async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = String(input);
        const headers = (init?.headers ?? {}) as Record<string, string>;
        calls.push({ url, method: init?.method, auth: headers.Authorization });
        if (url.endsWith('/v1/sync/push')) {
          return new Response(JSON.stringify({ accepted: [{ type: 'note', id: note.id, rev: 1 }], rejected: [] }), { status: 200 });
        }
        return new Response(JSON.stringify({ records: [{ type: 'note', record: note }], cursor: '1', more: false }), { status: 200 });
      }) as typeof fetch,
    });
    const pushRes = await client.push([{ type: 'note', record: note }]);
    expect(pushRes.accepted).toHaveLength(1);
    const pullRes = await client.pull('0');
    expect(pullRes.records).toHaveLength(1);
    expect(calls[0]?.auth).toBe('Bearer md_test');
  });
});

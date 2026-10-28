import { beforeEach, describe, expect, it } from 'vitest';
import { app } from '../src/app';
import type { Env } from '../src/env';
import { createD1 } from './d1';

const ADMIN = { Authorization: 'Bearer dev-admin-token' };
let env: Env;
const ctx = { waitUntil() {}, passThroughOnException() {} } as unknown as ExecutionContext;
const call = (path: string, init: RequestInit = {}) => app.request(path, init, env, ctx);
const post = (path: string, body: unknown, headers: Record<string, string> = {}) =>
  call(path, { method: 'POST', headers: { 'Content-Type': 'application/json', ...headers }, body: JSON.stringify(body) });

async function device(): Promise<Record<string, string>> {
  const { code } = (await (await post('/v1/pair/start', {}, ADMIN)).json()) as { code: string };
  const { token } = (await (await post('/v1/pair/complete', { code })).json()) as { token: string };
  return { Authorization: `Bearer ${token}` };
}

const note = (over: Record<string, unknown> = {}) => ({
  type: 'note',
  record: { id: 'N1', updated_at: 100, deleted_at: null, device_id: 'A', rev: 1, title: 't', body: 'b', tags: [], pinned: false, ...over },
});
const pushAs = async (auth: Record<string, string>, ...records: unknown[]) => (await (await post('/v1/sync/push', { records }, auth)).json()) as any;
const pullAs = async (auth: Record<string, string>, cursor = '0') => (await (await call(`/v1/sync/pull?cursor=${cursor}`, { headers: auth })).json()) as any;

beforeEach(() => {
  env = { DB: createD1(), ENCRYPTION_KEY: '', DEV_ADMIN_TOKEN: 'dev-admin-token', AI: {} as Ai };
});

describe('sync', () => {
  it('is only for paired devices', async () => {
    expect((await post('/v1/sync/push', { records: [] })).status).toBe(401);
    expect((await call('/v1/sync/pull')).status).toBe(401);
    const key = (await (await post('/v1/admin/api-keys', { name: 'k', scopes: ['ai:chat'] }, ADMIN)).json()) as { key: string };
    expect((await call('/v1/sync/pull', { headers: { Authorization: `Bearer ${key.key}` } })).status).toBe(401);
  });

  it('stores pushes and serves them by cursor, in order', async () => {
    const auth = await device();
    const r = await pushAs(auth, note(), note({ id: 'N2', updated_at: 101 }));
    expect(r.accepted.map((a: any) => a.id)).toEqual(['N1', 'N2']);
    const first = await pullAs(auth);
    expect(first.records.map((x: any) => x.record.id)).toEqual(['N1', 'N2']);
    expect(first.more).toBe(false);
    await pushAs(auth, note({ updated_at: 200, rev: 2, title: 'newer' }));
    const next = await pullAs(auth, first.cursor);
    expect(next.records).toHaveLength(1);
    expect(next.records[0].record.title).toBe('newer');
    expect((await pullAs(auth, next.cursor)).records).toEqual([]);
  });

  it('resolves conflicts last-write-wins with device id as tiebreak and is idempotent', async () => {
    const auth = await device();
    await pushAs(auth, note({ updated_at: 200, device_id: 'B', rev: 2, title: 'B wins' }));
    const stale = await pushAs(auth, note({ updated_at: 150, device_id: 'A', rev: 2, title: 'A loses' }));
    expect(stale.rejected).toHaveLength(1);
    const tie = await pushAs(auth, note({ updated_at: 200, device_id: 'A', rev: 3 }));
    expect(tie.rejected).toHaveLength(1); // 'A' < 'B'
    const tieWin = await pushAs(auth, note({ updated_at: 200, device_id: 'C', rev: 3, title: 'C wins' }));
    expect(tieWin.accepted).toHaveLength(1);
    const again = await pushAs(auth, note({ updated_at: 200, device_id: 'C', rev: 3, title: 'C wins' }));
    expect(again.accepted).toHaveLength(1);
    const { records, cursor } = await pullAs(auth);
    expect(records).toHaveLength(1);
    expect(records[0].record.title).toBe('C wins');
    // A retried identical push must not generate a fresh change for other devices.
    await pushAs(auth, note({ updated_at: 200, device_id: 'C', rev: 3, title: 'C wins' }));
    expect((await pullAs(auth, cursor)).records).toEqual([]);
  });

  it('retains old tombstones and rejects a stale offline device', async () => {
    const auth = await device();
    await pushAs(auth, note({ updated_at: 300, deleted_at: 300, rev: 2 }));
    const stale = await pushAs(auth, note({ updated_at: 100, rev: 1 }));
    expect(stale.rejected).toHaveLength(1);
    expect((await pullAs(auth)).records[0].record.deleted_at).toBe(300);
  });

  it('rejects malformed batches and pages large pulls', async () => {
    const auth = await device();
    for (const bad of [{ records: [{ type: 'bogus', record: {} }] }, { records: [note({ rev: -1 })] }, { nope: 1 }]) {
      expect((await post('/v1/sync/push', bad, auth)).status).toBe(400);
    }
    expect((await call('/v1/sync/pull?cursor=abc', { headers: auth })).status).toBe(400);
    const batch = Array.from({ length: 500 }, (_, i) => note({ id: `A${i}`, updated_at: i + 1 }));
    await pushAs(auth, ...batch);
    await pushAs(auth, note({ id: 'LAST', updated_at: 999 }));
    const page = await pullAs(auth);
    expect(page.records).toHaveLength(500);
    expect(page.more).toBe(true);
    const rest = await pullAs(auth, page.cursor);
    expect(rest.records.map((r: any) => r.record.id)).toEqual(['LAST']);
    expect(rest.more).toBe(false);
  });

  it('summarises and wipes synced data for admins only', async () => {
    const auth = await device();
    await pushAs(auth, note(), note({ id: 'N2', deleted_at: 5, updated_at: 5 }));
    expect((await call('/v1/admin/sync/summary')).status).toBe(401);
    const { summary } = (await (await call('/v1/admin/sync/summary', { headers: ADMIN })).json()) as any;
    expect(summary).toEqual([{ type: 'note', records: 2, tombstones: 1 }]);
    expect(((await (await call('/v1/admin/sync', { method: 'DELETE', headers: ADMIN })).json()) as any).deleted).toBe(2);
    expect((await pullAs(auth)).records).toEqual([]);
  });

  it('lists only live records of an allowed type for admins', async () => {
    const auth = await device();
    await pushAs(auth, note(), note({ id: 'deleted', updated_at: 200, deleted_at: 200 }));
    expect((await call('/v1/admin/sync/records?type=note')).status).toBe(401);
    expect((await call('/v1/admin/sync/records?type=message', { headers: ADMIN })).status).toBe(400);
    expect((await call('/v1/admin/sync/records?type=note&cursor=1x', { headers: ADMIN })).status).toBe(400);
    const res = await call('/v1/admin/sync/records?type=note', { headers: ADMIN });
    expect(res.headers.get('cache-control')).toBe('no-store');
    const data = await res.json() as any;
    expect(data.records.map((record: any) => record.id)).toEqual(['N1']);
    expect(data.more).toBe(false);
  });

  it('paginates admin records without returning tombstones', async () => {
    const auth = await device();
    await pushAs(auth, ...Array.from({ length: 101 }, (_, i) => note({ id: `page-${i}`, updated_at: i + 1 })));
    const first = await (await call('/v1/admin/sync/records?type=note', { headers: ADMIN })).json() as any;
    expect(first.records).toHaveLength(100);
    expect(first.more).toBe(true);
    const second = await (await call(`/v1/admin/sync/records?type=note&cursor=${first.cursor}`, { headers: ADMIN })).json() as any;
    expect(second.records.map((record: any) => record.id)).toEqual(['page-100']);
    expect(second.more).toBe(false);
  });

  it('keeps cursors increasing after the cloud data is wiped', async () => {
    const auth = await device();
    await pushAs(auth, note());
    const before = await pullAs(auth);
    await call('/v1/admin/sync', { method: 'DELETE', headers: ADMIN });
    await pushAs(auth, note({ id: 'new', updated_at: 200 }));
    const after = await pullAs(auth, before.cursor);
    expect(after.records.map((item: any) => item.record.id)).toEqual(['new']);
    expect(Number(after.cursor)).toBeGreaterThan(Number(before.cursor));
  });
});

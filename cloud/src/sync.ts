import { PushRequest, type PullResponse, type PushResponse, type SyncRecord } from '../../shared/src/sync';
import type { Env } from './env';

const PURGE_AFTER_MS = 30 * 24 * 60 * 60 * 1000;
const MAX_RECORD_BYTES = 1_100_000;
const PULL_LIMIT = 500;

export class SyncError extends Error {}

/**
 * Last-write-wins on (updated_at, device_id); an identical write is accepted without effect, so retries are safe.
 * Returns true when the incoming record is stored or already matches what is stored.
 */
async function store(env: Env, item: SyncRecord): Promise<boolean> {
  const r = item.record;
  const data = JSON.stringify(r);
  if (data.length > MAX_RECORD_BYTES) throw new SyncError('record_too_large');
  const next = '(SELECT COALESCE(MAX(server_seq), 0) + 1 FROM sync_records)';
  const result = await env.DB.prepare(
    `INSERT INTO sync_records (type, id, server_seq, updated_at, deleted_at, device_id, rev, data)
     VALUES (?, ?, ${next}, ?, ?, ?, ?, ?)
     ON CONFLICT(type, id) DO UPDATE SET server_seq = ${next}, updated_at = excluded.updated_at, deleted_at = excluded.deleted_at,
       device_id = excluded.device_id, rev = excluded.rev, data = excluded.data
     WHERE excluded.updated_at > sync_records.updated_at
        OR (excluded.updated_at = sync_records.updated_at AND excluded.device_id > sync_records.device_id)`,
  )
    .bind(item.type, r.id, r.updated_at, r.deleted_at, r.device_id, r.rev, data)
    .run();
  if (result.meta.changes) return true;
  const current = await env.DB.prepare('SELECT updated_at, device_id FROM sync_records WHERE type = ? AND id = ?')
    .bind(item.type, r.id)
    .first<{ updated_at: number; device_id: string }>();
  return current?.updated_at === r.updated_at && current.device_id === r.device_id;
}

export async function push(env: Env, body: unknown): Promise<PushResponse> {
  const parsed = PushRequest.safeParse(body);
  if (!parsed.success) throw new SyncError('invalid_batch');
  const response: PushResponse = { accepted: [], rejected: [] };
  for (const item of parsed.data.records) {
    const ref = { type: item.type, id: item.record.id, rev: item.record.rev };
    (await store(env, item)) ? response.accepted.push(ref) : response.rejected.push(ref);
  }
  return response;
}

export async function pull(env: Env, cursor: string | undefined): Promise<PullResponse & { more: boolean }> {
  const after = Number.parseInt(cursor ?? '0', 10);
  if (!Number.isSafeInteger(after) || after < 0) throw new SyncError('invalid_cursor');
  const { results } = await env.DB.prepare('SELECT type, server_seq, data FROM sync_records WHERE server_seq > ? ORDER BY server_seq LIMIT ?')
    .bind(after, PULL_LIMIT + 1)
    .all<{ type: string; server_seq: number; data: string }>();
  const page = results.slice(0, PULL_LIMIT);
  const last = page.at(-1);
  return {
    records: page.map((row) => ({ type: row.type, record: JSON.parse(row.data) })) as SyncRecord[],
    cursor: String(last ? last.server_seq : after),
    more: results.length > PULL_LIMIT,
  };
}

/** Tombstones older than 30 days are dropped; a device offline for longer must resync from scratch. */
export async function purgeTombstones(env: Env, now = Date.now()): Promise<number> {
  const r = await env.DB.prepare('DELETE FROM sync_records WHERE deleted_at IS NOT NULL AND deleted_at < ?').bind(now - PURGE_AFTER_MS).run();
  return r.meta.changes;
}

export async function deleteAllSyncedData(env: Env): Promise<number> {
  const r = await env.DB.prepare('DELETE FROM sync_records').run();
  return r.meta.changes;
}

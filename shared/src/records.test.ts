import { describe, expect, it } from 'vitest';
import { newId, Note, PushRequest } from './index';

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
});

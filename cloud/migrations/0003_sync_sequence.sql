-- Keep cursors monotonic even if records are deleted. Triggers run in the same
-- SQLite statement as the write, so a pull cannot observe a reserved sequence
-- before its record is visible.
CREATE TABLE sync_sequence (id INTEGER PRIMARY KEY CHECK (id = 1), value INTEGER NOT NULL);
INSERT INTO sync_sequence (id, value) SELECT 1, COALESCE(MAX(server_seq), 0) FROM sync_records;

CREATE TRIGGER sync_records_insert_seq AFTER INSERT ON sync_records BEGIN
    UPDATE sync_sequence SET value = value + 1 WHERE id = 1;
    UPDATE sync_records SET server_seq = (SELECT value FROM sync_sequence WHERE id = 1)
      WHERE type = NEW.type AND id = NEW.id;
END;

CREATE TRIGGER sync_records_update_seq AFTER UPDATE OF updated_at, deleted_at, device_id, rev, data ON sync_records BEGIN
    UPDATE sync_sequence SET value = value + 1 WHERE id = 1;
    UPDATE sync_records SET server_seq = (SELECT value FROM sync_sequence WHERE id = 1)
      WHERE type = NEW.type AND id = NEW.id;
END;

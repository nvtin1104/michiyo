CREATE TABLE devices (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    created_at INTEGER NOT NULL,
    last_seen INTEGER,
    revoked_at INTEGER
);
CREATE TABLE pairing_codes (
    code_hash TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    expires_at INTEGER NOT NULL
);
CREATE TABLE api_keys (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    key_hash TEXT NOT NULL UNIQUE,
    prefix TEXT NOT NULL,
    scopes TEXT NOT NULL,
    daily_limit INTEGER,
    created_at INTEGER NOT NULL,
    revoked_at INTEGER
);
-- Values are AES-GCM ciphertext; no route returns them.
CREATE TABLE provider_keys (
    provider TEXT PRIMARY KEY NOT NULL,
    ciphertext TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE usage (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    at INTEGER NOT NULL,
    device_id TEXT,
    key_id TEXT,
    route TEXT NOT NULL,
    model TEXT NOT NULL,
    input_chars INTEGER NOT NULL,
    output_chars INTEGER NOT NULL
);
CREATE INDEX usage_key_at ON usage (key_id, at);
CREATE INDEX usage_at ON usage (at);
CREATE TABLE rate_limits (
    bucket TEXT PRIMARY KEY NOT NULL,
    count INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);

-- One row per synced record (latest winner only). server_seq orders changes for pull cursors.
CREATE TABLE sync_records (
    type TEXT NOT NULL,
    id TEXT NOT NULL,
    server_seq INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER,
    device_id TEXT NOT NULL,
    rev INTEGER NOT NULL,
    data TEXT NOT NULL,
    PRIMARY KEY (type, id)
);
CREATE INDEX sync_records_seq ON sync_records (server_seq);
CREATE INDEX sync_records_deleted ON sync_records (deleted_at) WHERE deleted_at IS NOT NULL;

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

CREATE TABLE telegram_webhook_inbox (
    update_id INTEGER PRIMARY KEY NOT NULL,
    received_at INTEGER NOT NULL,
    payload TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'received',
    updated_at INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX telegram_inbox_status_updated ON telegram_webhook_inbox(status, updated_at);

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

CREATE TABLE notes (
    id TEXT PRIMARY KEY NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER,
    device_id TEXT NOT NULL,
    rev INTEGER NOT NULL,
    data TEXT NOT NULL CHECK (json_valid(data))
);
CREATE TABLE habits (
    id TEXT PRIMARY KEY NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER,
    device_id TEXT NOT NULL,
    rev INTEGER NOT NULL,
    data TEXT NOT NULL CHECK (json_valid(data))
);
CREATE TABLE habit_logs (
    id TEXT PRIMARY KEY NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER,
    device_id TEXT NOT NULL,
    rev INTEGER NOT NULL,
    data TEXT NOT NULL CHECK (json_valid(data))
);
CREATE TABLE events (
    id TEXT PRIMARY KEY NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER,
    device_id TEXT NOT NULL,
    rev INTEGER NOT NULL,
    data TEXT NOT NULL CHECK (json_valid(data))
);
CREATE TABLE reminders (
    id TEXT PRIMARY KEY NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER,
    device_id TEXT NOT NULL,
    rev INTEGER NOT NULL,
    data TEXT NOT NULL CHECK (json_valid(data))
);
CREATE TABLE activity (
    id TEXT PRIMARY KEY NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER,
    device_id TEXT NOT NULL,
    rev INTEGER NOT NULL,
    data TEXT NOT NULL CHECK (json_valid(data))
);
CREATE TABLE conversations (
    id TEXT PRIMARY KEY NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER,
    device_id TEXT NOT NULL,
    rev INTEGER NOT NULL,
    data TEXT NOT NULL CHECK (json_valid(data))
);
CREATE TABLE messages (
    id TEXT PRIMARY KEY NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER,
    device_id TEXT NOT NULL,
    rev INTEGER NOT NULL,
    data TEXT NOT NULL CHECK (json_valid(data))
);
CREATE TABLE outbox (id TEXT NOT NULL, kind TEXT NOT NULL, rev INTEGER NOT NULL, data TEXT NOT NULL CHECK (json_valid(data)), PRIMARY KEY (id, kind, rev));
CREATE TABLE sync_state (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL);
CREATE VIRTUAL TABLE notes_fts USING fts5(id UNINDEXED, title, body);

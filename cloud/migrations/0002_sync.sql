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

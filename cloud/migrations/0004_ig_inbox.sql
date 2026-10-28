-- Raw verified deliveries are retained for later flow processing. A digest
-- deduplicates identical deliveries, not individual events across batches.
CREATE TABLE ig_webhook_inbox (
    digest TEXT PRIMARY KEY NOT NULL,
    received_at INTEGER NOT NULL,
    payload TEXT NOT NULL
);

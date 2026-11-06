-- The device created by the desktop app's one-time bootstrap is the owner and may administer the Worker.
ALTER TABLE devices ADD COLUMN owner INTEGER NOT NULL DEFAULT 0;

-- Each BOOTSTRAP_SECRET (stored as sha256) can create an owner device only once.
CREATE TABLE bootstrap_used (
    secret_hash TEXT PRIMARY KEY NOT NULL,
    used_at INTEGER NOT NULL
);

-- One-time, short-lived codes the owner device exchanges for a web admin session.
CREATE TABLE login_codes (
    code_hash TEXT PRIMARY KEY NOT NULL,
    expires_at INTEGER NOT NULL
);

-- Web admin sessions (HttpOnly cookie); only the sha256 of the cookie value is stored.
CREATE TABLE admin_sessions (
    id_hash TEXT PRIMARY KEY NOT NULL,
    expires_at INTEGER NOT NULL
);

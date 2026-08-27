CREATE TABLE users (
    id            INTEGER PRIMARY KEY,
    username      TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    created_at    INTEGER NOT NULL  -- unix 秒
);

CREATE TABLE sessions (
    token_hash TEXT PRIMARY KEY,   -- SHA-256(session token)，不落明文
    user_id    INTEGER NOT NULL REFERENCES users(id),
    expires_at INTEGER NOT NULL    -- unix 秒
);

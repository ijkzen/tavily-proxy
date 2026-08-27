CREATE TABLE proxy_keys (
    id            INTEGER PRIMARY KEY,
    name          TEXT NOT NULL,
    key_hash      TEXT NOT NULL UNIQUE,      -- SHA-256(token)，不落明文
    key_tail      TEXT NOT NULL,             -- 尾号 4 位，仅用于展示
    revoked       INTEGER NOT NULL DEFAULT 0,
    total_credits INTEGER NOT NULL DEFAULT 0,
    last_used_at  INTEGER,
    created_at    INTEGER NOT NULL
);

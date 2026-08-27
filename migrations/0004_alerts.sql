CREATE TABLE alerts (
    id              INTEGER PRIMARY KEY,
    upstream_key_id INTEGER REFERENCES upstream_keys(id),
    kind            TEXT NOT NULL,   -- quota_poll_failed / key_invalid 等
    message         TEXT NOT NULL,
    created_at      INTEGER NOT NULL
);

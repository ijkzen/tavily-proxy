-- research 任务：request_id → 提交时所用上游密钥的绑定（票 10）
-- 轮询必须落在同一 key；持久化后重启不丢映射。
CREATE TABLE research_tasks (
    request_id      TEXT PRIMARY KEY,
    upstream_key_id INTEGER NOT NULL REFERENCES upstream_keys(id),
    proxy_key_id    INTEGER REFERENCES proxy_keys(id),
    created_at      INTEGER NOT NULL
);

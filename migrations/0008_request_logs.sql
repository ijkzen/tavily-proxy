-- 请求日志（票 11）：每次 MCP 工具调用的完整落账，保留 30 天
CREATE TABLE request_logs (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    proxy_key_id    INTEGER REFERENCES proxy_keys(id),
    tool            TEXT NOT NULL,
    params_summary  TEXT,
    upstream_key_id INTEGER REFERENCES upstream_keys(id),
    credits         INTEGER NOT NULL DEFAULT 0,
    duration_ms     INTEGER NOT NULL DEFAULT 0,
    success         INTEGER NOT NULL,
    error           TEXT,
    created_at      INTEGER NOT NULL
);
CREATE INDEX idx_request_logs_created_at ON request_logs(created_at);
CREATE INDEX idx_request_logs_proxy_key ON request_logs(proxy_key_id);

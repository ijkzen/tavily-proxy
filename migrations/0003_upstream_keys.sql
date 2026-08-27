CREATE TABLE upstream_keys (
    id               INTEGER PRIMARY KEY,
    nickname         TEXT NOT NULL,
    key_ciphertext   TEXT NOT NULL,          -- AES-256-GCM(nonce‖密文) 的 hex
    key_tail         TEXT NOT NULL,          -- 尾号 4 位，仅用于展示
    status           TEXT NOT NULL DEFAULT 'active',  -- active/cooling/exhausted/disabled
    reset_day        INTEGER NOT NULL DEFAULT 1,      -- 计费周期重置日（每月几号）
    usage_cached     INTEGER NOT NULL DEFAULT 0,      -- 账号已用 credits（票 05 填充）
    limit_cached     INTEGER,                         -- 账号总额度；NULL = 未知
    usage_fetched_at INTEGER,                         -- 上次成功轮询 /usage 的时间
    created_at       INTEGER NOT NULL
);

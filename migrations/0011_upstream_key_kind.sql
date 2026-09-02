-- 上游密钥支持 Tavily / Exa 两类提供商：
-- kind: 'tavily' | 'exa'；存量密钥默认 tavily。
-- quota_reset_at: Exa 的本地额度重置点（unix 秒）；Exa 无公开余额接口，
-- 额度按「每月 10 美元」本地记账，到重置点回滚。Tavily 不用此列（按 reset_day 计算）。
ALTER TABLE upstream_keys ADD COLUMN kind TEXT NOT NULL DEFAULT 'tavily';
ALTER TABLE upstream_keys ADD COLUMN quota_reset_at INTEGER;

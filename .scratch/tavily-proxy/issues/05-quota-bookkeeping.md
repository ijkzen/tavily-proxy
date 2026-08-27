# 05 — 额度簿记

**What to build:** 上游密钥的额度数据源（ADR-0002）：后台每 60 秒（可配置）对每个上游密钥轮询 `GET /usage`；有效剩余额度 = (key.limit ?? account.plan_limit) − 账号已用量，正确建模「同账号多 key 共享额度」；每次请求后用响应里的 usage.credits 做本地扣减；轮询失败时降级为纯本地估算并告警。

**Blocked by:** 04 — 上游密钥池管理

**Status:** ready-for-agent

- [ ] 周期轮询 GET /usage 并落库（usage/limit/账号维度），周期可配置
- [ ] 有效剩余额度按账号粒度计算，同账号多 key 不重复计额度
- [ ] usage.credits 本地扣减在两次轮询间保持数据新鲜
- [ ] 轮询失败静默降级为本地估算并产生告警记录
- [ ] 测试：mock 上游提供 /usage 响应驱动刷新；失败降级路径

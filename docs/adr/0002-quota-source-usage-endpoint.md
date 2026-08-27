# 额度数据以 GET /usage 为主数据源

选路所需的上游密钥剩余额度，以周期性调用 Tavily `GET /usage` 为准（默认每 60 秒每 key 一次），两次轮询之间用请求响应中的 `usage.credits` 做本地扣减保持新鲜；轮询失败时降级为纯本地估算。

**关键事实**：Tavily 按**账号**粒度计量额度（2026-08-27 实测：`GET /usage` 返回 `{key: {usage, limit, ...}, account: {plan_usage, plan_limit, ...}}`，`key.limit` 通常为 null）。有效剩余额度 = `(key.limit ?? account.plan_limit) − account.plan_usage`——同一账号的多个 key 共享额度，池化它们只能缓解每分钟限流，不能叠加总额度。

**Considered Options**：手动配置每个 key 的月额度（不需要未文档化接口，但会漂移、运营负担大）；纯本地 `usage.credits` 累计（不知道计费周期重置点，且无法感知代理之外的消耗）。

**风险**：`GET /usage` 是未文档化端点，响应结构由实测确认、无官方承诺，且据第三方说法有限流（约 10 次/10 分钟——故轮询周期不宜过短）。接口行为变化时应静默降级并告警。

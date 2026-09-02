# 双提供商（Tavily + Exa）分组随机选路与 Exa 本地额度记账

代理从单一 Tavily 扩展为同时支持 Tavily 与 Exa 两类上游。MCP 工具表面不变（仍只暴露 `tavily_search` / `tavily_extract`），但每次工具调用先在提供商之间做**组随机**，选中组内再做**轮询**，实现「两条独立通道、成本均摊」的目标。

## 选路策略（替代 ADR-0002 的「剩余额度最多优先」）

- **组随机**：有健康密钥（`status = 'active'`）的提供商组等概率被选中。若只有一组有 key，则固定走该组。
- **组内轮询**：组内按轮询游标（`AppState.rr_cursor`，每提供商一个 `AtomicUsize`）循环取 key，不再比较额度高低。
- 原「按剩余额度最多选 key」废弃——理由：组内额度最高的 key 会被持续打满，轮询让成本均匀摊到每把 key 上；额度信息仍用于展示与排名（看板），不参与选路。

## Exa 的差异点

| 维度 | Tavily | Exa |
|---|---|---|
| 基址 | `https://api.tavily.com`（`TAVILY_BASE_URL`） | `https://api.exa.ai`（`EXA_BASE_URL`） |
| 鉴权 | `Authorization: Bearer` | `x-api-key` 头 |
| search 端点 | `POST /search` | `POST /search` |
| extract 端点 | `POST /extract` | `POST /contents` |
| 参数风格 | 下划线（`max_results` 等） | 驼峰（`numResults` 等），由 provider 翻译 |
| 用量单位 | credits（`usage.credits`） | 美元（`costDollars.total`） |
| 余额查询 | `GET /usage`（未文档化） | **无公开接口**（仅 dashboard 可见） |
| 耗尽状态码 | 432 / 433 | 402（`NO_MORE_CREDITS` 等） |

## Exa 本地额度记账（ADR-0002 的 Exa 分支）

Exa 没有可查询的余额 API，采用本地记账：

- 创建 Exa 密钥时初始化：`usage_cached = 0`、`limit_cached = 10.0`（每月 10 美元免费额度）、`quota_reset_at` = 下月 1 号 0 点（UTC）。
- 每次请求成功后按响应的 `costDollars.total` 累加 `usage_cached`（美元，保留 4 位小数）。
- **惰性重置**：跨过重置点后首次请求会把 `usage_cached` 回滚到最近重置点（`quota_reset_at`）并重新累计；`exhausted_until` 也指向 `quota_reset_at`。
- Tavily 维持原有轮询与本地扣减；Exa 密钥**不参与** `/usage` 轮询（无此接口，避免告警噪音）。

## 提供商抽象

`src/provider.rs` 定义 `Provider`（kind / base_url / 端点路径 / 鉴权 / 参数翻译 / 成本提取 / 耗尽判定），`AppState.providers` 按 `Kind` 索引。MCP 层 `call_sync_tool` 先调 `balancer::pick_group` 组随机，再按选中组的原生参数翻译与鉴权执行；research（`tavily_research`）固定走 Tavily 组。

## 密钥池展示

上游密钥新增 `kind` 列（`tavily` / `exa`，创建时显式选择，未传时按 `exa-` 前缀推断）。看板按提供商分组、组内按剩余额度（tavily credits / exa 美元）降序排名展示；日志行标注实际命中的提供商。

**Considered Options**：
- Exa 余额手动填写 → 无公开接口，人工维护成本高，且与「每月 10 美元」常态不符，拒绝。
- 不暴露 exa_* 工具名、仅用 tavily_search 统一入口 → 调用方无需感知提供商，符合「内部再均衡」意图；代价是工具参数需兼容两家（exa 侧做驼峰翻译，tavily 参数直通）。

**风险**：Exa 的 `costDollars.total` 与实际计费可能因计划/折扣有偏差（本地记账是估算）；`quota_reset_at` 用「下月 1 号」近似真实重置日，若有出入会导致展示失真但不影响可用性（耗尽由 402 状态码兜底）。

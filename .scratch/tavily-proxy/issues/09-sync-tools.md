# 09 — extract / crawl / map 三个同步工具

**What to build:** 补齐剩余三个同步工具：`tavily_extract`、`tavily_crawl`、`tavily_map`，名称与参数和官方完全一致，分别转发到上游 REST /extract、/crawl、/map；走 07/08 建好的管道，自动获得鉴权、include_usage 注入、选路与失败转移。

**Blocked by:** 07 — MCP 端点 + tavily_search 直转

**Status:** resolved

- [ ] 三个工具名称与参数表面和官方一致，结果原样返回
- [ ] 三个工具的请求都经过选路与状态机（复用而非另写）
- [ ] 测试：三个工具各一次端到端透传 + 至少一次失败转移

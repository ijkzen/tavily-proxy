# REST 直连而非 MCP 桥接

代理自己实现 MCP 服务端（Rust `rmcp`），把工具调用转发为对 `api.tavily.com` 的 REST 请求，而不是把 JSON-RPC 桥接到官方远程 MCP（`mcp.tavily.com`）。

**原因**：核心价值是「按剩余额度最多」在多个上游密钥间选路，这要求每次请求由我们选 key、并精确记录每个 key 的消耗（REST 响应支持 `include_usage` 返回 credits）。MCP 桥接拿不到用量数据，选路粒度也不受控。

**代价**：我们需要自行维护与官方一致的 5 个工具表面（tavily_search/extract/crawl/map/research）及错误映射（429 冷却 / 432·433 耗尽 / 401 禁用 / 400 透传），上游 API 变更时要跟进。

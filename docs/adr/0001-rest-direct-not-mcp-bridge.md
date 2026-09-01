# REST 直连而非 MCP 桥接

代理自己实现 MCP 服务端（Rust `rmcp`），把工具调用转发为对 `api.tavily.com` 的 REST 请求，而不是把 JSON-RPC 桥接到官方远程 MCP（`mcp.tavily.com`）。

**原因**：核心价值是「按剩余额度最多」在多个上游密钥间选路，这要求每次请求由我们选 key、并精确记录每个 key 的消耗（REST 响应支持 `include_usage` 返回 credits）。MCP 桥接拿不到用量数据，选路粒度也不受控。

**代价**：我们需要自行维护与官方一致的 5 个工具表面（tavily_search/extract/crawl/map/research）及错误映射（429 冷却 / 432·433 耗尽 / 401 禁用 / 400 透传），上游 API 变更时要跟进。

**修订（2026-08-27）**：最终未使用 `rmcp` 与 `tower-sessions`。MCP Streamable HTTP 的无状态形态（POST /mcp + application/json 响应，无 Mcp-Session-Id）很小，手写 JSON-RPC 分发（src/mcp.rs）比引入 rmcp 更直接；会话同理用手签 cookie + SHA-256 哈希存库（src/auth.rs）。spec 中技术栈一栏的 rmcp/tower-sessions 相应作废。

**修订（2026-09-01）**：MCP 对外表面收敛为 `tavily_search` / `tavily_extract` 两个工具，crawl/map/research 不再向 MCP 客户端暴露（tools/list 不列出、tools/call 返回未知工具），仅保留其上游转发/编排代码（balancer.rs / research.rs）备内部使用。

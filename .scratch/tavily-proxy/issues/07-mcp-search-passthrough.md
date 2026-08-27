# 07 — MCP 端点 + tavily_search 直转

**What to build:** 第一个端到端通路（ADR-0001）：`POST /mcp` Streamable HTTP 端点（rmcp），代理密钥鉴权（`Authorization: Bearer` 或 `?key=` query，无效/吊销则拒绝）；`tavily_search` 工具名称与参数和官方完全一致，转发到上游 REST /search 并自动注入 include_usage=true；400 类参数错误原样透传给调用方。本票只做单 key 直转，选路与状态机在 08。

**Blocked by:** 04 — 上游密钥池管理；06 — 代理密钥管理

**Status:** resolved

- [ ] MCP 客户端能完成 initialize → tools/list → tools/call 全流程
- [ ] Bearer 与 ?key= 两种传 key 方式都可用；无效/吊销 key 被拒绝
- [ ] tavily_search 参数表面与官方一致，结果原样返回
- [ ] 转发请求自动带 include_usage=true
- [ ] 上游 400 错误原样透传
- [ ] 测试：MCP 会话全流程 + mock 上游断言收到的请求体与头

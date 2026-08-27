# 10 — research 同步编排

**What to build:** `tavily_research` 工具（名称参数与官方一致）：提交 research 任务后同步等待完成再返回结果，与官方远程 MCP 行为一致；任务绑定提交时所用的上游密钥（request_id → upstream_key 持久化，轮询落在同一 key，服务重启后可恢复）；总超时 10 分钟（可配置），超时返回明确错误。

**Blocked by:** 08 — 选路器与状态机

**Status:** ready-for-agent

- [ ] 工具调用内部完成提交→轮询→返回最终结果，客户端无感异步
- [ ] 轮询请求绑定提交时所用的上游密钥，映射持久化可重启恢复
- [ ] 总超时 10 分钟，超时返回明确错误
- [ ] research 任务失败（上游 failed 状态）原样反馈给调用方
- [ ] 测试：mock 上游模拟 pending→completed 全流程；绑定 key 断言；超时路径

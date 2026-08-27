---
name: Bug report
about: 报告一个 bug
title: ''
labels: bug
assignees: ''
---

<!--
注意：本仓库的 issue 跟踪在本地 markdown（.scratch/），GitHub issue 仅用于讨论。
安全相关的问题请勿在此提交——见 SECURITY.md。
-->

**描述**
清晰简洁地描述这个 bug。

**复现步骤**
1. 调用 `tavily_search` / `tavily_research`，参数：`...`
2. 用代理密钥 `tp-...`（脱敏）或管理员身份操作
3. 观察到的行为

**期望行为**
应该发生什么。

**实际行为**
发生了什么。附上日志（`RUST_LOG` 输出）与 HTTP 状态码。

**环境**
- 部署方式：本地 `cargo run` / Docker / 生产 https://tavily.ijkzen.cn
- 操作系统：macOS / Linux / 其他
- 版本：`git rev-parse HEAD` 或最近一次部署时间

**补充**
其他上下文或截图。

# 01 — 行走骨架

**What to build:** 一个可运行的单二进制骨架：Rust（axum + sqlx/SQLite）服务启动后自动建库建表，内嵌 Vite+React 前端空白页，暴露 /healthz；同时建好黑盒测试基建——测试能启动真实 app（临时目录 SQLite、随机端口），并把 Tavily 上游指向本地 mock server（upstream base URL 做成配置项）。这是后续所有票的地基。

**Blocked by:** None — can start immediately

**Status:** resolved

- [ ] 构建产出单个二进制，启动后 /healthz 返回 200，/ 返回前端页面
- [ ] SQLite 首次启动自动创建并完成迁移
- [ ] 前端 pnpm 构建产物被内嵌进二进制（rust-embed），无需外部静态文件
- [ ] 集成测试基建：测试内启动真实 app 并断言 /healthz；mock 上游 server 可被配置为 upstream base URL

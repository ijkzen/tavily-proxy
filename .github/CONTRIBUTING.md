# Contributing to tavily-proxy

感谢你对 tavily-proxy 感兴趣！这是一个个人维护的小项目，但非常欢迎合理的贡献。

## 工作流

本仓库使用基于本地 markdown 文件的 issue 跟踪（`docs/agents/issue-tracker.md`），
**目前不接受 GitHub issue / PR**，改动请通过邮件或直接提 issue 讨论（见下方"联系"）。

## 报告问题

如果发现 bug 或希望提出新功能，请先：

1. 阅读 `CONTEXT.md`（领域术语）与 `docs/adr/`（架构决策），确认是否已有相关设计。
2. 描述清楚：期望行为、实际行为、复现步骤（尽量带上 `tavily_search` 等工具参数）、日志。

安全相关问题**不要**公开报告——见 `SECURITY.md`。

## 本地开发

依赖：Rust（stable）、Node ≥ 22 + pnpm（`corepack enable pnpm`）。

```bash
# 前端构建（rust-embed 编译期嵌入 web/dist，改前端后必须先构建）
cd web && pnpm install --frozen-lockfile && pnpm build && cd ..

# 测试（集成测试会启动真实 app + mock Tavily 上游）
cargo test --all-targets

# 代码质量（CI 会做同样的检查）
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

### 环境变量

见根 `README.md` 的"配置"一节。测试不需要任何 Tavily 密钥——集成测试把
`TAVILY_BASE_URL` 指向本地 mock server（`tests/mock_upstream.rs`）。

### 提交约定

- 提交信息用中文、动词开头（`feat:` / `fix:` / `chore:` / `docs:` / `style:` / `ci:`），
  参考 `git log` 的既有风格。
- 修改领域概念时同步更新 `CONTEXT.md`；做出架构决策时新增 `docs/adr/`。
- 改动尽量带测试：黑盒集成测试放在 `tests/`。

## 联系

问题与讨论：通过仓库 owner 的邮箱或 GitHub 站内信，或直接发起 issue 讨论。

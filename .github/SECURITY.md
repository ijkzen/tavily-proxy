# Security Policy

## Reporting a Vulnerability

tavily-proxy 是一个公网单用户代理服务，**安全边界**：

- 代理密钥（`tp-...`）与上游密钥（`tvly-...`）的机密性
- 管理界面的登录会话
- 加密存储的上游密钥密文的完整性

发现任何安全问题，**请不要**在 GitHub issue 或公开渠道报告。请通过仓库 owner
的邮箱（见 GitHub 仓库首页）私下报告，附上：

- 漏洞类型与影响范围
- 复现步骤（测试环境即可，不要用真实生产密钥）
- 建议的修复方案（可选）

## Supported Versions

本仓库**不做版本化发布**，`main` 分支即最新。问题修复会直接合入 `main` 并尽快部署到
生产（https://tavily.ijkzen.cn）。

## Disclosure Policy

- 确认有效的问题会在修复合入并部署后再公开披露。
- 严重性评估以对生产实例（代理密钥池、上游密钥、会话）的实际影响为准。

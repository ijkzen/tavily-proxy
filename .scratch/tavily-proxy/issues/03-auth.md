# 03 — 认证（首访引导 + 登录）

**What to build:** 单用户登录体系：数据库无用户时 Web 首访引导页开放（创建账号后永久关闭）；用户名+密码登录（argon2 哈希、HttpOnly+SameSite session cookie）、登出、设置页改密；连续登录失败限速。

**Blocked by:** 01 — 行走骨架

**Status:** ready-for-agent

- [ ] 无用户时首访引导可创建账号；有用户后引导页返回不可再用
- [ ] 密码以 argon2 哈希落库，登录成功建立 session cookie（HttpOnly+SameSite）
- [ ] 未登录访问管理页/管理 API 被拒绝
- [ ] 连续登录失败触发限速
- [ ] 已登录用户可在设置页修改密码
- [ ] 黑盒测试覆盖：引导开关、登录成/败、限速、改密

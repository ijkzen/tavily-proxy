# tavily-proxy

## 交互规则

- 所有需要向用户提出的问题（需求澄清、方案选择、设计决策、确认等），一律使用 AskUserQuestion 工具呈现为结构化选项，并给出推荐答案；不要在正文里直接罗列问题让用户自由作答。

## Agent skills

### Issue tracker

Issues and specs live as local markdown files under `.scratch/<feature>/` in this repo. See `docs/agents/issue-tracker.md`.

### Triage labels

The five canonical triage roles use label strings identical to their names (`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`). See `docs/agents/triage-labels.md`.

### Domain docs

Single-context layout: one `CONTEXT.md` plus `docs/adr/` at the repo root. See `docs/agents/domain.md`.

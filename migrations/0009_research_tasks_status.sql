-- research_tasks 补状态列：让持久化映射成为可消费的任务账本（code-review 修复）
-- status: running → completed | failed | interrupted（重启时仍在 running 的一律标记 interrupted）
ALTER TABLE research_tasks ADD COLUMN status TEXT NOT NULL DEFAULT 'running';
ALTER TABLE research_tasks ADD COLUMN finished_at INTEGER;

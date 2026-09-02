-- 请求日志明细：完整参数与响应体（弹窗明细展示用）。
-- params_json / response_json 为完整 JSON 文本，与摘要列 params_summary 并存；
-- 响应体可能较大（extract 的 Markdown 可达数百 KB），随 30 天保留期一起清理。
ALTER TABLE request_logs ADD COLUMN params_json TEXT;
ALTER TABLE request_logs ADD COLUMN response_json TEXT;

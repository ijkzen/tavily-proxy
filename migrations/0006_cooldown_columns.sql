ALTER TABLE upstream_keys ADD COLUMN cooling_until INTEGER;    -- 冷却截止时间（unix 秒）
ALTER TABLE upstream_keys ADD COLUMN exhausted_until INTEGER;  -- 耗尽恢复时间（重置日 0 点）

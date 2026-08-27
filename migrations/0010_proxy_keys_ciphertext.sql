-- 代理密钥双写：key_ciphertext 存 AES-GCM 密文，支持事后明文可见/复制（key_hash 仍用于验证）
-- 旧行 key_ciphertext 为 NULL，无法补回明文，reveal 返回 409
ALTER TABLE proxy_keys ADD COLUMN key_ciphertext TEXT;

//! 上游密钥的静态加密：AES-256-GCM。
//! 主密钥解析顺序：环境变量 MASTER_KEY（64 位 hex）→ meta 表自动生成并持久化。
//! 后者只防「库文件单独泄露」，不防整台主机失陷——公网部署建议显式设 MASTER_KEY。

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use anyhow::{Context, bail};
use rand::RngCore;
use sqlx::SqlitePool;

#[derive(Clone)]
pub struct Crypto {
    key: [u8; 32],
}

impl Crypto {
    pub async fn load(db: &SqlitePool) -> anyhow::Result<Self> {
        if let Ok(hex_key) = std::env::var("MASTER_KEY") {
            let bytes = hex::decode(hex_key.trim()).context("MASTER_KEY 不是合法 hex")?;
            let key: [u8; 32] = bytes
                .try_into()
                .map_err(|_| anyhow::anyhow!("MASTER_KEY 必须是 32 字节（64 位 hex）"))?;
            return Ok(Self { key });
        }

        let existing: Option<String> =
            sqlx::query_scalar("SELECT value FROM meta WHERE key = 'master_key'")
                .fetch_optional(db)
                .await?;
        let key: [u8; 32] = match existing {
            Some(hex_key) => hex::decode(hex_key)
                .ok()
                .and_then(|b| b.try_into().ok())
                .context("meta 表中的 master_key 损坏")?,
            None => {
                let mut key = [0u8; 32];
                rand::rng().fill_bytes(&mut key);
                sqlx::query("INSERT INTO meta (key, value) VALUES ('master_key', ?)")
                    .bind(hex::encode(key))
                    .execute(db)
                    .await?;
                key
            }
        };
        Ok(Self { key })
    }

    /// 加密为 hex(nonce ‖ ciphertext)。
    pub fn encrypt(&self, plaintext: &str) -> anyhow::Result<String> {
        let mut nonce = [0u8; 12];
        rand::rng().fill_bytes(&mut nonce);
        let ciphertext = Aes256Gcm::new((&self.key).into())
            .encrypt(Nonce::from_slice(&nonce), plaintext.as_bytes())
            .map_err(|_| anyhow::anyhow!("加密失败"))?;
        let mut blob = nonce.to_vec();
        blob.extend_from_slice(&ciphertext);
        Ok(hex::encode(blob))
    }

    pub fn decrypt(&self, blob: &str) -> anyhow::Result<String> {
        let bytes = hex::decode(blob).context("密文不是合法 hex")?;
        if bytes.len() < 13 {
            bail!("密文太短");
        }
        let (nonce, ciphertext) = bytes.split_at(12);
        let plaintext = Aes256Gcm::new((&self.key).into())
            .decrypt(Nonce::from_slice(nonce), ciphertext)
            .map_err(|_| anyhow::anyhow!("解密失败"))?;
        String::from_utf8(plaintext).context("明文不是合法 UTF-8")
    }
}

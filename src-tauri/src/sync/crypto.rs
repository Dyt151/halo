//! 端到端加密：argon2 口令派生 + chacha20poly1305 AEAD。

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};

/// 加密工具（对称 AEAD + KDF）。
pub struct Crypto;

impl Crypto {
    /// 由口令 + 盐派生 32 字节密钥（Argon2id）。
    pub fn derive_key(passphrase: &str, salt: &[u8]) -> Result<[u8; 32], String> {
        if passphrase.is_empty() {
            return Err("passphrase is empty".to_string());
        }
        let params = Params::new(65536, 3, 1, Some(32)).map_err(|e| e.to_string())?;
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        let mut key = [0u8; 32];
        argon2
            .hash_password_into(passphrase.as_bytes(), salt, &mut key)
            .map_err(|e| e.to_string())?;
        Ok(key)
    }

    /// 生成 16 字节随机盐。
    pub fn generate_salt() -> [u8; 16] {
        let id = uuid::Uuid::new_v4();
        let mut salt = [0u8; 16];
        salt.copy_from_slice(id.as_bytes());
        salt
    }

    /// 生成 12 字节随机 nonce。
    fn generate_nonce() -> [u8; 12] {
        let id = uuid::Uuid::new_v4();
        let mut nonce = [0u8; 12];
        nonce.copy_from_slice(&id.as_bytes()[..12]);
        nonce
    }

    /// 加密：返回 `nonce(12) || ciphertext`。
    pub fn encrypt(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, String> {
        let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
        let nonce_bytes = Self::generate_nonce();
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher.encrypt(nonce, plaintext).map_err(|e| e.to_string())?;
        let mut out = Vec::with_capacity(12 + ciphertext.len());
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    /// 解密：输入为 `nonce(12) || ciphertext`。
    pub fn decrypt(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>, String> {
        if data.len() < 12 {
            return Err("ciphertext too short".to_string());
        }
        let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
        let nonce = Nonce::from_slice(&data[..12]);
        cipher
            .decrypt(nonce, &data[12..])
            .map_err(|e| e.to_string())
    }
}

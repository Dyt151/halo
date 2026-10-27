//! 同步 E2E 加密（chacha20poly1305 + argon2）测试。

use crate::sync::crypto::Crypto;

#[test]
fn derive_key_rejects_empty_passphrase() {
    assert!(Crypto::derive_key("", &[0u8; 16]).is_err());
}

#[test]
fn derive_key_is_deterministic_and_password_sensitive() {
    let salt = [7u8; 16];
    let k1 = Crypto::derive_key("correct horse", &salt).unwrap();
    let k2 = Crypto::derive_key("correct horse", &salt).unwrap();
    let k3 = Crypto::derive_key("wrong horse", &salt).unwrap();
    assert_eq!(k1, k2);
    assert_ne!(k1, k3);
    assert_eq!(k1.len(), 32);
}

#[test]
fn generate_salt_is_16_bytes_and_random() {
    let s1 = Crypto::generate_salt();
    let s2 = Crypto::generate_salt();
    assert_eq!(s1.len(), 16);
    assert_ne!(s1, s2);
}

#[test]
fn encrypt_decrypt_roundtrip() {
    let salt = Crypto::generate_salt();
    let key = Crypto::derive_key("passphrase", &salt).unwrap();
    let plaintext: &[u8] = "hello 世界 🔐 敏感数据".as_bytes();
    let ciphertext = Crypto::encrypt(&key, plaintext).unwrap();
    // 密文 = 12 字节 nonce + AEAD 密文（含 16 字节 tag），必不等于明文。
    assert!(ciphertext.len() >= 12 + plaintext.len());
    assert_ne!(ciphertext.as_slice(), plaintext);
    let decrypted = Crypto::decrypt(&key, &ciphertext).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn tampered_ciphertext_fails_decryption() {
    let key = Crypto::derive_key("pw", &[1u8; 16]).unwrap();
    let mut ciphertext = Crypto::encrypt(&key, b"top secret").unwrap();
    let last = ciphertext.len() - 1;
    ciphertext[last] ^= 0xFF;
    assert!(Crypto::decrypt(&key, &ciphertext).is_err());
}

#[test]
fn tampered_nonce_fails_decryption() {
    let key = Crypto::derive_key("pw", &[2u8; 16]).unwrap();
    let mut ciphertext = Crypto::encrypt(&key, b"top secret").unwrap();
    ciphertext[0] ^= 0xFF; // 破坏 nonce 首个字节
    assert!(Crypto::decrypt(&key, &ciphertext).is_err());
}

#[test]
fn too_short_ciphertext_fails() {
    let key = [0u8; 32];
    assert!(Crypto::decrypt(&key, &[1, 2, 3]).is_err());
    assert!(Crypto::decrypt(&key, &[]).is_err());
}

#[test]
fn wrong_key_fails_decryption() {
    let k1 = Crypto::derive_key("alice", &[3u8; 16]).unwrap();
    let k2 = Crypto::derive_key("bob", &[3u8; 16]).unwrap();
    let ciphertext = Crypto::encrypt(&k1, b"message").unwrap();
    assert!(Crypto::decrypt(&k2, &ciphertext).is_err());
}

#[test]
fn empty_plaintext_roundtrip() {
    let key = Crypto::derive_key("pw", &[4u8; 16]).unwrap();
    let ciphertext = Crypto::encrypt(&key, b"").unwrap();
    assert_eq!(Crypto::decrypt(&key, &ciphertext).unwrap(), b"");
}

use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[cfg(feature = "encryption")]
use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
#[cfg(feature = "encryption")]
use rand::{rngs::OsRng, RngCore};

#[derive(Debug, Clone)]
pub struct EncryptionConfig {
    pub master_key: [u8; 32],
    pub key_rotation_enabled: bool,
    pub current_key_version: u32,
    pub historical_keys: HashMap<u32, [u8; 32]>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EncryptedField {
    pub data: String,
    pub nonce: String,
    pub key_version: u32,
}

pub struct FieldEncryption {
    config: EncryptionConfig,
}

impl FieldEncryption {
    pub fn new(config: EncryptionConfig) -> Self { Self { config } }

    pub fn from_env() -> Result<Self> {
        // Prefer OPENACT_MASTER_KEY; fallback AUTHFLOW_MASTER_KEY for compatibility
        let master_key_hex = std::env::var("OPENACT_MASTER_KEY")
            .or_else(|_| std::env::var("AUTHFLOW_MASTER_KEY"))
            .map_err(|_| anyhow!("OPENACT_MASTER_KEY environment variable not set"))?;
        let master_key = hex::decode(&master_key_hex)
            .map_err(|_| anyhow!("Invalid master key format, expected hex string"))?;
        if master_key.len() != 32 { return Err(anyhow!("Master key must be 32 bytes (64 hex)")); }
        let mut key_array = [0u8; 32];
        key_array.copy_from_slice(&master_key);
        let config = EncryptionConfig {
            master_key: key_array,
            key_rotation_enabled: std::env::var("OPENACT_KEY_ROTATION").unwrap_or_default() == "true",
            current_key_version: std::env::var("OPENACT_KEY_VERSION").unwrap_or_else(|_| "1".to_string()).parse().unwrap_or(1),
            historical_keys: HashMap::new(),
        };
        Ok(Self::new(config))
    }

    #[cfg(feature = "encryption")]
    pub fn encrypt_field(&self, plaintext: &str) -> Result<EncryptedField> {
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let cipher = Aes256Gcm::new_from_slice(&self.config.master_key)
            .map_err(|e| anyhow!("Failed to create cipher: {}", e))?;
        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| anyhow!("Encryption failed: {}", e))?;
        Ok(EncryptedField {
            data: STANDARD.encode(&ciphertext),
            nonce: STANDARD.encode(&nonce_bytes),
            key_version: self.config.current_key_version,
        })
    }

    #[cfg(not(feature = "encryption"))]
    pub fn encrypt_field(&self, plaintext: &str) -> Result<EncryptedField> {
        Ok(EncryptedField { data: STANDARD.encode(plaintext.as_bytes()), nonce: STANDARD.encode(b"no-encryption"), key_version: 0 })
    }

    #[cfg(feature = "encryption")]
    pub fn decrypt_field(&self, encrypted: &EncryptedField) -> Result<String> {
        let nonce_bytes = STANDARD.decode(&encrypted.nonce).map_err(|e| anyhow!("Failed to decode nonce: {}", e))?;
        if nonce_bytes.len() != 12 {
            if nonce_bytes.as_slice() == b"no-encryption" {
                let plaintext = STANDARD.decode(&encrypted.data).map_err(|e| anyhow!("Failed to decode data: {}", e))?;
                return String::from_utf8(plaintext).map_err(|e| anyhow!("Invalid UTF-8: {}", e));
            }
            return Err(anyhow!("Invalid nonce length: {}", nonce_bytes.len()));
        }
        let key = if encrypted.key_version == self.config.current_key_version {
            &self.config.master_key
        } else {
            self.config
                .historical_keys
                .get(&encrypted.key_version)
                .ok_or_else(|| anyhow!("Key version {} not found", encrypted.key_version))?
        };
        let ciphertext = STANDARD.decode(&encrypted.data).map_err(|e| anyhow!("Failed to decode ciphertext: {}", e))?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| anyhow!("Failed to create cipher: {}", e))?;
        let plaintext = cipher.decrypt(nonce, ciphertext.as_slice()).map_err(|e| anyhow!("Decryption failed: {}", e))?;
        String::from_utf8(plaintext).map_err(|e| anyhow!("Invalid UTF-8 in decrypted data: {}", e))
    }

    #[cfg(not(feature = "encryption"))]
    pub fn decrypt_field(&self, encrypted: &EncryptedField) -> Result<String> {
        if encrypted.key_version == 0 {
            let plaintext = STANDARD.decode(&encrypted.data).map_err(|e| anyhow!("Failed to decode data: {}", e))?;
            String::from_utf8(plaintext).map_err(|e| anyhow!("Invalid UTF-8 in data: {}", e))
        } else {
            Err(anyhow!("Encryption feature not enabled, cannot decrypt encrypted data"))
        }
    }
}

impl Default for EncryptionConfig {
    fn default() -> Self {
        Self { master_key: [0u8; 32], key_rotation_enabled: false, current_key_version: 1, historical_keys: HashMap::new() }
    }
}

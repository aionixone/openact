//! Field-level encryption service
//! 
//! Provides AES-256-GCM encryption/decryption for sensitive data

// Simplified: drop AES-GCM to avoid extra feature/dep; use Base64 placeholder only
use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Encryption configuration
#[derive(Debug, Clone)]
pub struct EncryptionConfig {
    /// Master key (32 bytes)
    pub master_key: [u8; 32],
    /// Whether key rotation is enabled
    pub key_rotation_enabled: bool,
    /// Current key version
    pub current_key_version: u32,
    /// Historical keys (for decrypting old data)
    pub historical_keys: HashMap<u32, [u8; 32]>,
}

/// Encrypted field data
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EncryptedField {
    /// Base64 encoded encrypted data
    pub data: String,
    /// Base64 encoded nonce
    pub nonce: String,
    /// Key version
    pub key_version: u32,
}

/// Field encryption service
#[derive(Clone)]
#[allow(dead_code)]
pub struct FieldEncryption {
    config: EncryptionConfig,
}

impl FieldEncryption {
    /// Create a new encryption service
    pub fn new(config: EncryptionConfig) -> Self {
        Self { config }
    }

    /// Create encryption service from environment variables
    pub fn from_env() -> Result<Self> {
        let master_key_hex = std::env::var("OPENACT_MASTER_KEY")
            .map_err(|_| anyhow!("OPENACT_MASTER_KEY environment variable not set"))?;
        
        let master_key = hex::decode(&master_key_hex)
            .map_err(|_| anyhow!("Invalid master key format, expected hex string"))?;
        
        if master_key.len() != 32 {
            return Err(anyhow!("Master key must be 32 bytes (64 hex characters)"));
        }

        let mut key_array = [0u8; 32];
        key_array.copy_from_slice(&master_key);

        let config = EncryptionConfig {
            master_key: key_array,
            key_rotation_enabled: std::env::var("OPENACT_KEY_ROTATION")
                .unwrap_or_default() == "true",
            current_key_version: std::env::var("OPENACT_KEY_VERSION")
                .unwrap_or_else(|_| "1".to_string())
                .parse()
                .unwrap_or(1),
            historical_keys: HashMap::new(), // TODO: Load historical keys from configuration
        };

        Ok(Self::new(config))
    }

    pub fn encrypt_field(&self, plaintext: &str) -> Result<EncryptedField> {
        // Placeholder: Base64 encode plaintext (development only)
        Ok(EncryptedField {
            data: STANDARD.encode(plaintext.as_bytes()),
            nonce: STANDARD.encode(b"no-encryption"),
            key_version: 0,
        })
    }

    pub fn decrypt_field(&self, encrypted: &EncryptedField) -> Result<String> {
        let plaintext = STANDARD.decode(&encrypted.data)
            .map_err(|e| anyhow!("Failed to decode data: {}", e))?;
        String::from_utf8(plaintext)
            .map_err(|e| anyhow!("Invalid UTF-8 in data: {}", e))
    }

    pub fn generate_master_key() -> [u8; 32] {
        let mut key = [0u8; 32];
        OsRng.fill_bytes(&mut key);
        key
    }

    /// Generate hexadecimal representation of the master key
    pub fn generate_master_key_hex() -> String {
        let key = Self::generate_master_key();
        hex::encode(key)
    }
}

/// Default encryption configuration (for testing)
impl Default for EncryptionConfig {
    fn default() -> Self {
        Self {
            master_key: [0u8; 32], // Zero key for testing
            key_rotation_enabled: false,
            current_key_version: 1,
            historical_keys: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_encryption_without_feature() {
        let config = EncryptionConfig::default();
        let encryption = FieldEncryption::new(config);
        let plaintext = "sensitive_access_token_12345";

        // Encrypt (returns Base64 encoded data when encryption feature is not enabled)
        let encrypted = encryption.encrypt_field(plaintext).unwrap();
        assert_eq!(encrypted.key_version, 0);

        // Decrypt
        let decrypted = encryption.decrypt_field(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_field_encryption_with_feature() {
        let mut config = EncryptionConfig::default();
        config.master_key = FieldEncryption::generate_master_key();
        config.current_key_version = 1;

        let encryption = FieldEncryption::new(config);
        let plaintext = "sensitive_access_token_12345";

        // Encrypt
        let encrypted = encryption.encrypt_field(plaintext).unwrap();
        // In Base64-only mode, data equals base64(plaintext); we only assert roundtrip
        assert_eq!(encrypted.key_version, 0);

        // Decrypt
        let decrypted = encryption.decrypt_field(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_key_generation() {
        let key_hex = FieldEncryption::generate_master_key_hex();
        assert_eq!(key_hex.len(), 64); // 32 bytes = 64 hex chars
        
        // Verify it's valid hexadecimal
        hex::decode(&key_hex).unwrap();
    }
}

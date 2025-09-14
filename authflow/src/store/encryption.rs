//! Field-level encryption service
//! 
//! Provides AES-256-GCM encryption/decryption for sensitive data

#[cfg(feature = "encryption")]
use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};
use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
#[cfg(feature = "encryption")]
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
        let master_key_hex = std::env::var("AUTHFLOW_MASTER_KEY")
            .map_err(|_| anyhow!("AUTHFLOW_MASTER_KEY environment variable not set"))?;
        
        let master_key = hex::decode(&master_key_hex)
            .map_err(|_| anyhow!("Invalid master key format, expected hex string"))?;
        
        if master_key.len() != 32 {
            return Err(anyhow!("Master key must be 32 bytes (64 hex characters)"));
        }

        let mut key_array = [0u8; 32];
        key_array.copy_from_slice(&master_key);

        let config = EncryptionConfig {
            master_key: key_array,
            key_rotation_enabled: std::env::var("AUTHFLOW_KEY_ROTATION")
                .unwrap_or_default() == "true",
            current_key_version: std::env::var("AUTHFLOW_KEY_VERSION")
                .unwrap_or_else(|_| "1".to_string())
                .parse()
                .unwrap_or(1),
            historical_keys: HashMap::new(), // TODO: Load historical keys from configuration
        };

        Ok(Self::new(config))
    }

    /// Encrypt field data
    #[cfg(feature = "encryption")]
    pub fn encrypt_field(&self, plaintext: &str) -> Result<EncryptedField> {
        // Generate nonce
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Create cipher
        let cipher = Aes256Gcm::new_from_slice(&self.config.master_key)
            .map_err(|e| anyhow!("Failed to create cipher: {}", e))?;

        // Encrypt data
        let ciphertext = cipher.encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| anyhow!("Encryption failed: {}", e))?;

        Ok(EncryptedField {
            data: STANDARD.encode(&ciphertext),
            nonce: STANDARD.encode(&nonce_bytes),
            key_version: self.config.current_key_version,
        })
    }

    /// Encrypt field data (placeholder when encryption feature is not enabled)
    #[cfg(not(feature = "encryption"))]
    pub fn encrypt_field(&self, plaintext: &str) -> Result<EncryptedField> {
        // When encryption is not enabled, return Base64 encoded plaintext (for development only)
        Ok(EncryptedField {
            data: STANDARD.encode(plaintext.as_bytes()),
            nonce: STANDARD.encode(b"no-encryption"),
            key_version: 0,
        })
    }

    /// Decrypt field data
    #[cfg(feature = "encryption")]
    pub fn decrypt_field(&self, encrypted: &EncryptedField) -> Result<String> {
        // Decode nonce first to detect legacy "no-encryption" format BEFORE key selection
        let nonce_bytes = STANDARD
            .decode(&encrypted.nonce)
            .map_err(|e| anyhow!("Failed to decode nonce: {}", e))?;

        // Backward compatibility: data written without encryption feature enabled stores
        // Base64("no-encryption") as the nonce and Base64(plaintext) as data.
        // In that case, treat it as plaintext instead of AES-GCM, regardless of key_version.
        if nonce_bytes.len() != 12 {
            if nonce_bytes.as_slice() == b"no-encryption" {
                let plaintext = STANDARD
                    .decode(&encrypted.data)
                    .map_err(|e| anyhow!("Failed to decode data: {}", e))?;
                return String::from_utf8(plaintext)
                    .map_err(|e| anyhow!("Invalid UTF-8 in data: {}", e));
            }
            return Err(anyhow!("Invalid nonce length: expected 12 bytes, got {}", nonce_bytes.len()));
        }

        // Select key only for real AES-GCM payloads
        let key = if encrypted.key_version == self.config.current_key_version {
            &self.config.master_key
        } else {
            self.config
                .historical_keys
                .get(&encrypted.key_version)
                .ok_or_else(|| anyhow!("Key version {} not found", encrypted.key_version))?
        };

        // Decode ciphertext after confirming AES-GCM path
        let ciphertext = STANDARD
            .decode(&encrypted.data)
            .map_err(|e| anyhow!("Failed to decode ciphertext: {}", e))?;
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Create cipher
        let cipher = Aes256Gcm::new_from_slice(key)
            .map_err(|e| anyhow!("Failed to create cipher: {}", e))?;

        // Decrypt data
        let plaintext = cipher
            .decrypt(nonce, ciphertext.as_slice())
            .map_err(|e| anyhow!("Decryption failed: {}", e))?;

        String::from_utf8(plaintext).map_err(|e| anyhow!("Invalid UTF-8 in decrypted data: {}", e))
    }

    /// Decrypt field data (placeholder when encryption feature is not enabled)
    #[cfg(not(feature = "encryption"))]
    pub fn decrypt_field(&self, encrypted: &EncryptedField) -> Result<String> {
        if encrypted.key_version == 0 {
            // When encryption is not enabled, decode Base64 plaintext
            let plaintext = STANDARD.decode(&encrypted.data)
                .map_err(|e| anyhow!("Failed to decode data: {}", e))?;
            String::from_utf8(plaintext)
                .map_err(|e| anyhow!("Invalid UTF-8 in data: {}", e))
        } else {
            Err(anyhow!("Encryption feature not enabled, cannot decrypt encrypted data"))
        }
    }

    /// Generate a new master key (for key rotation)
    #[cfg(feature = "encryption")]
    pub fn generate_master_key() -> [u8; 32] {
        let mut key = [0u8; 32];
        OsRng.fill_bytes(&mut key);
        key
    }

    /// Generate a new master key (placeholder when encryption feature is not enabled)
    #[cfg(not(feature = "encryption"))]
    pub fn generate_master_key() -> [u8; 32] {
        [0u8; 32] // Return zero-filled array
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
        #[cfg(feature = "encryption")]
        assert_eq!(encrypted.key_version, 1);
        #[cfg(not(feature = "encryption"))]
        assert_eq!(encrypted.key_version, 0);

        // Decrypt
        let decrypted = encryption.decrypt_field(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[cfg(feature = "encryption")]
    #[test]
    fn test_field_encryption_with_feature() {
        let mut config = EncryptionConfig::default();
        config.master_key = FieldEncryption::generate_master_key();
        config.current_key_version = 1;

        let encryption = FieldEncryption::new(config);
        let plaintext = "sensitive_access_token_12345";

        // Encrypt
        let encrypted = encryption.encrypt_field(plaintext).unwrap();
        assert_ne!(encrypted.data, STANDARD.encode(plaintext.as_bytes()));
        assert_eq!(encrypted.key_version, 1);

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

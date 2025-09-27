#[cfg(feature = "encryption")]
use aes_gcm::{
    aead::{generic_array::GenericArray, Aead, AeadCore, OsRng},
    Aes256Gcm, KeyInit, Nonce,
};
#[cfg(feature = "encryption")]
use base64::{engine::general_purpose, Engine as _};

#[cfg(feature = "encryption")]
pub struct Crypto {
    key: [u8; 32],
}

#[cfg(feature = "encryption")]
impl Crypto {
    pub fn from_env() -> Option<Self> {
        let key_b64 = std::env::var("OPENACT_ENC_KEY").ok()?;
        let mut key = [0u8; 32];
        let decoded = general_purpose::STANDARD.decode(key_b64).ok()?;
        if decoded.len() != 32 {
            return None;
        }
        key.copy_from_slice(&decoded);
        Some(Self { key })
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> (String, String) {
        let cipher = Aes256Gcm::new(GenericArray::from_slice(&self.key));
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher
            .encrypt(&nonce, plaintext)
            .expect("encryption failure");
        (
            general_purpose::STANDARD.encode(ciphertext),
            general_purpose::STANDARD.encode(nonce.as_slice()),
        )
    }

    pub fn decrypt(&self, b64_ciphertext: &str, b64_nonce: &str) -> Option<Vec<u8>> {
        let cipher = Aes256Gcm::new(GenericArray::from_slice(&self.key));
        let nonce_bytes = general_purpose::STANDARD.decode(b64_nonce).ok()?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = general_purpose::STANDARD.decode(b64_ciphertext).ok()?;
        cipher.decrypt(nonce, ciphertext.as_ref()).ok()
    }
}

#[cfg(not(feature = "encryption"))]
pub struct Crypto;
#[cfg(not(feature = "encryption"))]
impl Crypto {
    pub fn from_env() -> Option<Self> {
        None
    }
}

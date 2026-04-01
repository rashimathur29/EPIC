use aes_gcm::{Aes256Gcm, Key, Nonce};
use aes_gcm::aead::{Aead, KeyInit};
use base64::{Engine as _, engine::general_purpose};
use anyhow::{Result, anyhow};
use rand::Rng;

#[derive(Clone)]   
pub struct AES256Encryptor {
    cipher: Aes256Gcm,
}

impl AES256Encryptor {
    pub fn new(key: &str) -> Result<Self> {
        let key_bytes = general_purpose::STANDARD.decode(key)?;
        let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
        let cipher = Aes256Gcm::new(key);
        Ok(Self { cipher })
    }

    pub fn encrypt(&self, data: &str) -> Result<String> {
        let mut rng = rand::thread_rng();
        let mut nonce_bytes = [0u8; 12];
        rng.fill(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = self.cipher.encrypt(nonce, data.as_bytes())
            .map_err(|e| anyhow!("Encryption failed: {:?}", e))?;

        let mut result = nonce_bytes.to_vec();
        result.extend_from_slice(&ciphertext);
        Ok(general_purpose::STANDARD.encode(&result))
    }

    pub fn decrypt(&self, encrypted_data: &str) -> Result<String> {
        let data = general_purpose::STANDARD.decode(encrypted_data)?;
        if data.len() < 12 {
            return Err(anyhow!("Invalid encrypted data"));
        }
        let nonce = Nonce::from_slice(&data[..12]);
        let ciphertext = &data[12..];

        let plaintext = self.cipher.decrypt(nonce, ciphertext)
            .map_err(|e| anyhow!("Decryption failed: {:?}", e))?;

        String::from_utf8(plaintext)
            .map_err(|e| anyhow!("Invalid UTF-8 in decrypted data: {}", e))
    }
}

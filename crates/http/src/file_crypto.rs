//! AES-256-GCM для файлов заявок. В RustFS попадает только этот конверт;
//! ключ конкретной заявки выводится из мастер-секрета через HKDF-SHA256.

use aes_gcm::aead::{Aead as _, KeyInit as _, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use hkdf::Hkdf;
use sha2_compat::Sha256;
use uuid::Uuid;

const MAGIC: &[u8; 8] = b"TOURENT1";
const NONCE_LEN: usize = 12;
const MIN_MASTER_BYTES: usize = 16;

#[derive(Debug, thiserror::Error)]
pub enum FileCryptoError {
    #[error("переменная PRICE_ENCRYPTION_KEY не задана")]
    MissingMasterKey,
    #[error("PRICE_ENCRYPTION_KEY должен содержать не менее {MIN_MASTER_BYTES} байт")]
    WeakMasterKey,
    #[error("не удалось вывести ключ файла")]
    KeyDerivation,
    #[error("не удалось зашифровать файл")]
    Encryption,
    #[error("файл не является шифрованным конвертом TOU.Rent")]
    InvalidEnvelope,
    #[error("не удалось расшифровать файл: данные или ключ не совпадают")]
    Decryption,
}

pub struct FileCipher {
    master: Box<[u8]>,
}

impl std::fmt::Debug for FileCipher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileCipher").finish_non_exhaustive()
    }
}

impl FileCipher {
    pub fn from_env() -> Result<Self, FileCryptoError> {
        let master =
            std::env::var("PRICE_ENCRYPTION_KEY").map_err(|_| FileCryptoError::MissingMasterKey)?;
        Self::new(master.as_bytes())
    }

    pub fn new(master: &[u8]) -> Result<Self, FileCryptoError> {
        if master.len() < MIN_MASTER_BYTES {
            return Err(FileCryptoError::WeakMasterKey);
        }
        Ok(Self {
            master: master.into(),
        })
    }

    pub fn encrypt(
        &self,
        application_id: Uuid,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, FileCryptoError> {
        let key = self.application_key(application_id)?;
        let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| FileCryptoError::Encryption)?;
        let nonce_bytes: [u8; NONCE_LEN] = rand::random();
        let nonce = Nonce::from(nonce_bytes);
        let ciphertext = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext,
                    aad: application_id.as_bytes(),
                },
            )
            .map_err(|_| FileCryptoError::Encryption)?;

        let mut envelope = Vec::with_capacity(MAGIC.len() + NONCE_LEN + ciphertext.len());
        envelope.extend_from_slice(MAGIC);
        envelope.extend_from_slice(&nonce_bytes);
        envelope.extend_from_slice(&ciphertext);
        Ok(envelope)
    }

    pub fn decrypt(
        &self,
        application_id: Uuid,
        envelope: &[u8],
    ) -> Result<Vec<u8>, FileCryptoError> {
        let body = envelope
            .strip_prefix(MAGIC)
            .ok_or(FileCryptoError::InvalidEnvelope)?;
        let (nonce_bytes, ciphertext) = body
            .split_at_checked(NONCE_LEN)
            .ok_or(FileCryptoError::InvalidEnvelope)?;
        let nonce_array: [u8; NONCE_LEN] = nonce_bytes
            .try_into()
            .map_err(|_| FileCryptoError::InvalidEnvelope)?;
        let nonce = Nonce::from(nonce_array);
        let key = self.application_key(application_id)?;
        let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| FileCryptoError::Decryption)?;
        cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: ciphertext,
                    aad: application_id.as_bytes(),
                },
            )
            .map_err(|_| FileCryptoError::Decryption)
    }

    fn application_key(&self, application_id: Uuid) -> Result<[u8; 32], FileCryptoError> {
        let hkdf = Hkdf::<Sha256>::new(Some(b"TOU.Rent application files v1"), &self.master);
        let mut key = [0_u8; 32];
        hkdf.expand(application_id.as_bytes(), &mut key)
            .map_err(|_| FileCryptoError::KeyDerivation)?;
        Ok(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_preserves_pdf() {
        let application_id = Uuid::now_v7();
        let cipher = FileCipher::new(b"a-test-master-key-with-32-bytes!!").expect("cipher");
        let plaintext = b"%PDF-1.7\nprivate dossier";

        let encrypted = cipher.encrypt(application_id, plaintext).expect("encrypt");
        let decrypted = cipher.decrypt(application_id, &encrypted).expect("decrypt");

        assert_ne!(encrypted, plaintext);
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn another_application_cannot_decrypt_the_envelope() {
        let cipher = FileCipher::new(b"a-test-master-key-with-32-bytes!!").expect("cipher");
        let owner = Uuid::now_v7();
        let encrypted = cipher.encrypt(owner, b"%PDF-secret").expect("encrypt");

        let error = cipher
            .decrypt(Uuid::now_v7(), &encrypted)
            .expect_err("different application key must fail");

        assert!(matches!(error, FileCryptoError::Decryption));
    }

    #[test]
    fn short_master_key_is_rejected() {
        let error = FileCipher::new(b"short").expect_err("weak key must fail");
        assert!(matches!(error, FileCryptoError::WeakMasterKey));
    }
}

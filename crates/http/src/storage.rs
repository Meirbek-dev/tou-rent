//! Файловое хранилище RustFS по S3 API (арх. § 2, ADR-0006): бакет
//! `dossiers` - материалы тендера (сканы заявок; протоколы - Т9+). Доступ
//! к объектам решает http-слой (FR-403), подписанные URL не используются -
//! файлы ходят через api.
//!
//! Удаления здесь нет намеренно: `dossiers` живет под Object Lock в режиме
//! compliance (INV-042, ADR-0004), и учетной записи приложения право
//! `s3:DeleteObject` в этом бакете не выдано.

use std::sync::Arc;

use object_store::ObjectStore;
use object_store::aws::AmazonS3Builder;

pub type Storage = Arc<dyn ObjectStore>;

pub struct StorageConfig {
    pub endpoint: String,
    pub access_key: String,
    pub secret_key: String,
    pub bucket: String,
}

impl StorageConfig {
    /// Значения по умолчанию соответствуют dev-стенду compose (NFR-09:
    /// в проде задаются окружением/SOPS).
    pub fn from_env() -> Self {
        let var =
            |name: &str, default: &str| std::env::var(name).unwrap_or_else(|_| default.to_owned());
        Self {
            endpoint: var("S3_ENDPOINT", "http://localhost:9000"),
            access_key: var("S3_ACCESS_KEY", "tou_rent"),
            secret_key: var("S3_SECRET_KEY", "tou_rent_dev"),
            bucket: var("S3_BUCKET_DOSSIERS", "dossiers"),
        }
    }
}

pub fn connect(config: &StorageConfig) -> Result<Storage, object_store::Error> {
    let store = AmazonS3Builder::new()
        .with_endpoint(&config.endpoint)
        .with_bucket_name(&config.bucket)
        .with_access_key_id(&config.access_key)
        .with_secret_access_key(&config.secret_key)
        // RustFS: адресация path-style (virtual-hosted требует отдельно
        // настроенных доменов), http внутри стенда
        .with_virtual_hosted_style_request(false)
        .with_allow_http(true)
        .with_region("us-east-1")
        .build()?;
    Ok(Arc::new(store))
}

/// Ключ объекта заявки: файлы группируются по заявке (досье, арх. § 2).
pub fn application_file_key(application_id: uuid::Uuid, file_id: uuid::Uuid) -> String {
    format!("applications/{application_id}/{file_id}")
}

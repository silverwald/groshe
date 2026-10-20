//! Persistent device identity and author keyring for Groshe.
//!
//! Provides hardware-backed secure identity storage on Apple platforms (iOS/macOS
//! Keychain Services) and filesystem-based secure storage with restricted permissions
//! on desktop/daemon environments, mediated by the [`IdentityStorage`] trait.

use async_trait::async_trait;
use iroh::SecretKey;
use iroh_docs::AuthorId;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::SyncError;

pub const DEVICE_KEY_FILE: &str = "device.key";
pub const AUTHOR_ID_FILE: &str = "author.id";

pub const KEYCHAIN_DEFAULT_SERVICE: &str = "org.groshe.identity";
pub const KEYCHAIN_SECRET_KEY_ACCOUNT: &str = "node_secret_key";
pub const KEYCHAIN_AUTHOR_ID_ACCOUNT: &str = "default_author_id";

/// Abstract secure storage backend for cryptographic identity credentials.
#[async_trait]
pub trait IdentityStorage: Send + Sync {
    /// Loads the node secret key if one has been previously saved.
    async fn load_secret_key(&self) -> Result<Option<SecretKey>, SyncError>;

    /// Securely persists the node secret key.
    async fn save_secret_key(&self, key: &SecretKey) -> Result<(), SyncError>;

    /// Loads the default author ID if one has been previously saved.
    async fn load_author_id(&self) -> Result<Option<AuthorId>, SyncError>;

    /// Securely persists the default author ID.
    async fn save_author_id(&self, author_id: &AuthorId) -> Result<(), SyncError>;
}

/// Filesystem-backed identity storage with POSIX 0o600 restricted file permissions.
#[derive(Debug, Clone)]
pub struct FileIdentityStorage {
    dir: PathBuf,
}

impl FileIdentityStorage {
    pub fn new(dir: impl AsRef<Path>) -> Self {
        Self {
            dir: dir.as_ref().to_path_buf(),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

#[async_trait]
impl IdentityStorage for FileIdentityStorage {
    async fn load_secret_key(&self) -> Result<Option<SecretKey>, SyncError> {
        let key_path = self.dir.join(DEVICE_KEY_FILE);
        if !key_path.exists() {
            return Ok(None);
        }

        let bytes = tokio::fs::read(&key_path).await?;
        if bytes.len() != 32 {
            return Err(SyncError::Storage(format!(
                "Invalid secret key length {} in {:?}",
                bytes.len(),
                key_path
            )));
        }
        let mut key_arr = [0u8; 32];
        key_arr.copy_from_slice(&bytes);
        Ok(Some(SecretKey::from_bytes(&key_arr)))
    }

    async fn save_secret_key(&self, key: &SecretKey) -> Result<(), SyncError> {
        tokio::fs::create_dir_all(&self.dir).await?;
        let key_path = self.dir.join(DEVICE_KEY_FILE);
        tokio::fs::write(&key_path, key.to_bytes()).await?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            let _ = std::fs::set_permissions(&key_path, perms);
        }

        Ok(())
    }

    async fn load_author_id(&self) -> Result<Option<AuthorId>, SyncError> {
        let author_path = self.dir.join(AUTHOR_ID_FILE);
        if !author_path.exists() {
            return Ok(None);
        }

        let s = tokio::fs::read_to_string(&author_path).await?;
        Ok(AuthorId::from_str(s.trim()).ok())
    }

    async fn save_author_id(&self, author_id: &AuthorId) -> Result<(), SyncError> {
        tokio::fs::create_dir_all(&self.dir).await?;
        let author_path = self.dir.join(AUTHOR_ID_FILE);
        tokio::fs::write(&author_path, author_id.to_string()).await?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            let _ = std::fs::set_permissions(&author_path, perms);
        }

        Ok(())
    }
}

/// Keychain-backed identity storage for Apple platforms (iOS/macOS Keychain Services).
///
/// On non-Apple targets or test environments, an internal thread-safe in-memory
/// simulated vault is provided to guarantee 100% portable testability.
#[derive(Clone)]
pub struct KeychainIdentityStorage {
    service_name: String,
    #[cfg(not(target_vendor = "apple"))]
    simulated_vault: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl KeychainIdentityStorage {
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
            #[cfg(not(target_vendor = "apple"))]
            simulated_vault: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn default_service() -> Self {
        Self::new(KEYCHAIN_DEFAULT_SERVICE)
    }

    pub fn service_name(&self) -> &str {
        &self.service_name
    }
}

#[async_trait]
impl IdentityStorage for KeychainIdentityStorage {
    #[cfg(target_vendor = "apple")]
    async fn load_secret_key(&self) -> Result<Option<SecretKey>, SyncError> {
        use security_framework::passwords::get_generic_password;
        match get_generic_password(&self.service_name, KEYCHAIN_SECRET_KEY_ACCOUNT) {
            Ok(bytes) => {
                if bytes.len() != 32 {
                    return Err(SyncError::Storage(format!(
                        "Keychain secret key has invalid length {}",
                        bytes.len()
                    )));
                }
                let mut key_arr = [0u8; 32];
                key_arr.copy_from_slice(&bytes);
                Ok(Some(SecretKey::from_bytes(&key_arr)))
            }
            Err(err) if err.code() == -25300 => Ok(None), // errSecItemNotFound
            Err(err) => Err(SyncError::Storage(format!("Keychain error: {err}"))),
        }
    }

    #[cfg(not(target_vendor = "apple"))]
    async fn load_secret_key(&self) -> Result<Option<SecretKey>, SyncError> {
        let vault = self.simulated_vault.read().await;
        if let Some(bytes) = vault.get(KEYCHAIN_SECRET_KEY_ACCOUNT) {
            if bytes.len() != 32 {
                return Err(SyncError::Storage(format!(
                    "Simulated keychain secret key has invalid length {}",
                    bytes.len()
                )));
            }
            let mut key_arr = [0u8; 32];
            key_arr.copy_from_slice(bytes);
            Ok(Some(SecretKey::from_bytes(&key_arr)))
        } else {
            Ok(None)
        }
    }

    #[cfg(target_vendor = "apple")]
    async fn save_secret_key(&self, key: &SecretKey) -> Result<(), SyncError> {
        use security_framework::passwords::set_generic_password;
        set_generic_password(
            &self.service_name,
            KEYCHAIN_SECRET_KEY_ACCOUNT,
            &key.to_bytes(),
        )
        .map_err(|e| SyncError::Storage(format!("Failed to save secret key to Keychain: {e}")))
    }

    #[cfg(not(target_vendor = "apple"))]
    async fn save_secret_key(&self, key: &SecretKey) -> Result<(), SyncError> {
        let mut vault = self.simulated_vault.write().await;
        vault.insert(
            KEYCHAIN_SECRET_KEY_ACCOUNT.to_string(),
            key.to_bytes().to_vec(),
        );
        Ok(())
    }

    #[cfg(target_vendor = "apple")]
    async fn load_author_id(&self) -> Result<Option<AuthorId>, SyncError> {
        use security_framework::passwords::get_generic_password;
        match get_generic_password(&self.service_name, KEYCHAIN_AUTHOR_ID_ACCOUNT) {
            Ok(bytes) => {
                let s = String::from_utf8(bytes).map_err(|e| {
                    SyncError::Storage(format!("Invalid UTF-8 in Keychain author ID: {e}"))
                })?;
                Ok(AuthorId::from_str(s.trim()).ok())
            }
            Err(err) if err.code() == -25300 => Ok(None),
            Err(err) => Err(SyncError::Storage(format!("Keychain error: {err}"))),
        }
    }

    #[cfg(not(target_vendor = "apple"))]
    async fn load_author_id(&self) -> Result<Option<AuthorId>, SyncError> {
        let vault = self.simulated_vault.read().await;
        if let Some(bytes) = vault.get(KEYCHAIN_AUTHOR_ID_ACCOUNT) {
            let s = String::from_utf8(bytes.clone()).map_err(|e| {
                SyncError::Storage(format!("Invalid UTF-8 in simulated author ID: {e}"))
            })?;
            Ok(AuthorId::from_str(s.trim()).ok())
        } else {
            Ok(None)
        }
    }

    #[cfg(target_vendor = "apple")]
    async fn save_author_id(&self, author_id: &AuthorId) -> Result<(), SyncError> {
        use security_framework::passwords::set_generic_password;
        set_generic_password(
            &self.service_name,
            KEYCHAIN_AUTHOR_ID_ACCOUNT,
            author_id.to_string().as_bytes(),
        )
        .map_err(|e| SyncError::Storage(format!("Failed to save author ID to Keychain: {e}")))
    }

    #[cfg(not(target_vendor = "apple"))]
    async fn save_author_id(&self, author_id: &AuthorId) -> Result<(), SyncError> {
        let mut vault = self.simulated_vault.write().await;
        vault.insert(
            KEYCHAIN_AUTHOR_ID_ACCOUNT.to_string(),
            author_id.to_string().into_bytes(),
        );
        Ok(())
    }
}

/// Device identity holding the node secret key and docs author ID.
#[derive(Clone)]
pub struct DeviceIdentity {
    pub secret_key: SecretKey,
    pub author_id: Option<AuthorId>,
    pub storage: Arc<dyn IdentityStorage>,
}

impl std::fmt::Debug for DeviceIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceIdentity")
            .field("secret_key", &"<redacted>")
            .field("author_id", &self.author_id)
            .finish()
    }
}

impl DeviceIdentity {
    /// Loads an existing device identity from `data_dir` or generates a new one.
    pub async fn load_or_create(data_dir: impl AsRef<Path>) -> Result<Self, SyncError> {
        let storage = Arc::new(FileIdentityStorage::new(data_dir));
        Self::load_or_create_with_storage(storage).await
    }

    /// Loads or creates an identity backed by a custom [`IdentityStorage`] implementation.
    pub async fn load_or_create_with_storage(
        storage: Arc<dyn IdentityStorage>,
    ) -> Result<Self, SyncError> {
        let secret_key = match storage.load_secret_key().await? {
            Some(key) => key,
            None => {
                let key = SecretKey::generate();
                storage.save_secret_key(&key).await?;
                key
            }
        };

        let author_id = storage.load_author_id().await?;

        Ok(Self {
            secret_key,
            author_id,
            storage,
        })
    }

    /// Persists the default author ID associated with this identity.
    pub async fn save_author_id(&mut self, author_id: AuthorId) -> Result<(), SyncError> {
        self.storage.save_author_id(&author_id).await?;
        self.author_id = Some(author_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_identity_persistence() {
        let tmp = tempfile::tempdir().unwrap();
        let mut id1 = DeviceIdentity::load_or_create(tmp.path()).await.unwrap();

        let dummy_secret = SecretKey::generate();
        let dummy_author = AuthorId::from(*dummy_secret.public().as_bytes());
        id1.save_author_id(dummy_author).await.unwrap();

        // Reload from same dir
        let id2 = DeviceIdentity::load_or_create(tmp.path()).await.unwrap();
        assert_eq!(id1.secret_key.to_bytes(), id2.secret_key.to_bytes());
        assert_eq!(id2.author_id, Some(dummy_author));
    }

    #[tokio::test]
    async fn test_keychain_identity_storage_roundtrip() {
        let storage = Arc::new(KeychainIdentityStorage::default_service());
        let mut id1 = DeviceIdentity::load_or_create_with_storage(storage.clone())
            .await
            .unwrap();

        let dummy_secret = SecretKey::generate();
        let dummy_author = AuthorId::from(*dummy_secret.public().as_bytes());
        id1.save_author_id(dummy_author).await.unwrap();

        // Reload from same keychain storage instance
        let id2 = DeviceIdentity::load_or_create_with_storage(storage)
            .await
            .unwrap();
        assert_eq!(id1.secret_key.to_bytes(), id2.secret_key.to_bytes());
        assert_eq!(id2.author_id, Some(dummy_author));
    }

    #[tokio::test]
    async fn test_corrupted_key_length_rejection() {
        let tmp = tempfile::tempdir().unwrap();
        let key_path = tmp.path().join(DEVICE_KEY_FILE);
        // Write invalid 16-byte key
        tokio::fs::write(&key_path, &[0u8; 16]).await.unwrap();

        let res = DeviceIdentity::load_or_create(tmp.path()).await;
        assert!(res.is_err());
    }
}

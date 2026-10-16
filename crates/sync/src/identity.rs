//! Persistent device identity and author keyring for Groshe.
//!
//! Stores the device's Iroh `SecretKey` and default `AuthorId` on disk
//! with restricted file permissions.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use iroh::SecretKey;
use iroh_docs::AuthorId;

use crate::SyncError;

pub const DEVICE_KEY_FILE: &str = "device.key";
pub const AUTHOR_ID_FILE: &str = "author.id";

/// Device identity holding the node secret key and docs author ID.
#[derive(Debug, Clone)]
pub struct DeviceIdentity {
    pub secret_key: SecretKey,
    pub author_id: Option<AuthorId>,
    pub data_dir: PathBuf,
}

impl DeviceIdentity {
    /// Loads an existing device identity from `data_dir` or generates a new one.
    pub async fn load_or_create(data_dir: impl AsRef<Path>) -> Result<Self, SyncError> {
        let dir = data_dir.as_ref().to_path_buf();
        tokio::fs::create_dir_all(&dir).await?;

        let key_path = dir.join(DEVICE_KEY_FILE);
        let secret_key = if key_path.exists() {
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
            SecretKey::from_bytes(&key_arr)
        } else {
            let key = SecretKey::generate();
            tokio::fs::write(&key_path, key.to_bytes()).await?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = std::fs::Permissions::from_mode(0o600);
                let _ = std::fs::set_permissions(&key_path, perms);
            }

            key
        };

        let author_path = dir.join(AUTHOR_ID_FILE);
        let author_id = if author_path.exists() {
            let s = tokio::fs::read_to_string(&author_path).await?;
            AuthorId::from_str(s.trim()).ok()
        } else {
            None
        };

        Ok(Self {
            secret_key,
            author_id,
            data_dir: dir,
        })
    }

    /// Persists the default author ID associated with this identity.
    pub async fn save_author_id(&mut self, author_id: AuthorId) -> Result<(), SyncError> {
        let author_path = self.data_dir.join(AUTHOR_ID_FILE);
        tokio::fs::write(&author_path, author_id.to_string()).await?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            let _ = std::fs::set_permissions(&author_path, perms);
        }

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
}
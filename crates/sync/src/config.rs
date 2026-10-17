//! Node configuration management for Groshe (`groshe.toml`).
//!
//! Controls sovereign relay endpoints, WAN rendezvous settings, and local network defaults.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Root configuration schema for `groshe.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrosheConfig {
    #[serde(default)]
    pub network: NetworkConfig,
}

/// Network and P2P connection options.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Custom Iroh relay server URL.
    ///
    /// If `None`, empty, or set to `"default"`, the node uses public Number 0 production relays.
    /// To disable relays entirely (local LAN only), set to `"disabled"`.
    #[serde(default)]
    pub relay_url: Option<String>,
}

impl GrosheConfig {
    /// Loads configuration from `groshe.toml` in `dir`, creating a default template if not found.
    pub async fn load_or_create(dir: impl AsRef<Path>) -> Result<Self, std::io::Error> {
        let path = dir.as_ref().join("groshe.toml");
        if path.exists() {
            let content = tokio::fs::read_to_string(&path).await?;
            toml::from_str(&content).map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Failed to parse {}: {}", path.display(), e),
                )
            })
        } else {
            let default_cfg = Self::default();
            let template = r#"# Groshe P2P Node Configuration
# Documentation: docs/RELAY_SETUP.md

[network]
# Custom Iroh relay server URL for sovereign deployments.
# Defaults to public n0 production relays if commented out or omitted.
# To disable relays entirely for local-only LAN sync, specify: relay_url = "disabled"
# relay_url = "https://relay.example.com"
"#;
            let _ = tokio::fs::write(&path, template).await;
            Ok(default_cfg)
        }
    }

    /// Returns the configured custom relay URL, if present and non-empty.
    pub fn custom_relay_url(&self) -> Option<&str> {
        self.network
            .relay_url
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| {
                !s.is_empty()
                    && !s.eq_ignore_ascii_case("default")
                    && !s.eq_ignore_ascii_case("disabled")
            })
    }

    /// Returns true if relays are explicitly disabled.
    pub fn is_relay_disabled(&self) -> bool {
        self.network
            .relay_url
            .as_deref()
            .map(|s| s.trim().eq_ignore_ascii_case("disabled"))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_config_load_and_create_default() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = GrosheConfig::load_or_create(tmp.path()).await.unwrap();
        assert_eq!(cfg.custom_relay_url(), None);
        assert!(!cfg.is_relay_disabled());

        let file_path = tmp.path().join("groshe.toml");
        assert!(file_path.exists());
    }

    #[tokio::test]
    async fn test_custom_relay_url_parsing() {
        let tmp = tempfile::tempdir().unwrap();
        let toml_content = r#"
[network]
relay_url = "https://relay.groshe.internal"
"#;
        tokio::fs::write(tmp.path().join("groshe.toml"), toml_content).await.unwrap();

        let cfg = GrosheConfig::load_or_create(tmp.path()).await.unwrap();
        assert_eq!(cfg.custom_relay_url(), Some("https://relay.groshe.internal"));
        assert!(!cfg.is_relay_disabled());
    }

    #[tokio::test]
    async fn test_disabled_relay_mode() {
        let tmp = tempfile::tempdir().unwrap();
        let toml_content = r#"
[network]
relay_url = "disabled"
"#;
        tokio::fs::write(tmp.path().join("groshe.toml"), toml_content).await.unwrap();

        let cfg = GrosheConfig::load_or_create(tmp.path()).await.unwrap();
        assert_eq!(cfg.custom_relay_url(), None);
        assert!(cfg.is_relay_disabled());
    }
}

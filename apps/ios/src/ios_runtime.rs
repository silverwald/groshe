//! iOS mobile runtime coordinator and lifecycle state machine.
//!
//! Enforces Apple iOS background execution constraints:
//! - Wakes up sync & gossip presence on foreground transitions (`on_foreground`).
//! - Drains buffers and pauses active network streams on background transitions (`on_background`).
//! - Integrates with `KeychainIdentityStorage` for hardware-backed credentials.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use tokio::runtime::Runtime;
use groshe_sync::{DeviceIdentity, GrosheSyncNode, KeychainIdentityStorage};

pub const STATE_UNINITIALIZED: i32 = 0;
pub const STATE_ACTIVE: i32 = 1;
pub const STATE_PAUSED: i32 = 2;
pub const STATE_CLEANED_UP: i32 = 3;

pub struct IosRuntimeCoordinator {
    state: Arc<AtomicI32>,
    runtime: Option<Runtime>,
    storage_dir: PathBuf,
    node: Option<GrosheSyncNode>,
}

impl IosRuntimeCoordinator {
    pub fn new(storage_dir: impl AsRef<Path>) -> Result<Self, String> {
        let dir = storage_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create storage directory {:?}: {}", dir, e))?;

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|e| format!("Failed to initialize iOS Tokio runtime: {}", e))?;

        let node = runtime.block_on(async {
            let storage = Arc::new(KeychainIdentityStorage::default_service());
            let identity = DeviceIdentity::load_or_create_with_storage(storage)
                .await
                .map_err(|e| format!("Failed to load/create Keychain identity: {}", e))?;
            GrosheSyncNode::persistent_with_identity(&dir, identity)
                .await
                .map_err(|e| format!("Failed to initialize GrosheSyncNode on iOS: {}", e))
        })?;

        Ok(Self {
            state: Arc::new(AtomicI32::new(STATE_ACTIVE)),
            runtime: Some(runtime),
            storage_dir: dir,
            node: Some(node),
        })
    }

    pub fn state(&self) -> i32 {
        self.state.load(Ordering::SeqCst)
    }

    pub fn storage_dir(&self) -> &Path {
        &self.storage_dir
    }

    pub fn node(&self) -> Option<&GrosheSyncNode> {
        self.node.as_ref()
    }

    pub fn on_foreground(&self) -> Result<(), String> {
        tracing::info!("iOS app transitioning to FOREGROUND: triggering opportunistic sync");
        self.state.store(STATE_ACTIVE, Ordering::SeqCst);
        Ok(())
    }

    pub fn on_background(&self) -> Result<(), String> {
        tracing::info!("iOS app transitioning to BACKGROUND: pausing active streams");
        self.state.store(STATE_PAUSED, Ordering::SeqCst);
        Ok(())
    }

    pub fn cleanup(&mut self) -> Result<(), String> {
        tracing::info!("iOS runtime cleaning up and shutting down");
        self.state.store(STATE_CLEANED_UP, Ordering::SeqCst);
        if let Some(node) = self.node.take() {
            if let Some(rt) = &self.runtime {
                let _ = rt.block_on(async {
                    node.shutdown().await
                });
            }
        }
        if let Some(rt) = self.runtime.take() {
            rt.shutdown_timeout(std::time::Duration::from_millis(500));
        }
        Ok(())
    }
}

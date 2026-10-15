//! Android process runtime and mobile environment management for Groshe.
//!
//! Handles Tokio multi-thread runtime initialization, internal storage resolution,
//! and Android WifiManager MulticastLock acquisition for local mDNS peer discovery.

use std::path::PathBuf;

/// Result of mobile runtime setup.
pub struct MobileEnvironment {
    pub storage_dir: PathBuf,
    pub runtime: tokio::runtime::Runtime,
}

/// Initialize the Tokio runtime optimized for mobile lifecycle constraints.
pub fn init_tokio_runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("groshe-sync-worker")
        .enable_all()
        .build()
}

#[cfg(target_os = "android")]
/// Acquire Android WifiManager MulticastLock via JNI to allow mDNS discovery packets.
pub fn acquire_multicast_lock(
    app: &android_activity::AndroidApp,
) -> Result<(), Box<dyn std::error::Error>> {
    use jni::objects::{JObject, JValue};
    use jni::JavaVM;

    let vm_ptr = app.vm_as_ptr() as *mut jni::sys::JavaVM;
    if vm_ptr.is_null() {
        return Err("JavaVM pointer is null".into());
    }

    let vm = unsafe { JavaVM::from_raw(vm_ptr) }?;
    let mut env = vm.attach_current_thread()?;

    let activity_ptr = app.activity_as_ptr() as jni::sys::jobject;
    if activity_ptr.is_null() {
        return Err("Activity pointer is null".into());
    }
    let activity = unsafe { JObject::from_raw(activity_ptr) };

    // Fetch Context.WIFI_SERVICE
    let wifi_service_str = env.new_string("wifi")?;
    let wifi_manager = env
        .call_method(
            &activity,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[JValue::Object(&wifi_service_str)],
        )?
        .l()?;

    if wifi_manager.is_null() {
        return Err("WifiManager service returned null".into());
    }

    // Call wifiManager.createMulticastLock("groshe_mdns")
    let lock_tag = env.new_string("groshe_mdns")?;
    let lock = env
        .call_method(
            &wifi_manager,
            "createMulticastLock",
            "(Ljava/lang/String;)Landroid/net/wifi/WifiManager$MulticastLock;",
            &[JValue::Object(&lock_tag)],
        )?
        .l()?;

    if lock.is_null() {
        return Err("Failed to create MulticastLock".into());
    }

    // lock.setReferenceCounted(true)
    env.call_method(&lock, "setReferenceCounted", "(Z)V", &[JValue::Bool(1)])?;

    // lock.acquire()
    env.call_method(&lock, "acquire", "()V", &[])?;

    tracing::info!("Successfully acquired Android WifiManager MulticastLock for mDNS discovery");
    Ok(())
}

#[cfg(target_os = "android")]
/// Resolve Android internal files directory (`context.getFilesDir()`).
pub fn resolve_android_storage_dir(app: &android_activity::AndroidApp) -> PathBuf {
    app.internal_data_path()
        .unwrap_or_else(|| PathBuf::from("/data/data/com.groshe.app/files"))
        .join("groshe_sync")
}

#[cfg(not(target_os = "android"))]
/// Fallback storage path for desktop preview / testing.
pub fn resolve_desktop_storage_dir() -> PathBuf {
    let base = std::env::var("GROSHE_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("groshe_mobile_preview"));
    base.join("groshe_sync")
}

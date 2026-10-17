//! Android process runtime and mobile environment management for Groshe.
//!
//! Handles Tokio multi-thread runtime initialization, internal storage resolution,
//! Android WifiManager MulticastLock acquisition for local mDNS peer discovery,
//! and haptic feedback via Android Vibrator JNI.

use std::path::PathBuf;
#[cfg(target_os = "android")]
use std::sync::OnceLock;

#[cfg(target_os = "android")]
static JAVA_VM: OnceLock<jni::JavaVM> = OnceLock::new();
#[cfg(target_os = "android")]
static ACTIVITY_REF: OnceLock<jni::objects::GlobalRef> = OnceLock::new();

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
/// Initialize Android environment, caching JavaVM & Activity reference and acquiring MulticastLock.
pub fn init_android_environment(
    app: &android_activity::AndroidApp,
) -> Result<(), Box<dyn std::error::Error>> {
    use jni::objects::{JObject, JValue};
    use jni::JavaVM;

    let vm_ptr = app.vm_as_ptr() as *mut jni::sys::JavaVM;
    if vm_ptr.is_null() {
        return Err("JavaVM pointer is null".into());
    }

    let vm = unsafe { JavaVM::from_raw(vm_ptr) }?;
    let _ = JAVA_VM.set(vm);
    let vm = JAVA_VM.get().expect("JavaVM must be initialized");

    let activity_ptr = app.activity_as_ptr() as jni::sys::jobject;
    if activity_ptr.is_null() {
        return Err("Activity pointer is null".into());
    }

    let mut env = vm.attach_current_thread()?;
    let activity = unsafe { JObject::from_raw(activity_ptr) };
    let global_activity = env.new_global_ref(&activity)?;
    let _ = ACTIVITY_REF.set(global_activity);

    // Fetch Context.WIFI_SERVICE and acquire MulticastLock
    let wifi_service_str = env.new_string("wifi")?;
    let wifi_manager = env
        .call_method(
            &activity,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[JValue::Object(&wifi_service_str)],
        )?
        .l()?;

    if !wifi_manager.is_null() {
        let lock_tag = env.new_string("groshe_mdns")?;
        let lock = env
            .call_method(
                &wifi_manager,
                "createMulticastLock",
                "(Ljava/lang/String;)Landroid/net/wifi/WifiManager$MulticastLock;",
                &[JValue::Object(&lock_tag)],
            )?
            .l()?;

        if !lock.is_null() {
            env.call_method(&lock, "setReferenceCounted", "(Z)V", &[JValue::Bool(1)])?;
            env.call_method(&lock, "acquire", "()V", &[])?;
            tracing::info!(
                "Successfully acquired Android WifiManager MulticastLock for mDNS discovery"
            );
        }
    }

    Ok(())
}

#[cfg(target_os = "android")]
/// Trigger tactile haptic feedback using Android Vibrator service via JNI.
pub fn trigger_haptic_feedback() {
    let Some(vm) = JAVA_VM.get() else { return };
    let Some(activity_ref) = ACTIVITY_REF.get() else {
        return;
    };
    let Ok(mut env) = vm.attach_current_thread() else {
        return;
    };

    let vibrator_service_str = match env.new_string("vibrator") {
        Ok(s) => s,
        Err(_) => return,
    };

    let vibrator = match env.call_method(
        activity_ref.as_obj(),
        "getSystemService",
        "(Ljava/lang/String;)Ljava/lang/Object;",
        &[jni::objects::JValue::Object(&vibrator_service_str)],
    ) {
        Ok(v) => match v.l() {
            Ok(obj) if !obj.is_null() => obj,
            _ => return,
        },
        Err(_) => return,
    };

    // Vibrate 40ms for a crisp tactile check-off response
    let _ = env.call_method(
        &vibrator,
        "vibrate",
        "(J)V",
        &[jni::objects::JValue::Long(40)],
    );
}

#[cfg(not(target_os = "android"))]
/// Trigger tactile haptic feedback (simulated / no-op on desktop preview).
pub fn trigger_haptic_feedback() {
    tracing::debug!("Haptic feedback simulated (desktop preview)");
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

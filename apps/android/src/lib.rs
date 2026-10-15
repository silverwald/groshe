//! Android mobile entry point and JNI bindings for Groshe.

pub mod android_runtime;

slint::include_modules!();

#[cfg(target_os = "android")]
#[no_mangle]
#[allow(improper_ctypes_definitions)]
pub extern "C" fn android_main(app: slint::android::AndroidApp) {
    tracing::info!("Initializing Groshe on Android...");

    // Acquire MulticastLock via JNI for mDNS peer discovery
    if let Err(e) = android_runtime::acquire_multicast_lock(&app) {
        tracing::warn!("Could not acquire MulticastLock via JNI: {}", e);
    }

    // Resolve internal files directory for persistent Iroh storage
    let storage_dir = android_runtime::resolve_android_storage_dir(&app);
    tracing::info!("Android internal storage directory: {:?}", storage_dir);

    // Initialize Tokio multi-thread runtime for mobile async tasks
    let _runtime =
        android_runtime::init_tokio_runtime().expect("Failed to initialize mobile Tokio runtime");

    slint::android::init(app).expect("Failed to initialize Slint Android backend");
    let main_window = AppWindow::new().expect("Failed to create main window");
    main_window
        .run()
        .expect("Slint event loop terminated with error");
}

#[cfg(not(target_os = "android"))]
pub fn run_desktop() -> Result<(), Box<dyn std::error::Error>> {
    let _runtime = android_runtime::init_tokio_runtime()?;
    let main_window = AppWindow::new()?;
    main_window.run()?;
    Ok(())
}

//! Android mobile entry point and JNI bindings for Groshe.

slint::include_modules!();

#[cfg(target_os = "android")]
#[no_mangle]
#[allow(improper_ctypes_definitions)]
pub extern "C" fn android_main(app: slint::android::AndroidApp) {
    tracing::info!("Initializing Groshe on Android...");
    slint::android::init(app).expect("Failed to initialize Slint Android backend");
    let main_window = AppWindow::new().expect("Failed to create main window");
    main_window
        .run()
        .expect("Slint event loop terminated with error");
}

#[cfg(not(target_os = "android"))]
pub fn run_desktop() -> Result<(), Box<dyn std::error::Error>> {
    let main_window = AppWindow::new()?;
    main_window.run()?;
    Ok(())
}

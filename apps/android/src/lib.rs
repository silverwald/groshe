//! Android entry point for Groshe mobile app.
//!
//! Exposes JNI / native activity hooks and binds the Slint UI to the
//! underlying sync and store runtimes.

#[no_mangle]
pub extern "C" fn android_main() {
    tracing::info!("Initializing Groshe Android application...");
}

//! iOS mobile entry point and native C-FFI runtime for Groshe.

pub mod ios_runtime;

use std::ffi::{c_char, c_int, CStr};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Mutex;
use ios_runtime::IosRuntimeCoordinator;

slint::include_modules!();

static RUNTIME: Mutex<Option<IosRuntimeCoordinator>> = Mutex::new(None);
static GLOBAL_STATE: AtomicI32 = AtomicI32::new(ios_runtime::STATE_UNINITIALIZED);

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn groshe_ios_init(storage_path: *const c_char) -> c_int {
    if storage_path.is_null() {
        return -1;
    }

    let path_str = match unsafe { CStr::from_ptr(storage_path) }.to_str() {
        Ok(s) => s,
        Err(_) => return -2,
    };

    let mut lock = match RUNTIME.lock() {
        Ok(l) => l,
        Err(_) => return -3,
    };

    match IosRuntimeCoordinator::new(path_str) {
        Ok(coordinator) => {
            GLOBAL_STATE.store(ios_runtime::STATE_ACTIVE, Ordering::SeqCst);
            *lock = Some(coordinator);
            0
        }
        Err(err) => {
            tracing::error!("Failed to initialize iOS runtime: {}", err);
            -4
        }
    }
}

#[no_mangle]
pub extern "C" fn groshe_ios_run() -> c_int {
    match AppWindow::new() {
        Ok(window) => {
            if let Err(e) = window.run() {
                tracing::error!("Slint event loop error: {}", e);
                -1
            } else {
                0
            }
        }
        Err(e) => {
            tracing::error!("Failed to instantiate Slint AppWindow: {}", e);
            -2
        }
    }
}

#[no_mangle]
pub extern "C" fn groshe_ios_on_foreground() -> c_int {
    let lock = match RUNTIME.lock() {
        Ok(l) => l,
        Err(_) => return -1,
    };

    if let Some(ref coordinator) = *lock {
        if coordinator.on_foreground().is_ok() {
            GLOBAL_STATE.store(ios_runtime::STATE_ACTIVE, Ordering::SeqCst);
            0
        } else {
            -2
        }
    } else {
        -3
    }
}

#[no_mangle]
pub extern "C" fn groshe_ios_on_background() -> c_int {
    let lock = match RUNTIME.lock() {
        Ok(l) => l,
        Err(_) => return -1,
    };

    if let Some(ref coordinator) = *lock {
        if coordinator.on_background().is_ok() {
            GLOBAL_STATE.store(ios_runtime::STATE_PAUSED, Ordering::SeqCst);
            0
        } else {
            -2
        }
    } else {
        -3
    }
}

#[no_mangle]
pub extern "C" fn groshe_ios_cleanup() -> c_int {
    let mut lock = match RUNTIME.lock() {
        Ok(l) => l,
        Err(_) => return -1,
    };

    GLOBAL_STATE.store(ios_runtime::STATE_CLEANED_UP, Ordering::SeqCst);
    if let Some(mut coordinator) = lock.take() {
        if coordinator.cleanup().is_ok() {
            0
        } else {
            -2
        }
    } else {
        0
    }
}

#[no_mangle]
pub extern "C" fn groshe_ios_get_state() -> c_int {
    GLOBAL_STATE.load(Ordering::SeqCst)
}

use std::ffi::CString;
use groshe_ios::ios_runtime::{
    IosRuntimeCoordinator, STATE_ACTIVE, STATE_CLEANED_UP, STATE_PAUSED,
};
use groshe_ios::{
    groshe_ios_cleanup, groshe_ios_get_state, groshe_ios_init, groshe_ios_on_background,
    groshe_ios_on_foreground,
};

#[test]
fn test_ios_coordinator_lifecycle() {
    let tmp = tempfile::tempdir().unwrap();
    let mut coordinator = IosRuntimeCoordinator::new(tmp.path()).unwrap();

    assert_eq!(coordinator.state(), STATE_ACTIVE);
    assert!(coordinator.node().is_some());

    coordinator.on_background().unwrap();
    assert_eq!(coordinator.state(), STATE_PAUSED);

    coordinator.on_foreground().unwrap();
    assert_eq!(coordinator.state(), STATE_ACTIVE);

    coordinator.cleanup().unwrap();
    assert_eq!(coordinator.state(), STATE_CLEANED_UP);
    assert!(coordinator.node().is_none());
}

#[test]
fn test_ios_c_ffi_lifecycle_and_state_transitions() {
    let tmp = tempfile::tempdir().unwrap();
    let path_str = tmp.path().to_str().unwrap();
    let c_path = CString::new(path_str).unwrap();

    // Ensure clean initial state
    let _ = groshe_ios_cleanup();
    assert_eq!(groshe_ios_get_state(), STATE_CLEANED_UP);

    // 1. Initialize runtime via C-FFI
    let init_res = groshe_ios_init(c_path.as_ptr());
    assert_eq!(init_res, 0);
    assert_eq!(groshe_ios_get_state(), STATE_ACTIVE);

    // 2. Transition to background
    let bg_res = groshe_ios_on_background();
    assert_eq!(bg_res, 0);
    assert_eq!(groshe_ios_get_state(), STATE_PAUSED);

    // 3. Resume to foreground
    let fg_res = groshe_ios_on_foreground();
    assert_eq!(fg_res, 0);
    assert_eq!(groshe_ios_get_state(), STATE_ACTIVE);

    // 4. Cleanup
    let cleanup_res = groshe_ios_cleanup();
    assert_eq!(cleanup_res, 0);
    assert_eq!(groshe_ios_get_state(), STATE_CLEANED_UP);
}

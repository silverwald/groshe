//! Android mobile entry point and JNI bindings for Groshe.

pub mod android_runtime;
pub mod sync_bridge;

slint::include_modules!();

/// Wire up UI handlers and sync dispatch for AppWindow.
pub fn setup_app_window(app: &AppWindow, sync_handle: sync_bridge::SyncBridgeHandle) {
    let sh = sync_handle.clone();
    app.on_select_list(move |id, _title| {
        sh.send(sync_bridge::SyncCommand::SelectList { id: id.to_string() });
    });

    let sh = sync_handle.clone();
    app.on_create_list(move |title| {
        sh.send(sync_bridge::SyncCommand::CreateList {
            title: title.to_string(),
        });
    });

    let sh = sync_handle.clone();
    app.on_delete_list(move |id| {
        sh.send(sync_bridge::SyncCommand::DeleteList { id: id.to_string() });
    });

    let sh = sync_handle.clone();
    app.on_rotate_list(move |id| {
        sh.send(sync_bridge::SyncCommand::RotateList { id: id.to_string() });
    });

    let sh = sync_handle.clone();
    app.on_share_list(move |id| {
        sh.send(sync_bridge::SyncCommand::SelectList { id: id.to_string() });
    });

    let sh = sync_handle.clone();
    app.on_toggle_item_check(move |id| {
        android_runtime::trigger_haptic_feedback();
        sh.send(sync_bridge::SyncCommand::ToggleItemCheck { id: id.to_string() });
    });

    let sh = sync_handle.clone();
    app.on_delete_item(move |id| {
        sh.send(sync_bridge::SyncCommand::DeleteItem { id: id.to_string() });
    });

    let sh = sync_handle.clone();
    app.on_add_item(move |raw_input| {
        sh.send(sync_bridge::SyncCommand::AddItem {
            raw_input: raw_input.to_string(),
        });
    });

    let sh = sync_handle.clone();
    app.on_trigger_undo(move || {
        sh.send(sync_bridge::SyncCommand::UndoDelete);
    });

    let sh = sync_handle.clone();
    app.on_join_ticket(move |ticket| {
        sh.send(sync_bridge::SyncCommand::JoinTicket {
            ticket_str: ticket.to_string(),
        });
    });

    let sh = sync_handle.clone();
    app.on_prune_tombstones(move || {
        sh.send(sync_bridge::SyncCommand::PruneTombstones);
    });

    let sh = sync_handle.clone();
    app.on_export_active_list(move || {
        sh.send(sync_bridge::SyncCommand::ExportActiveListMarkdown);
    });

    let sh = sync_handle.clone();
    app.on_export_json_backup(move || {
        sh.send(sync_bridge::SyncCommand::ExportActiveListJson);
    });

    let app_weak = app.as_weak();
    app.on_copy_ticket(move || {
        if let Some(app) = app_weak.upgrade() {
            let t = app.get_ticket_str().to_string();
            tracing::info!("Share ticket copied to clipboard: {}", t);
        }
    });
}

#[cfg(target_os = "android")]
#[no_mangle]
#[allow(improper_ctypes_definitions)]
pub extern "C" fn android_main(app: slint::android::AndroidApp) {
    tracing::info!("Initializing Groshe on Android...");

    // Acquire MulticastLock via JNI for mDNS peer discovery and register JVM/Activity
    if let Err(e) = android_runtime::init_android_environment(&app) {
        tracing::warn!("Could not initialize Android environment: {}", e);
    }

    // Resolve internal files directory for persistent Iroh storage
    let storage_dir = android_runtime::resolve_android_storage_dir(&app);
    tracing::info!("Android internal storage directory: {:?}", storage_dir);

    // Initialize Tokio multi-thread runtime for mobile async tasks
    let runtime =
        android_runtime::init_tokio_runtime().expect("Failed to initialize mobile Tokio runtime");

    slint::android::init(app).expect("Failed to initialize Slint Android backend");
    let main_window = AppWindow::new().expect("Failed to create main window");
    let sync_handle = sync_bridge::spawn_sync_bridge(&runtime, storage_dir, main_window.as_weak());
    setup_app_window(&main_window, sync_handle);
    main_window
        .run()
        .expect("Slint event loop terminated with error");
}

#[cfg(not(target_os = "android"))]
pub fn run_desktop() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = android_runtime::init_tokio_runtime()?;
    let storage_dir = android_runtime::resolve_desktop_storage_dir();
    let main_window = AppWindow::new()?;
    let sync_handle = sync_bridge::spawn_sync_bridge(&runtime, storage_dir, main_window.as_weak());
    setup_app_window(&main_window, sync_handle);
    main_window.run()?;
    Ok(())
}

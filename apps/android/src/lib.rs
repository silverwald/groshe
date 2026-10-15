//! Android mobile entry point and JNI bindings for Groshe.

pub mod android_runtime;
pub mod sync_bridge;

slint::include_modules!();

use slint::{Model, VecModel};
use std::rc::Rc;

/// Wire up UI handlers and sync dispatch for AppWindow.
pub fn setup_app_window(
    app: &AppWindow,
    sync_handle: sync_bridge::SyncBridgeHandle,
    items_model: Rc<VecModel<GroceryItemData>>,
) {
    app.set_items(slint::ModelRc::from(items_model.clone()));

    let last_deleted: Rc<std::cell::RefCell<Option<(usize, GroceryItemData)>>> =
        Rc::new(std::cell::RefCell::new(None));

    let app_weak = app.as_weak();
    app.on_toggle_in_store_mode(move || {
        if let Some(app) = app_weak.upgrade() {
            let current = app.get_in_store_mode();
            app.set_in_store_mode(!current);
            tracing::info!("In-store mode toggled: {}", !current);
        }
    });

    let items_model_clone = items_model.clone();
    let sync_handle_clone = sync_handle.clone();
    app.on_toggle_item_check(move |id| {
        for i in 0..items_model_clone.row_count() {
            if let Some(mut item) = items_model_clone.row_data(i) {
                if item.id == id {
                    item.checked = !item.checked;
                    let checked = item.checked;
                    items_model_clone.set_row_data(i, item);
                    sync_handle_clone.send(sync_bridge::SyncCommand::ToggleItemCheck {
                        id: id.to_string(),
                        checked,
                    });
                    break;
                }
            }
        }
    });

    let items_model_clone = items_model.clone();
    let sync_handle_clone = sync_handle.clone();
    app.on_add_item(move |name, qty| {
        let id = uuid::Uuid::now_v7().to_string();
        let new_item = GroceryItemData {
            id: id.clone().into(),
            name: name.clone(),
            quantity: format!("{}", qty).into(),
            category: "General".into(),
            checked: false,
        };
        items_model_clone.push(new_item);
        sync_handle_clone.send(sync_bridge::SyncCommand::AddItem {
            id,
            name: name.to_string(),
            quantity: format!("{}", qty),
            category: "General".to_string(),
        });
    });

    let items_model_clone = items_model.clone();
    let last_deleted_clone = last_deleted.clone();
    let sync_handle_clone = sync_handle.clone();
    let app_weak = app.as_weak();
    app.on_delete_item(move |id| {
        for i in 0..items_model_clone.row_count() {
            if let Some(item) = items_model_clone.row_data(i) {
                if item.id == id {
                    let removed = items_model_clone.remove(i);
                    *last_deleted_clone.borrow_mut() = Some((i, removed));
                    if let Some(app) = app_weak.upgrade() {
                        app.set_undo_toast_message("Item deleted".into());
                        app.set_show_undo_toast(true);
                    }
                    sync_handle_clone
                        .send(sync_bridge::SyncCommand::DeleteItem { id: id.to_string() });
                    break;
                }
            }
        }
    });

    let items_model_clone = items_model.clone();
    let last_deleted_clone = last_deleted.clone();
    let sync_handle_clone = sync_handle.clone();
    let app_weak = app.as_weak();
    app.on_trigger_undo(move || {
        if let Some((idx, item)) = last_deleted_clone.borrow_mut().take() {
            let id = item.id.to_string();
            let name = item.name.to_string();
            let quantity = item.quantity.to_string();
            let category = item.category.to_string();
            if idx <= items_model_clone.row_count() {
                items_model_clone.insert(idx, item);
            } else {
                items_model_clone.push(item);
            }
            if let Some(app) = app_weak.upgrade() {
                app.set_show_undo_toast(false);
            }
            sync_handle_clone.send(sync_bridge::SyncCommand::AddItem {
                id,
                name,
                quantity,
                category,
            });
        }
    });
}

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
    let runtime =
        android_runtime::init_tokio_runtime().expect("Failed to initialize mobile Tokio runtime");

    slint::android::init(app).expect("Failed to initialize Slint Android backend");
    let main_window = AppWindow::new().expect("Failed to create main window");
    let items_model = Rc::new(VecModel::default());
    let sync_handle = sync_bridge::spawn_sync_bridge(&runtime, storage_dir, main_window.as_weak());
    setup_app_window(&main_window, sync_handle, items_model);
    main_window
        .run()
        .expect("Slint event loop terminated with error");
}

#[cfg(not(target_os = "android"))]
pub fn run_desktop() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = android_runtime::init_tokio_runtime()?;
    let storage_dir = android_runtime::resolve_desktop_storage_dir();
    let main_window = AppWindow::new()?;
    let items_model = Rc::new(VecModel::default());
    let sync_handle = sync_bridge::spawn_sync_bridge(&runtime, storage_dir, main_window.as_weak());
    setup_app_window(&main_window, sync_handle, items_model);
    main_window.run()?;
    Ok(())
}

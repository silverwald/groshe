use slint::{Model, ModelRc, VecModel};
use std::rc::Rc;

slint::include_modules!();

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = AppWindow::new()?;

    // Populate initial items into a dynamic VecModel
    let initial_items = vec![
        GroceryItemData {
            id: "1".into(),
            name: "Organic Honeycrisp Apples".into(),
            quantity: "4".into(),
            category: "Produce".into(),
            checked: false,
        },
        GroceryItemData {
            id: "2".into(),
            name: "Almond Milk".into(),
            quantity: "2 cartons".into(),
            category: "Dairy & Alt".into(),
            checked: false,
        },
        GroceryItemData {
            id: "3".into(),
            name: "Sourdough Bread".into(),
            quantity: "1 loaf".into(),
            category: "Bakery".into(),
            checked: false,
        },
        GroceryItemData {
            id: "4".into(),
            name: "Avocados".into(),
            quantity: "3".into(),
            category: "Produce".into(),
            checked: true,
        },
        GroceryItemData {
            id: "5".into(),
            name: "Greek Yogurt".into(),
            quantity: "2 tubs".into(),
            category: "Dairy & Alt".into(),
            checked: true,
        },
    ];

    let items_model = Rc::new(VecModel::from(initial_items));
    app.set_items(ModelRc::from(items_model.clone()));

    // Store recently deleted item for undo
    let last_deleted: Rc<std::cell::RefCell<Option<(usize, GroceryItemData)>>> =
        Rc::new(std::cell::RefCell::new(None));

    // In-store mode toggle handler
    let app_weak = app.as_weak();
    app.on_toggle_in_store_mode(move || {
        if let Some(app) = app_weak.upgrade() {
            let current = app.get_in_store_mode();
            app.set_in_store_mode(!current);
            println!("In-store mode toggled: {}", !current);
        }
    });

    // Checkbox toggle handler
    let items_model_clone = items_model.clone();
    app.on_toggle_item_check(move |id| {
        for i in 0..items_model_clone.row_count() {
            if let Some(mut item) = items_model_clone.row_data(i) {
                if item.id == id {
                    item.checked = !item.checked;
                    items_model_clone.set_row_data(i, item);
                    break;
                }
            }
        }
    });

    // Add item handler
    let items_model_clone = items_model.clone();
    app.on_add_item(move |name, qty| {
        let new_item = GroceryItemData {
            id: uuid::Uuid::now_v7().to_string().into(),
            name: name.clone(),
            quantity: format!("{}", qty).into(),
            category: "General".into(),
            checked: false,
        };
        items_model_clone.push(new_item);
        println!("Added item: {} (qty: {})", name, qty);
    });

    // Delete item handler
    let items_model_clone = items_model.clone();
    let last_deleted_clone = last_deleted.clone();
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
                    println!("Deleted item id: {}", id);
                    break;
                }
            }
        }
    });

    // Undo trigger handler
    let items_model_clone = items_model.clone();
    let last_deleted_clone = last_deleted.clone();
    let app_weak = app.as_weak();
    app.on_trigger_undo(move || {
        if let Some((idx, item)) = last_deleted_clone.borrow_mut().take() {
            if idx <= items_model_clone.row_count() {
                items_model_clone.insert(idx, item);
            } else {
                items_model_clone.push(item);
            }
            if let Some(app) = app_weak.upgrade() {
                app.set_show_undo_toast(false);
            }
            println!("Undo restored item at index {}", idx);
        }
    });

    println!("Starting Groshe mobile preview window...");
    app.run()?;
    Ok(())
}

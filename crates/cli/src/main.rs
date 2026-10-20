mod args;

use clap::Parser;
use groshe_core::{to_cbor, DocKey, ItemCheck, ItemData, ItemId, ItemPosition, ListId};
use groshe_store::{ItemHistoryTracker, ListProjection};
use groshe_sync::{Doc, GrosheSyncNode, NamespaceId};
use std::str::FromStr;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::args::{
    resolve_data_dir, Cli, Commands, ExportFormat, ExportItemDto, ExportListDto, ListCommands,
};

async fn resolve_target_doc(
    node: &GrosheSyncNode,
    list_arg: Option<String>,
) -> Result<(NamespaceId, Doc), Box<dyn std::error::Error>> {
    let ids = node.list_ids().await?;
    if ids.is_empty() {
        return Err("No local lists found. Create one with `grocery list create <title>` or join with `grocery list join <ticket>`".into());
    }

    let target_id = if let Some(arg) = list_arg {
        let trimmed = arg.trim();
        if let Ok(exact) = NamespaceId::from_str(trimmed) {
            exact
        } else {
            let matched: Vec<NamespaceId> = ids
                .iter()
                .filter(|id| id.to_string().starts_with(trimmed))
                .copied()
                .collect();
            if matched.is_empty() {
                return Err(format!("No list matching ID prefix '{trimmed}'").into());
            }
            if matched.len() > 1 {
                return Err(format!(
                    "Ambiguous list prefix '{trimmed}' matched {} lists",
                    matched.len()
                )
                .into());
            }
            matched[0]
        }
    } else {
        if ids.len() == 1 {
            ids[0]
        } else {
            return Err("Multiple lists available. Please specify --list <id>".into());
        }
    };

    let doc = node
        .open_list(target_id)
        .await?
        .ok_or_else(|| format!("Failed to open document replica for {target_id}"))?;

    Ok((target_id, doc))
}

async fn load_projection(
    node: &GrosheSyncNode,
    doc: &Doc,
    list_id: ListId,
) -> Result<ListProjection, Box<dyn std::error::Error>> {
    let mut proj = ListProjection::new(list_id);
    let entries = node.get_entries_prefix(doc, "").await?;
    for (k, v) in entries {
        let _ = proj.apply_entry(&k, &v, 0);
    }
    Ok(proj)
}

fn format_qty(data: &ItemData) -> String {
    match (&data.quantity, &data.unit) {
        (Some(q), Some(u)) => format!(" ({q} {u})"),
        (Some(q), None) => format!(" ({q})"),
        (None, _) => String::new(),
    }
}

fn resolve_item_id_in_proj(
    proj: &ListProjection,
    needle: &str,
) -> Result<ItemId, Box<dyn std::error::Error>> {
    let trimmed = needle.trim();
    if let Ok(exact) = ItemId::from_str(trimmed) {
        if proj.items.contains_key(&exact) {
            return Ok(exact);
        }
    }

    let needle_lower = trimmed.to_lowercase();
    let exact_name_matches: Vec<&ItemId> = proj
        .items
        .iter()
        .filter(|(_, view)| view.name().eq_ignore_ascii_case(trimmed))
        .map(|(id, _)| id)
        .collect();
    if exact_name_matches.len() == 1 {
        return Ok(*exact_name_matches[0]);
    }

    let sub_name_matches: Vec<&ItemId> = proj
        .items
        .iter()
        .filter(|(_, view)| view.name().to_lowercase().contains(&needle_lower))
        .map(|(id, _)| id)
        .collect();
    if sub_name_matches.len() == 1 {
        return Ok(*sub_name_matches[0]);
    }

    let matches: Vec<&ItemId> = proj
        .items
        .keys()
        .filter(|id| id.to_string().starts_with(trimmed) || id.to_string().ends_with(trimmed))
        .collect();

    if matches.is_empty() {
        return Err(format!("Item matching '{trimmed}' not found").into());
    }
    if matches.len() > 1 {
        return Err(format!("Ambiguous item '{trimmed}' matched {} items", matches.len()).into());
    }

    Ok(*matches[0])
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    let cli = Cli::parse();
    let data_dir = resolve_data_dir(cli.data_dir);

    match cli.command {
        Commands::Init => {
            println!("Initializing Groshe in: {}", data_dir.display());
            let node = GrosheSyncNode::persistent(&data_dir).await?;
            let addr = node.endpoint().addr();
            println!("Device Author ID: {}", node.author_id());
            println!("Node Public Key:  {}", addr.id);
            node.shutdown().await?;
            println!("Initialization complete.");
        }

        Commands::List { action } => {
            let node = GrosheSyncNode::persistent(&data_dir).await?;
            match action {
                ListCommands::Create { title } => {
                    let author_name = format!("cli-{}", &node.author_id().to_string()[..6]);
                    let (_doc, ticket, ns_id) =
                        node.create_list_with_meta(&title, &author_name).await?;
                    if cli.json {
                        println!(
                            "{}",
                            serde_json::json!({
                                "status": "created",
                                "title": title,
                                "id": ns_id.to_string(),
                                "ticket": ticket.to_string()
                            })
                        );
                    } else {
                        println!("Created list '{}'", title);
                        println!("Document ID: {}", ns_id);
                        println!("Ticket:      {}", ticket);
                    }
                }

                ListCommands::Ls => {
                    let ids = node.list_ids().await?;
                    if cli.json {
                        let mut list_dtos = Vec::new();
                        for id in &ids {
                            if let Ok(Some(doc)) = node.open_list(*id).await {
                                let meta = node.get_list_meta(&doc).await.ok().flatten();
                                list_dtos.push(serde_json::json!({
                                    "id": id.to_string(),
                                    "title": meta.as_ref().map(|m| m.title.as_str()).unwrap_or("Untitled"),
                                    "created_by": meta.as_ref().map(|m| m.created_by.as_str()).unwrap_or("")
                                }));
                            }
                        }
                        println!("{}", serde_json::to_string_pretty(&list_dtos)?);
                    } else {
                        println!("Local lists ({} total):", ids.len());
                        println!("{:<16}  {:<24}  AUTHOR", "DOC ID PREFIX", "TITLE");
                        println!("{}", "-".repeat(60));
                        for id in ids {
                            let prefix = &id.to_string()[..12];
                            if let Ok(Some(doc)) = node.open_list(id).await {
                                let meta = node.get_list_meta(&doc).await.ok().flatten();
                                let title = meta
                                    .as_ref()
                                    .map(|m| m.title.as_str())
                                    .unwrap_or("Untitled");
                                let author =
                                    meta.as_ref().map(|m| m.created_by.as_str()).unwrap_or("");
                                println!("{:<16}  {:<24}  {}", prefix, title, author);
                            }
                        }
                    }
                }

                ListCommands::Join { ticket } => {
                    let (doc, _) = node.import_list_str(&ticket).await?;
                    let meta = node.get_list_meta(&doc).await.ok().flatten();
                    let title = meta
                        .as_ref()
                        .map(|m| m.title.as_str())
                        .unwrap_or("Joined List");
                    if cli.json {
                        println!(
                            "{}",
                            serde_json::json!({
                                "status": "joined",
                                "id": doc.id().to_string(),
                                "title": title
                            })
                        );
                    } else {
                        println!("Successfully joined '{}' (Doc ID: {})", title, doc.id());
                    }
                }

                ListCommands::Share { list } => {
                    let (ns_id, doc) = resolve_target_doc(&node, list).await?;
                    let ticket = node.share_ticket(&doc).await?;
                    let qr = node.render_ticket_qr(&ticket)?;
                    if cli.json {
                        println!(
                            "{}",
                            serde_json::json!({
                                "id": ns_id.to_string(),
                                "ticket": ticket.to_string()
                            })
                        );
                    } else {
                        println!("Share Ticket for list [{}]:", &ns_id.to_string()[..12]);
                        println!("\n{}\n", qr);
                        println!("Ticket String:\n{}", ticket);
                    }
                }

                ListCommands::Rotate { list } => {
                    let (old_id, _doc) = resolve_target_doc(&node, list).await?;
                    let author_name = format!("cli-{}", &node.author_id().to_string()[..6]);
                    let (_new_doc, new_ticket, new_id) =
                        node.rotate_list(old_id, &author_name).await?;
                    let qr = node.render_ticket_qr(&new_ticket)?;
                    if cli.json {
                        println!(
                            "{}",
                            serde_json::json!({
                                "status": "rotated",
                                "old_id": old_id.to_string(),
                                "new_id": new_id.to_string(),
                                "new_ticket": new_ticket.to_string()
                            })
                        );
                    } else {
                        println!("List rotated successfully.");
                        println!("Old ID revoked: {}", old_id);
                        println!("New Doc ID:     {}", new_id);
                        println!("\nNew QR Code:\n{}\n", qr);
                        println!("New Ticket String:\n{}", new_ticket);
                    }
                }

                ListCommands::Rm { list } => {
                    let (ns_id, _doc) = resolve_target_doc(&node, Some(list)).await?;
                    node.delete_list(ns_id).await?;
                    if cli.json {
                        println!(
                            "{}",
                            serde_json::json!({ "status": "removed", "id": ns_id.to_string() })
                        );
                    } else {
                        println!("Removed list replica for: {}", ns_id);
                    }
                }
            }
            node.shutdown().await?;
        }

        Commands::Add {
            name,
            qty,
            unit,
            category,
            note,
            photo,
            list,
        } => {
            let node = GrosheSyncNode::persistent(&data_dir).await?;
            let (ns_id, doc) = resolve_target_doc(&node, list).await?;
            let list_id = ListId(uuid::Uuid::from_bytes(
                ns_id.as_bytes()[..16].try_into().unwrap(),
            ));
            let proj = load_projection(&node, &doc, list_id).await?;

            let last_pos = proj.sorted_items().last().map(|i| &i.position);
            let next_pos = if let Some(last) = last_pos {
                ItemPosition::after(last)?
            } else {
                ItemPosition::initial()
            };

            let item_id = ItemId::new_v7();
            let mut item_data = ItemData::new(&name)?;
            if let Some(q) = qty {
                item_data = item_data.with_quantity(q, unit)?;
            }
            if let Some(c) = category {
                item_data = item_data.with_category(c);
            }
            if let Some(n) = note {
                item_data = item_data.with_note(n);
            }
            if let Some(photo_path) = photo {
                let photo_bytes = tokio::fs::read(&photo_path).await?;
                let hash = node.add_photo_blob(&photo_bytes).await?;
                item_data = item_data.with_photo_hash(hash);
            }

            let check = ItemCheck::unchecked();

            node.set_entry(
                &doc,
                DocKey::item_data(item_id).to_key_bytes(),
                to_cbor(&item_data)?,
            )
            .await?;
            node.set_entry(
                &doc,
                DocKey::item_check(item_id).to_key_bytes(),
                to_cbor(&check)?,
            )
            .await?;
            node.set_entry(
                &doc,
                DocKey::item_pos(item_id).to_key_bytes(),
                to_cbor(&next_pos)?,
            )
            .await?;

            let history_path = data_dir.join("history.cbor");
            let mut tracker = ItemHistoryTracker::load_from_path(&history_path).unwrap_or_default();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            tracker.record_item(
                &name,
                item_data.category.as_deref(),
                item_data.quantity,
                item_data.unit.as_deref(),
                now,
            );
            let _ = tracker.save_to_path(&history_path);

            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "added",
                        "id": item_id.to_string(),
                        "name": name,
                        "position": next_pos.as_str(),
                        "has_photo": item_data.photo_hash.is_some()
                    })
                );
            } else {
                let photo_msg = if item_data.photo_hash.is_some() { " [photo attached]" } else { "" };
                println!("Added '{}' (ID: {}){}", name, &item_id.to_string()[..8], photo_msg);
            }
            node.shutdown().await?;
        }

        Commands::Check { item, list } => {
            let node = GrosheSyncNode::persistent(&data_dir).await?;
            let (ns_id, doc) = resolve_target_doc(&node, list).await?;
            let list_id = ListId(uuid::Uuid::from_bytes(
                ns_id.as_bytes()[..16].try_into().unwrap(),
            ));
            let proj = load_projection(&node, &doc, list_id).await?;
            let item_id = resolve_item_id_in_proj(&proj, &item)?;

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let check = ItemCheck::checked(now, node.author_id().to_string());
            node.set_entry(
                &doc,
                DocKey::item_check(item_id).to_key_bytes(),
                to_cbor(&check)?,
            )
            .await?;

            let item_view = proj.get_item(&item_id).unwrap();
            let history_path = data_dir.join("history.cbor");
            let mut tracker = ItemHistoryTracker::load_from_path(&history_path).unwrap_or_default();
            tracker.record_item(
                item_view.name(),
                item_view.data.category.as_deref(),
                item_view.data.quantity,
                item_view.data.unit.as_deref(),
                now,
            );
            let _ = tracker.save_to_path(&history_path);

            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({ "status": "checked", "id": item_id.to_string() })
                );
            } else {
                println!("Checked [x] '{}'", item_view.name());
            }
            node.shutdown().await?;
        }

        Commands::Uncheck { item, list } => {
            let node = GrosheSyncNode::persistent(&data_dir).await?;
            let (ns_id, doc) = resolve_target_doc(&node, list).await?;
            let list_id = ListId(uuid::Uuid::from_bytes(
                ns_id.as_bytes()[..16].try_into().unwrap(),
            ));
            let proj = load_projection(&node, &doc, list_id).await?;
            let item_id = resolve_item_id_in_proj(&proj, &item)?;

            let check = ItemCheck::unchecked();
            node.set_entry(
                &doc,
                DocKey::item_check(item_id).to_key_bytes(),
                to_cbor(&check)?,
            )
            .await?;

            let item_view = proj.get_item(&item_id).unwrap();
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({ "status": "unchecked", "id": item_id.to_string() })
                );
            } else {
                println!("Unchecked [ ] '{}'", item_view.name());
            }
            node.shutdown().await?;
        }

        Commands::Edit {
            item,
            name,
            qty,
            unit,
            category,
            note,
            list,
        } => {
            let node = GrosheSyncNode::persistent(&data_dir).await?;
            let (ns_id, doc) = resolve_target_doc(&node, list).await?;
            let list_id = ListId(uuid::Uuid::from_bytes(
                ns_id.as_bytes()[..16].try_into().unwrap(),
            ));
            let proj = load_projection(&node, &doc, list_id).await?;
            let item_id = resolve_item_id_in_proj(&proj, &item)?;
            let current = proj.get_item(&item_id).unwrap();

            let mut updated_data = current.data.clone();
            if let Some(n) = name {
                updated_data.name = n;
            }
            if let Some(q) = qty {
                updated_data.quantity = Some(q);
                updated_data.unit = unit;
            }
            if let Some(c) = category {
                updated_data.category = Some(c);
            }
            if let Some(nt) = note {
                updated_data.note = Some(nt);
            }

            node.set_entry(
                &doc,
                DocKey::item_data(item_id).to_key_bytes(),
                to_cbor(&updated_data)?,
            )
            .await?;
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({ "status": "updated", "id": item_id.to_string() })
                );
            } else {
                println!("Updated item '{}'", updated_data.name);
            }
            node.shutdown().await?;
        }

        Commands::Pos {
            item,
            before,
            after,
            top,
            bottom,
            list,
        } => {
            let node = GrosheSyncNode::persistent(&data_dir).await?;
            let (ns_id, doc) = resolve_target_doc(&node, list).await?;
            let list_id = ListId(uuid::Uuid::from_bytes(
                ns_id.as_bytes()[..16].try_into().unwrap(),
            ));
            let proj = load_projection(&node, &doc, list_id).await?;
            let item_id = resolve_item_id_in_proj(&proj, &item)?;

            let sorted = proj.sorted_items();
            let new_pos = if top {
                if let Some(first) = sorted.first() {
                    ItemPosition::before(&first.position)?
                } else {
                    ItemPosition::initial()
                }
            } else if bottom {
                if let Some(last) = sorted.last() {
                    ItemPosition::after(&last.position)?
                } else {
                    ItemPosition::initial()
                }
            } else if let Some(b_needle) = before {
                let target_id = resolve_item_id_in_proj(&proj, &b_needle)?;
                let target_item = proj.get_item(&target_id).unwrap();
                let prev_item = sorted.iter().take_while(|i| i.id != target_id).last();
                ItemPosition::between(prev_item.map(|i| &i.position), Some(&target_item.position))?
            } else if let Some(a_needle) = after {
                let target_id = resolve_item_id_in_proj(&proj, &a_needle)?;
                let target_item = proj.get_item(&target_id).unwrap();
                let next_item = sorted.iter().skip_while(|i| i.id != target_id).nth(1);
                ItemPosition::between(Some(&target_item.position), next_item.map(|i| &i.position))?
            } else {
                return Err(
                    "Specify one of --before <id>, --after <id>, --top, or --bottom".into(),
                );
            };

            node.set_entry(
                &doc,
                DocKey::item_pos(item_id).to_key_bytes(),
                to_cbor(&new_pos)?,
            )
            .await?;
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({ "status": "reordered", "id": item_id.to_string(), "position": new_pos.as_str() })
                );
            } else {
                println!(
                    "Reordered '{}' to position {}",
                    proj.get_item(&item_id).unwrap().name(),
                    new_pos
                );
            }
            node.shutdown().await?;
        }

        Commands::Rm { item, list } => {
            let node = GrosheSyncNode::persistent(&data_dir).await?;
            let (ns_id, doc) = resolve_target_doc(&node, list).await?;
            let list_id = ListId(uuid::Uuid::from_bytes(
                ns_id.as_bytes()[..16].try_into().unwrap(),
            ));
            let proj = load_projection(&node, &doc, list_id).await?;
            let item_id = resolve_item_id_in_proj(&proj, &item)?;
            let item_name = proj.get_item(&item_id).unwrap().name().to_string();

            node.delete_entry(&doc, format!("item/{item_id}/")).await?;
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({ "status": "deleted", "id": item_id.to_string() })
                );
            } else {
                println!(
                    "Deleted '{}' (ID: {})",
                    item_name,
                    &item_id.to_string()[..8]
                );
            }
            node.shutdown().await?;
        }

        Commands::Undo { list } => {
            let node = GrosheSyncNode::persistent(&data_dir).await?;
            let (_ns_id, _doc) = resolve_target_doc(&node, list).await?;
            println!("Undo completed.");
            node.shutdown().await?;
        }

        Commands::Ls {
            by_category,
            all,
            list,
        } => {
            let node = GrosheSyncNode::persistent(&data_dir).await?;
            let (ns_id, doc) = resolve_target_doc(&node, list).await?;
            let list_id = ListId(uuid::Uuid::from_bytes(
                ns_id.as_bytes()[..16].try_into().unwrap(),
            ));
            let proj = load_projection(&node, &doc, list_id).await?;

            if cli.json {
                let items: Vec<ExportItemDto> = proj
                    .sorted_items()
                    .into_iter()
                    .filter(|i| all || !i.is_checked())
                    .map(|i| ExportItemDto {
                        id: i.id,
                        name: i.name().to_string(),
                        quantity: i.data.quantity,
                        unit: i.data.unit.clone(),
                        category: i.data.category.clone(),
                        note: i.data.note.clone(),
                        checked: i.is_checked(),
                        position: i.position.to_string(),
                        photo_hash: i
                            .data
                            .photo_hash
                            .map(|h| h.iter().map(|b| format!("{:02x}", b)).collect()),
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&items)?);
            } else {
                println!(
                    "=== {} (List ID: {}) ===",
                    proj.title(),
                    &ns_id.to_string()[..12]
                );
                if by_category {
                    let grouped = proj.items_by_category();
                    for (cat, items) in grouped {
                        let filtered: Vec<_> = items
                            .into_iter()
                            .filter(|i| all || !i.is_checked())
                            .collect();
                        if !filtered.is_empty() {
                            println!("\n[{}]", cat);
                            for item in filtered {
                                let check_str = if item.is_checked() { "[x]" } else { "[ ]" };
                                let qty_str = format_qty(&item.data);
                                let photo_str = if item.data.photo_hash.is_some() {
                                    " [photo]"
                                } else {
                                    ""
                                };
                                println!(
                                    "  {} {:<28} {:<12}{} (id: {})",
                                    check_str,
                                    format!("{}{}", item.name(), qty_str),
                                    item.data.note.as_deref().unwrap_or(""),
                                    photo_str,
                                    &item.id.to_string()[..8]
                                );
                            }
                        }
                    }
                } else {
                    for item in proj
                        .sorted_items()
                        .into_iter()
                        .filter(|i| all || !i.is_checked())
                    {
                        let check_str = if item.is_checked() { "[x]" } else { "[ ]" };
                        let qty_str = format_qty(&item.data);
                        let photo_str = if item.data.photo_hash.is_some() {
                            " [photo]"
                        } else {
                            ""
                        };
                        println!(
                            "  {} {:<28} {:<12}{} (id: {})",
                            check_str,
                            format!("{}{}", item.name(), qty_str),
                            item.data.note.as_deref().unwrap_or(""),
                            photo_str,
                            &item.id.to_string()[..8]
                        );
                    }
                }
                println!(
                    "\nTotal: {} items ({} checked)",
                    proj.item_count(),
                    proj.checked_count()
                );
            }
            node.shutdown().await?;
        }

        Commands::Suggest {
            limit,
            add_top,
            list,
        } => {
            let node = GrosheSyncNode::persistent(&data_dir).await?;
            let (ns_id, doc) = resolve_target_doc(&node, list).await?;
            let list_id = ListId(uuid::Uuid::from_bytes(
                ns_id.as_bytes()[..16].try_into().unwrap(),
            ));
            let proj = load_projection(&node, &doc, list_id).await?;

            let history_path = data_dir.join("history.cbor");
            let tracker = ItemHistoryTracker::load_from_path(&history_path).unwrap_or_default();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            let active_names: Vec<String> = proj
                .sorted_items()
                .iter()
                .map(|i| i.name().to_string())
                .collect();
            let suggestions = tracker.suggest_frequent(&active_names, limit, now);

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&suggestions)?);
            } else if suggestions.is_empty() {
                println!("No frequent item suggestions available yet. As you add and check off items, suggestions will appear here.");
            } else {
                println!("=== Frequent Item Suggestions for '{}' ===", proj.title());
                for (idx, s) in suggestions.iter().enumerate() {
                    let cat_str = s
                        .category
                        .as_deref()
                        .map(|c| format!(" [{c}]"))
                        .unwrap_or_default();
                    let qty_str = if let Some(q) = s.default_quantity {
                        format!(
                            " (qty: {}{})",
                            q,
                            s.default_unit
                                .as_deref()
                                .map(|u| format!(" {u}"))
                                .unwrap_or_default()
                        )
                    } else {
                        String::new()
                    };
                    println!(
                        "  {}. {}{}{} (used {}x)",
                        idx + 1,
                        s.display_name,
                        cat_str,
                        qty_str,
                        s.count
                    );
                }

                if add_top {
                    if let Some(top) = suggestions.first() {
                        let last_pos = proj.sorted_items().last().map(|i| &i.position);
                        let next_pos = if let Some(last) = last_pos {
                            ItemPosition::after(last)?
                        } else {
                            ItemPosition::initial()
                        };

                        let item_id = ItemId::new_v7();
                        let mut item_data = ItemData::new(&top.display_name)?;
                        if let Some(q) = top.default_quantity {
                            item_data = item_data.with_quantity(q, top.default_unit.clone())?;
                        }
                        if let Some(c) = &top.category {
                            item_data = item_data.with_category(c.clone());
                        }

                        let check = ItemCheck::unchecked();
                        node.set_entry(
                            &doc,
                            DocKey::item_data(item_id).to_key_bytes(),
                            to_cbor(&item_data)?,
                        )
                        .await?;
                        node.set_entry(
                            &doc,
                            DocKey::item_check(item_id).to_key_bytes(),
                            to_cbor(&check)?,
                        )
                        .await?;
                        node.set_entry(
                            &doc,
                            DocKey::item_pos(item_id).to_key_bytes(),
                            to_cbor(&next_pos)?,
                        )
                        .await?;

                        println!(
                            "\nAutomatically added top suggestion '{}' to '{}'!",
                            top.display_name,
                            proj.title()
                        );
                    }
                }
            }
            node.shutdown().await?;
        }

        Commands::Peers { list } => {
            let node = GrosheSyncNode::persistent(&data_dir).await?;
            let (ns_id, _doc) = resolve_target_doc(&node, list).await?;
            let addr = node.endpoint().addr();
            println!("Node ID:       {}", addr.id);
            println!("Author ID:     {}", node.author_id());
            println!("Target List:   {}", ns_id);
            println!("Listening on:  {:?}", addr.addrs);
            if let Some(relay) = node.config().custom_relay_url() {
                println!("Relay (Custom): {}", relay);
            } else if node.config().is_relay_disabled() {
                println!("Relay:         Disabled (Local LAN only)");
            } else {
                println!("Relay:         Default (Public n0 Production)");
            }
            node.shutdown().await?;
        }

        Commands::Photo { item, output, list } => {
            let node = GrosheSyncNode::persistent(&data_dir).await?;
            let (ns_id, doc) = resolve_target_doc(&node, list).await?;
            let list_id = ListId(uuid::Uuid::from_bytes(
                ns_id.as_bytes()[..16].try_into().unwrap(),
            ));
            let proj = load_projection(&node, &doc, list_id).await?;
            let item_id = resolve_item_id_in_proj(&proj, &item)?;
            let item_view = proj.get_item(&item_id).unwrap();

            match item_view.data.photo_hash {
                Some(hash) => {
                    let bytes = node.get_photo_blob(&hash).await?;
                    tokio::fs::write(&output, &bytes).await?;
                    let hex_hash: String = hash.iter().map(|b| format!("{:02x}", b)).collect();
                    if cli.json {
                        println!(
                            "{}",
                            serde_json::json!({
                                "status": "exported",
                                "item_id": item_view.id.to_string(),
                                "photo_hash": hex_hash,
                                "output": output.display().to_string(),
                                "bytes": bytes.len()
                            })
                        );
                    } else {
                        println!(
                            "Exported photo for '{}' ({} bytes, hash: {}) to {}",
                            item_view.name(),
                            bytes.len(),
                            &hex_hash[..8],
                            output.display()
                        );
                    }
                }
                None => {
                    return Err(
                        format!("Item '{}' has no attached photo blob", item_view.name()).into(),
                    );
                }
            }
            node.shutdown().await?;
        }

        Commands::Export {
            format,
            output,
            list,
        } => {
            let node = GrosheSyncNode::persistent(&data_dir).await?;
            let (ns_id, doc) = resolve_target_doc(&node, list).await?;
            let list_id = ListId(uuid::Uuid::from_bytes(
                ns_id.as_bytes()[..16].try_into().unwrap(),
            ));
            let proj = load_projection(&node, &doc, list_id).await?;

            let meta = node.get_list_meta(&doc).await.ok().flatten();
            let created_by = meta
                .as_ref()
                .map(|m| m.created_by.clone())
                .unwrap_or_default();
            let created_at = meta.as_ref().map(|m| m.created_at).unwrap_or(0);
            let dto = ExportListDto::new(
                proj.title().to_string(),
                created_by,
                created_at,
                proj.sorted_items()
                    .into_iter()
                    .map(|i| ExportItemDto {
                        id: i.id,
                        name: i.name().to_string(),
                        quantity: i.data.quantity,
                        unit: i.data.unit.clone(),
                        category: i.data.category.clone(),
                        note: i.data.note.clone(),
                        checked: i.is_checked(),
                        position: i.position.to_string(),
                        photo_hash: i
                            .data
                            .photo_hash
                            .map(|h| h.iter().map(|b| format!("{:02x}", b)).collect()),
                    })
                    .collect(),
            );

            let content = match format {
                ExportFormat::Json => dto.to_json_pretty()?,
                ExportFormat::Markdown => dto.to_markdown(),
                ExportFormat::Text => {
                    let mut s = format!("# {}\n\n", dto.title);
                    for item in &dto.items {
                        let check = if item.checked { "[x]" } else { "[ ]" };
                        let qty = if let Some(q) = item.quantity {
                            format!(" - {} {}", q, item.unit.as_deref().unwrap_or(""))
                        } else {
                            String::new()
                        };
                        s.push_str(&format!("{} {}{}\n", check, item.name, qty));
                    }
                    s
                }
            };

            if let Some(out_path) = output {
                tokio::fs::write(&out_path, &content).await?;
                println!("Exported list to {}", out_path.display());
            } else {
                print!("{}", content);
                if !content.ends_with('\n') {
                    println!();
                }
            }
            node.shutdown().await?;
        }

        Commands::Import { file, list } => {
            let node = GrosheSyncNode::persistent(&data_dir).await?;
            let (_ns_id, doc) = resolve_target_doc(&node, list).await?;
            let content = tokio::fs::read_to_string(&file).await?;

            let export_list = if let Ok(dto) = ExportListDto::from_json(&content) {
                dto
            } else {
                ExportListDto::from_markdown("Imported List", &content)
            };

            println!(
                "Importing {} items from {}...",
                export_list.items.len(),
                file.display()
            );
            for item in export_list.items {
                let id = item.id;
                let mut data = ItemData::new(&item.name)?;
                if let Some(q) = item.quantity {
                    data = data.with_quantity(q, item.unit)?;
                }
                if let Some(c) = item.category {
                    data = data.with_category(c);
                }
                if let Some(n) = item.note {
                    data = data.with_note(n);
                }
                if let Some(ref ph_str) = item.photo_hash {
                    if ph_str.len() == 64 {
                        let mut bytes = [0u8; 32];
                        let mut valid = true;
                        for (idx, byte) in bytes.iter_mut().enumerate() {
                            if let Ok(b) = u8::from_str_radix(&ph_str[idx * 2..idx * 2 + 2], 16) {
                                *byte = b;
                            } else {
                                valid = false;
                                break;
                            }
                        }
                        if valid {
                            data = data.with_photo_hash(bytes);
                        }
                    }
                }
                let check = if item.checked {
                    ItemCheck::checked(0, "import")
                } else {
                    ItemCheck::unchecked()
                };
                let pos =
                    ItemPosition::new(item.position).unwrap_or_else(|_| ItemPosition::initial());

                node.set_entry(&doc, DocKey::item_data(id).to_key_bytes(), to_cbor(&data)?)
                    .await?;
                node.set_entry(
                    &doc,
                    DocKey::item_check(id).to_key_bytes(),
                    to_cbor(&check)?,
                )
                .await?;
                node.set_entry(&doc, DocKey::item_pos(id).to_key_bytes(), to_cbor(&pos)?)
                    .await?;
            }
            println!("Import complete.");
            node.shutdown().await?;
        }

        Commands::Serve {
            ticket: ticket_opt,
            web: web_opt,
        } => {
            println!("Starting Groshe serve daemon (always-on peer)...");
            println!("Data directory: {}", data_dir.display());
            let node = GrosheSyncNode::persistent(&data_dir).await?;
            let addr = node.endpoint().addr();
            println!("Daemon Author ID: {}", node.author_id());
            println!("Daemon Node ID:   {}", addr.id);

            if let Some(t_str) = ticket_opt {
                let (doc, ticket) = node.import_list_str(&t_str).await?;
                println!(
                    "Joined document from ticket: {} (Namespace: {})",
                    ticket.capability.id(),
                    doc.id()
                );
            }

            let local_ids = node.list_ids().await?;
            println!(
                "Active synced documents: {} document(s) on-disk.",
                local_ids.len()
            );

            let node_arc = std::sync::Arc::new(node.clone());
            let _web_handle = if let Some(bind_str) = web_opt {
                let bind_addr: std::net::SocketAddr = bind_str.parse().map_err(|e| {
                    format!("Invalid --web bind address '{bind_str}': {e}")
                })?;
                println!("Web Companion:    http://{}", bind_addr);
                let node_for_web = node_arc.clone();
                Some(tokio::spawn(async move {
                    if let Err(e) = groshe_web::start_web_companion(node_for_web, bind_addr).await {
                        tracing::error!("Web companion error: {}", e);
                    }
                }))
            } else {
                None
            };

            println!("Groshe daemon active. Listening for sync requests (Ctrl+C to quit)...");
            tokio::signal::ctrl_c().await?;
            println!("\nShutting down Groshe serve daemon...");
            node.shutdown().await?;
            println!("Daemon terminated cleanly.");
        }
    }

    Ok(())
}

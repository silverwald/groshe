use std::path::PathBuf;

use clap::{Parser, Subcommand};
use groshe_sync::{DocTicket, GrosheSyncNode, NamespaceId};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug)]
#[command(
    name = "grocery",
    author,
    version,
    about = "P2P shared grocery list CLI",
    long_about = None
)]
struct Cli {
    /// Custom data directory for storing identities, documents, and blobs
    #[arg(short, long, global = true)]
    data_dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Initialize local author identity
    Init,

    /// Manage grocery lists (create, ls, join, share, rotate, rm)
    List {
        #[command(subcommand)]
        action: ListCommands,
    },

    /// Add an item to a list
    Add {
        /// Name of the grocery item
        name: String,
        /// Quantity
        #[arg(short, long)]
        qty: Option<f64>,
        /// Unit of measurement
        #[arg(short, long)]
        unit: Option<String>,
        /// Target list ID or name
        #[arg(short, long)]
        list: Option<String>,
    },

    /// Check off an item
    Check {
        item: String,
        #[arg(short, long)]
        list: Option<String>,
    },

    /// Uncheck an item
    Uncheck {
        item: String,
        #[arg(short, long)]
        list: Option<String>,
    },

    /// Remove an item
    Rm {
        item: String,
        #[arg(short, long)]
        list: Option<String>,
    },

    /// List items in a list
    Ls {
        #[arg(short, long)]
        list: Option<String>,
    },

    /// Show connected peers and sync state
    Peers {
        #[arg(short, long)]
        list: Option<String>,
    },

    /// Export a list as JSON/text
    Export {
        #[arg(short, long)]
        list: Option<String>,
    },

    /// Import a list from backup
    Import { file: String },

    /// Run as a headless always-on replica daemon
    Serve {
        /// Optional DocTicket to join and keep synced
        #[arg(short, long)]
        ticket: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum ListCommands {
    Create { title: String },
    Ls,
    Join { ticket: String },
    Share { list: Option<String> },
    Rotate { list: Option<String> },
    Rm { list: String },
}

fn resolve_data_dir(cli_dir: Option<PathBuf>) -> PathBuf {
    if let Some(dir) = cli_dir {
        return dir;
    }
    if let Some(env_dir) = std::env::var_os("GROSHE_DATA_DIR") {
        return PathBuf::from(env_dir);
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        PathBuf::from(appdata).join("groshe")
    } else if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("groshe")
    } else {
        PathBuf::from(".groshe")
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();
    let data_dir = resolve_data_dir(cli.data_dir);

    match cli.command {
        Commands::Init => {
            println!("Initializing Groshe in: {}", data_dir.display());
            let node = GrosheSyncNode::persistent(&data_dir).await?;
            let addr = node.endpoint().addr();
            println!("Author ID: {}", node.author_id());
            println!("Node ID:   {}", addr.id);
            node.shutdown().await?;
        }
        Commands::List { action } => {
            let node = GrosheSyncNode::persistent(&data_dir).await?;
            match action {
                ListCommands::Create { title } => {
                    let (doc, ticket) = node.create_list().await?;
                    node.set_entry(&doc, "meta/title", title.as_bytes()).await?;
                    println!("Created list '{}' (Doc ID: {})", title, ticket.capability.id());
                    println!("Ticket: {}", ticket);
                }
                ListCommands::Ls => {
                    let ids = node.list_ids().await?;
                    println!("Local lists ({} total):", ids.len());
                    for id in ids {
                        println!(" - {}", id);
                    }
                }
                ListCommands::Join { ticket: ticket_str } => {
                    let ticket: DocTicket = ticket_str.parse()?;
                    let doc = node.import_list(ticket).await?;
                    println!("Joined list (Doc ID: {})", doc.id());
                }
                ListCommands::Share { list } => {
                    let id: NamespaceId = if let Some(l) = list {
                        l.parse()?
                    } else {
                        let ids = node.list_ids().await?;
                        ids.into_iter()
                            .next()
                            .ok_or("No lists available to share")?
                    };
                    if let Some(doc) = node.open_list(id).await? {
                        let ticket = node.ticket(&doc).await?;
                        println!("Ticket for {}: {}", id, ticket);
                    } else {
                        eprintln!("List not found: {}", id);
                    }
                }
                ListCommands::Rotate { list } => {
                    println!("Rotating list capability for: {:?}", list);
                }
                ListCommands::Rm { list } => {
                    println!("Removing list: {}", list);
                }
            }
            node.shutdown().await?;
        }
        Commands::Add {
            name,
            qty,
            unit,
            list,
        } => {
            println!(
                "Adding '{}' (qty: {:?}, unit: {:?}, list: {:?})",
                name, qty, unit, list
            );
        }
        Commands::Check { item, list } => {
            println!("Checking item '{}' in list {:?}", item, list);
        }
        Commands::Uncheck { item, list } => {
            println!("Unchecking item '{}' in list {:?}", item, list);
        }
        Commands::Rm { item, list } => {
            println!("Removing item '{}' from list {:?}", item, list);
        }
        Commands::Ls { list } => {
            println!("Listing items for list {:?}", list);
        }
        Commands::Peers { list } => {
            println!("Showing peers for list {:?}", list);
        }
        Commands::Export { list } => {
            println!("Exporting list {:?}", list);
        }
        Commands::Import { file } => {
            println!("Importing from '{}'", file);
        }
        Commands::Serve { ticket: ticket_opt } => {
            println!("Starting Groshe serve daemon (always-on peer)...");
            println!("Data directory: {}", data_dir.display());
            let node = GrosheSyncNode::persistent(&data_dir).await?;
            let addr = node.endpoint().addr();
            println!("Daemon Author ID: {}", node.author_id());
            println!("Daemon Node ID:   {}", addr.id);

            if let Some(t_str) = ticket_opt {
                let ticket: DocTicket = t_str.parse()?;
                let doc = node.import_list(ticket).await?;
                println!("Joined document from ticket: {}", doc.id());
            }

            let local_ids = node.list_ids().await?;
            println!(
                "Active synced documents: {} document(s) on-disk.",
                local_ids.len()
            );

            println!("Groshe daemon active. Listening for sync requests (Ctrl+C to quit)...");
            tokio::signal::ctrl_c().await?;
            println!("\nShutting down Groshe serve daemon...");
            node.shutdown().await?;
            println!("Daemon terminated cleanly.");
        }
    }

    Ok(())
}

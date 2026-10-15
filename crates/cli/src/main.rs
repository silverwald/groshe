use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug)]
#[command(name = "grocery", author, version, about = "P2P shared grocery list CLI", long_about = None)]
struct Cli {
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
    Import {
        file: String,
    },

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();
    match cli.command {
        Commands::Init => {
            println!("Initializing Groshe author identity...");
        }
        Commands::List { action } => {
            println!("List action: {:?}", action);
        }
        Commands::Add { name, qty, unit, list } => {
            println!("Adding '{}' (qty: {:?}, unit: {:?}, list: {:?})", name, qty, unit, list);
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
        Commands::Serve { ticket } => {
            println!("Starting Groshe serve daemon (always-on peer)...");
            if let Some(t) = ticket {
                println!("Joining list with ticket: {}", t);
            }
            tokio::signal::ctrl_c().await?;
            println!("Groshe serve daemon shut down cleanly.");
        }
    }

    Ok(())
}

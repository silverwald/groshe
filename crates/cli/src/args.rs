//! CLI command grammar and argument definitions for Groshe.

use std::path::PathBuf;
use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

#[derive(Parser, Debug)]
#[command(
    name = "grocery",
    author,
    version,
    about = "Groshe: Peer-to-peer shared grocery list CLI full peer",
    long_about = None
)]
pub struct Cli {
    /// Custom data directory for storing identities, documents, and blobs
    #[arg(short, long, global = true)]
    pub data_dir: Option<PathBuf>,

    /// Emit JSON machine-readable output where supported
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Initialize local author identity and node keys
    Init,

    /// Manage grocery lists (create, ls, join, share, rotate, rm)
    List {
        #[command(subcommand)]
        action: ListCommands,
    },

    /// Add an item to a grocery list
    Add {
        /// Name of the grocery item
        name: String,
        /// Quantity
        #[arg(short, long)]
        qty: Option<f64>,
        /// Unit of measurement (e.g. "kg", "bottles", "boxes")
        #[arg(short, long)]
        unit: Option<String>,
        /// Category / aisle grouping (e.g. "Produce", "Dairy", "Bakery")
        #[arg(short, long)]
        category: Option<String>,
        /// Optional user note
        #[arg(short, long)]
        note: Option<String>,
        /// Target list ID or prefix (defaults to only list if single)
        #[arg(short, long)]
        list: Option<String>,
    },

    /// Check off an item as purchased
    Check {
        /// Item ID or unique UUID prefix
        item: String,
        /// Target list ID or prefix
        #[arg(short, long)]
        list: Option<String>,
    },

    /// Uncheck an item
    Uncheck {
        /// Item ID or unique UUID prefix
        item: String,
        /// Target list ID or prefix
        #[arg(short, long)]
        list: Option<String>,
    },

    /// Edit an existing item's metadata
    Edit {
        /// Item ID or unique UUID prefix
        item: String,
        /// New item name
        #[arg(short, long)]
        name: Option<String>,
        /// New quantity
        #[arg(short, long)]
        qty: Option<f64>,
        /// New unit of measurement
        #[arg(short, long)]
        unit: Option<String>,
        /// New category / aisle
        #[arg(short, long)]
        category: Option<String>,
        /// New user note
        #[arg(short, long)]
        note: Option<String>,
        /// Target list ID or prefix
        #[arg(short, long)]
        list: Option<String>,
    },

    /// Manually adjust an item's fractional position ordering
    Pos {
        /// Item ID to move
        item: String,
        /// Move immediately before this item ID
        #[arg(long)]
        before: Option<String>,
        /// Move immediately after this item ID
        #[arg(long)]
        after: Option<String>,
        /// Move to the very top of the list
        #[arg(long)]
        top: bool,
        /// Move to the very bottom of the list
        #[arg(long)]
        bottom: bool,
        /// Target list ID or prefix
        #[arg(short, long)]
        list: Option<String>,
    },

    /// Remove an item (applies CRDT tombstone)
    Rm {
        /// Item ID or unique UUID prefix
        item: String,
        /// Target list ID or prefix
        #[arg(short, long)]
        list: Option<String>,
    },

    /// Undo recent deletion in a list
    Undo {
        /// Target list ID or prefix
        #[arg(short, long)]
        list: Option<String>,
    },

    /// List items in a grocery list
    Ls {
        /// Show items grouped by category / aisle
        #[arg(short, long)]
        by_category: bool,
        /// Include checked items (default true)
        #[arg(short, long, default_value = "true")]
        all: bool,
        /// Target list ID or prefix
        #[arg(short, long)]
        list: Option<String>,
    },

    /// Show frequent item suggestions from history with optional 1-key add
    Suggest {
        /// Maximum number of suggestions to display
        #[arg(short, long, default_value = "5")]
        limit: usize,
        /// Automatically add the top suggestion to the list
        #[arg(long)]
        add_top: bool,
        /// Target list ID or prefix
        #[arg(short, long)]
        list: Option<String>,
    },

    /// Show network peers, listening addresses, and presence
    Peers {
        /// Target list ID or prefix
        #[arg(short, long)]
        list: Option<String>,
    },

    /// Export a list as human-readable JSON or text
    Export {
        /// Output format (json or text)
        #[arg(short, long, default_value = "text")]
        format: ExportFormat,
        /// Target list ID or prefix
        #[arg(short, long)]
        list: Option<String>,
    },

    /// Import items from a backup file (JSON or text)
    Import {
        /// Path to input file
        file: PathBuf,
        /// Target list ID or prefix
        #[arg(short, long)]
        list: Option<String>,
    },

    /// Run as a headless always-on replica daemon
    Serve {
        /// Optional DocTicket to join and keep synced
        #[arg(short, long)]
        ticket: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum ListCommands {
    /// Create a new shared grocery list
    Create {
        /// Title of the list
        title: String,
    },
    /// List all local grocery lists
    Ls,
    /// Join a list using a DocTicket
    Join {
        /// The DocTicket string
        ticket: String,
    },
    /// Generate a sharing DocTicket and display ASCII QR code
    Share {
        /// Target list ID or prefix
        list: Option<String>,
    },
    /// Rotate list capability: migrate active items to a new namespace and revoke old peers
    Rotate {
        /// Target list ID or prefix
        list: Option<String>,
    },
    /// Remove/drop a list from local storage
    Rm {
        /// Target list ID or prefix
        list: String,
    },
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Json,
    Text,
}

#[derive(Serialize, Deserialize)]
pub struct ExportItemDto {
    pub id: String,
    pub name: String,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    pub category: Option<String>,
    pub note: Option<String>,
    pub checked: bool,
    pub position: String,
}

#[derive(Serialize, Deserialize)]
pub struct ExportListDto {
    pub title: String,
    pub schema_version: u32,
    pub created_by: String,
    pub items: Vec<ExportItemDto>,
}

pub fn resolve_data_dir(cli_dir: Option<PathBuf>) -> PathBuf {
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
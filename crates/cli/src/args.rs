//! CLI command grammar and argument definitions for Groshe.

use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

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

    /// Manage shared households and multi-list grouping (create, join, share, show, add-list)
    Household {
        #[command(subcommand)]
        action: HouseholdCommands,
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
        /// Optional path to item thumbnail photo (<256KB)
        #[arg(short, long)]
        photo: Option<PathBuf>,
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

    /// Reorder an item in the list
    Pos {
        /// Item ID or unique UUID prefix
        item: String,
        /// Place item immediately before this item ID
        #[arg(long, conflicts_with_all = &["after", "top", "bottom"])]
        before: Option<String>,
        /// Place item immediately after this item ID
        #[arg(long, conflicts_with_all = &["before", "top", "bottom"])]
        after: Option<String>,
        /// Move item to the very top of the list
        #[arg(long, conflicts_with_all = &["before", "after", "bottom"])]
        top: bool,
        /// Move item to the very bottom of the list
        #[arg(long, conflicts_with_all = &["before", "after", "top"])]
        bottom: bool,
        /// Target list ID or prefix
        #[arg(short, long)]
        list: Option<String>,
    },

    /// Remove an item from a list (tombstone)
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

    /// Export or inspect an attached item photo blob
    Photo {
        /// Item ID or unique UUID prefix
        item: String,
        /// Output destination path for the photo blob
        #[arg(short, long)]
        output: PathBuf,
        /// Target list ID or prefix
        #[arg(short, long)]
        list: Option<String>,
    },

    /// Export a list as human-readable JSON or Markdown
    Export {
        /// Output format (json, markdown, or text)
        #[arg(short, long, default_value = "markdown")]
        format: ExportFormat,
        /// Optional destination file path (writes to stdout if omitted)
        #[arg(short, long)]
        output: Option<PathBuf>,
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
        /// Host the Web Companion on the specified address (defaults to 127.0.0.1:8080 if passed without value)
        #[arg(long, num_args = 0..=1, default_missing_value = "127.0.0.1:8080")]
        web: Option<String>,
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


#[derive(Subcommand, Debug)]
pub enum HouseholdCommands {
    /// Create a new shared household
    Create {
        /// Name of the household (e.g. "Smith Family")
        name: String,
    },
    /// Join an existing household using a DocTicket and auto-import all household lists
    Join {
        /// The Household DocTicket string
        ticket: String,
    },
    /// Generate a sharing DocTicket and ASCII QR code for a household
    Share {
        /// Target household ID or prefix (defaults to only household if single)
        #[arg(long)]
        household: Option<String>,
    },
    /// Show household members and shared lists (and reconcile any new lists)
    Show {
        /// Target household ID or prefix (defaults to only household if single)
        #[arg(long)]
        household: Option<String>,
    },
    /// Register an existing local list inside a household
    AddList {
        /// Target list ID or prefix (defaults to only list if single)
        #[arg(short, long)]
        list: Option<String>,
        /// Target household ID or prefix (defaults to only household if single)
        #[arg(long)]
        household: Option<String>,
    },
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Json,
    Markdown,
    Text,
}

pub type ExportItemDto = groshe_core::ExportItem;
pub type ExportListDto = groshe_core::ExportList;

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

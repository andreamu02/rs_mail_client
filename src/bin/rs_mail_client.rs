use anyhow::{Result, anyhow};
use clap::{Parser, Subcommand};

use rs_mail_client::auth::{token_manager::TokenManager, token_store};
use rs_mail_client::config::{load_config, resolve_db_path};
use rs_mail_client::daemon::{DaemonConfig, run_daemon};
use rs_mail_client::mail::imap_client::ImapClient;
use rs_mail_client::store::sqlite::SqliteRepo;
use rs_mail_client::terminal::run_tui;

#[derive(Parser)]
#[command(name = "rs_mail_client")]
#[command(about = "Rust mail client (daemon + TUI)", long_about = None)]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the TUI (reads from local cache)
    Tui {
        /// Open a specific UID (used by notification action)
        #[arg(long)]
        open: Option<u32>,
    },

    /// Run the daemon: fetch/store/prune/notify
    Daemon {
        #[arg(long, default_value_t = 5)]
        interval: u64,

        #[arg(long, default_value_t = 200)]
        keep: usize,

        /// How many pages (x20) to keep updated in the cache each cycle
        #[arg(long, default_value_t = 3)]
        pages: u32,
    },

    /// Store the OAuth client secret in keyring
    SetClientSecret {
        #[arg(long)]
        client_id: String,
    },
}

fn main() -> Result<()> {
    env_logger::init();

    let cli = Cli::parse();

    match cli.cmd {
        Command::SetClientSecret { client_id } => {
            eprintln!("Paste client secret (end with Ctrl-D):");
            let mut secret = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut secret)?;
            let secret = secret.trim();
            token_store::save_client_secret(&client_id, secret)?;
            println!("Saved client secret for client_id {}", client_id);
            Ok(())
        }

        Command::Tui { open } => {
            let cfg = load_config().map_err(|e| anyhow!("Configuration error: {e}"))?;
            let db_path = resolve_db_path(&cfg)?;
            let repo = SqliteRepo::open(&db_path)?;
            run_tui(&repo, open)
        }

        Command::Daemon {
            interval,
            keep,
            pages,
        } => {
            let cfg = load_config().map_err(|e| anyhow!("Configuration error: {e}"))?;
            let db_path = resolve_db_path(&cfg)?;
            let repo = SqliteRepo::open(&db_path)?;

            let token_mgr = TokenManager::from_config(&cfg)?;
            let imap_server = cfg
                .imap_server
                .clone()
                .unwrap_or_else(|| "imap.gmail.com".to_string());
            let user_email = cfg
                .user_email
                .clone()
                .ok_or_else(|| anyhow!("user_email not set in config"))?;

            let imap = ImapClient::new(imap_server, user_email);

            run_daemon(
                &repo,
                &imap,
                &token_mgr,
                DaemonConfig {
                    interval_secs: interval,
                    keep_recent: keep,
                    pages_to_fetch: pages,
                },
            )
        }
    }
}

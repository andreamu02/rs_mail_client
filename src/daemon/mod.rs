pub mod notifier;

use anyhow::Result;
use std::{
    sync::Arc,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};

use crate::auth::token_manager::TokenManager;
use crate::mail::imap_client::ImapClient;
use crate::store::repo::MailRepository;

use crate::daemon::notifier::Notifier;

pub struct DaemonConfig {
    pub interval_secs: u64,
    pub keep_recent: usize,
    pub pages_to_fetch: u32, // how many pages of 20 to cache
}

pub fn run_daemon(
    repo: &dyn MailRepository,
    imap: &ImapClient,
    token_mgr: &TokenManager,
    cfg: DaemonConfig,
) -> Result<()> {
    let running = Arc::new(AtomicBool::new(true));
    let r2 = running.clone();
    ctrlc::set_handler(move || {
        r2.store(false, Ordering::SeqCst);
    })?;

    let notifier = Notifier::new()?;

    while running.load(Ordering::SeqCst) {
        let access = match token_mgr.get_access_token() {
            Ok(t) => t,
            Err(e) => {
                eprintln!("Token error: {e}");
                thread::sleep(Duration::from_secs(cfg.interval_secs));
                continue;
            }
        };

        // fetch N pages (page 0 newest)
        let mut all_summaries = Vec::new();

        for p in 0..cfg.pages_to_fetch {
            match imap.fetch_page(&access, p, 20) {
                Ok(mut page_items) => {
                    if page_items.is_empty() {
                        break;
                    }
                    all_summaries.append(&mut page_items);
                }
                Err(e) => {
                    eprintln!("IMAP fetch_page error: {e}");
                    break;
                }
            }
        }

        if !all_summaries.is_empty() {
            // Upsert summaries
            repo.upsert_summaries(&all_summaries)?;

            // Upsert bodies for the same items (so TUI can read)
            for s in &all_summaries {
                // If body already exists, skip fetch
                if repo.get_body(s.id)?.is_some() {
                    continue;
                }
                if let Ok(b) = imap.fetch_body(&access, s.id) {
                    let _ = repo.upsert_body(&b);
                }
            }

            // prune
            repo.prune_keep_recent(cfg.keep_recent)?;

            // notify new emails: compare max UID to last_seen_uid
            let last_seen = repo.get_meta_i64("last_seen_uid")?.unwrap_or(0) as u32;
            let max_uid = all_summaries.iter().map(|x| x.id).max().unwrap_or(0);
            let mut new_items: Vec<_> = all_summaries
                .iter()
                .filter(|x| x.id > last_seen)
                .cloned()
                .collect();
            new_items.sort_by(|a, b| b.id.cmp(&a.id)); // newest first

            for it in new_items {
                notifier.notify_email(&it)?;
            }

            repo.set_meta_i64("last_seen_uid", max_uid as i64)?;
        }

        thread::sleep(Duration::from_secs(cfg.interval_secs));
    }

    Ok(())
}

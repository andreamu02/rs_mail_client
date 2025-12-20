// src/daemon/mod.rs
pub mod notifier;

use anyhow::{Result, anyhow};
use imap::extensions::idle::WaitOutcome;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::thread;
use std::time::{Duration, Instant};

use crate::auth::token_manager::TokenManager;
use crate::daemon::notifier::Notifier;
use crate::ipc::{Request, Response};
use crate::mail::imap_client::ImapClient;
use crate::store::repo::MailRepository;

#[cfg(unix)]
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
#[cfg(unix)]
use std::path::PathBuf;

pub struct DaemonConfig {
    pub interval_secs: u64,  // fallback poll interval
    pub keep_recent: usize,  // db prune
    pub pages_to_fetch: u32, // how many pages of 20 to cache each cycle
}

pub fn run_daemon(
    repo: &dyn MailRepository,
    imap: &ImapClient,
    token_mgr: &TokenManager,
    cfg: DaemonConfig,
) -> Result<()> {
    let running = Arc::new(AtomicBool::new(true));
    {
        let r = running.clone();
        ctrlc::set_handler(move || {
            r.store(false, Ordering::SeqCst);
        })?;
    }

    // Single-instance + IPC server (Unix only)
    #[cfg(unix)]
    let (listener, sock_path) = match setup_ipc_server() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            return Ok(());
        }
    };

    let notifier = Notifier::new()?;

    // IDLE wake channel
    let (idle_tx, idle_rx) = mpsc::channel::<()>();

    // Spawn IDLE watcher thread (best-effort)
    {
        let imap_owned = (*imap).clone();
        let token_owned = (*token_mgr).clone();
        let running2 = running.clone();

        thread::spawn(move || {
            idle_watch_loop(imap_owned, token_owned, running2, idle_tx);
        });
    }

    // Main loop:
    // - service IPC continuously
    // - run poll cycle on schedule OR immediately when IDLE says "mailbox changed"
    let mut next_run = Instant::now();

    while running.load(Ordering::SeqCst) {
        // IPC
        #[cfg(unix)]
        drain_ipc(&listener, repo, imap, token_mgr, &cfg);

        // If IDLE fired, run immediately (drain all queued events)
        let mut idle_fired = false;
        while idle_rx.try_recv().is_ok() {
            idle_fired = true;
        }
        if idle_fired {
            next_run = Instant::now();
        }

        // Scheduled cycle
        let now = Instant::now();
        if now >= next_run {
            if let Err(e) = do_poll_cycle(repo, imap, token_mgr, &cfg, &notifier) {
                eprintln!("Daemon cycle error: {e}");
            }
            next_run = now + Duration::from_secs(cfg.interval_secs.max(5)); // keep a sane fallback
        }

        thread::sleep(Duration::from_millis(150));
    }

    // Cleanup socket on exit
    #[cfg(unix)]
    {
        let _ = std::fs::remove_file(&sock_path);
    }

    Ok(())
}

/// IMAP IDLE watcher.
/// When it detects mailbox change, it sends a wake signal to the daemon loop.
fn idle_watch_loop(
    imap: ImapClient,
    token_mgr: TokenManager,
    running: Arc<AtomicBool>,
    tx: mpsc::Sender<()>,
) {
    // We intentionally keep this “forever loop” resilient:
    // any error -> short sleep -> reconnect.
    while running.load(Ordering::SeqCst) {
        let access = match token_mgr.get_access_token() {
            Ok(t) => t,
            Err(e) => {
                eprintln!("IDLE: token error: {e}");
                sleep_small(&running);
                continue;
            }
        };

        let mut session = match imap.connect_authenticated(&access) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("IDLE: connect/auth error: {e}");
                sleep_small(&running);
                continue;
            }
        };

        if let Err(e) = session.select("INBOX") {
            eprintln!("IDLE: select INBOX error: {e}");
            let _ = session.logout();
            sleep_small(&running);
            continue;
        }

        // Loop inside a connected session
        while running.load(Ordering::SeqCst) {
            // IMPORTANT: some servers want you to periodically leave IDLE.
            // We wait with a timeout and re-enter.
            match session.idle() {
                Ok(idle) => match idle.wait_with_timeout(Duration::from_secs(60)) {
                    Ok(WaitOutcome::MailboxChanged) => {
                        let _ = tx.send(());
                    }
                    Ok(WaitOutcome::TimedOut) => {
                        // just loop again so we can check `running` and keep the connection fresh
                    }
                    Err(e) => {
                        eprintln!("IDLE: wait error: {e}");
                        break; // break inner loop -> reconnect
                    }
                },
                Err(e) => {
                    eprintln!("IDLE: idle() error: {e}");
                    break; // break inner loop -> reconnect
                }
            }
        }

        let _ = session.logout();
        sleep_small(&running);
    }
}

fn sleep_small(running: &Arc<AtomicBool>) {
    // sleep in short chunks so shutdown is responsive
    for _ in 0..10 {
        if !running.load(Ordering::SeqCst) {
            break;
        }
        thread::sleep(Duration::from_millis(200));
    }
}

fn do_poll_cycle(
    repo: &dyn MailRepository,
    imap: &ImapClient,
    token_mgr: &TokenManager,
    cfg: &DaemonConfig,
    notifier: &Notifier,
) -> Result<()> {
    let access = token_mgr.get_access_token()?;

    // Fetch N pages (page 0 newest) and merge
    let mut all_summaries = Vec::new();
    for p in 0..cfg.pages_to_fetch {
        match imap.fetch_page(&access, p, 20) {
            Ok(mut items) => {
                if items.is_empty() {
                    break;
                }
                all_summaries.append(&mut items);
            }
            Err(e) => return Err(anyhow!("IMAP fetch_page error: {e}")),
        }
    }

    if all_summaries.is_empty() {
        return Ok(());
    }

    // Dedup by UID (critical to avoid duplicate notifications)
    all_summaries.sort_by(|a, b| b.id.cmp(&a.id));
    all_summaries.dedup_by(|a, b| a.id == b.id);

    // Store summaries
    repo.upsert_summaries(&all_summaries)?;

    // Store bodies (so TUI can read them)
    for s in &all_summaries {
        if repo.get_body(s.id)?.is_some() {
            continue;
        }
        if let Ok(b) = imap.fetch_body(&access, s.id) {
            let _ = repo.upsert_body(&b);
        }
    }

    // Prune store
    repo.prune_keep_recent(cfg.keep_recent)?;

    // Notifications: only notify items newer than last_seen_uid
    let max_uid = all_summaries.iter().map(|x| x.id).max().unwrap_or(0);
    let last_seen = repo.get_meta_i64("last_seen_uid")?.unwrap_or(0) as u32;

    // On first run, don't spam: just set marker.
    if last_seen == 0 {
        repo.set_meta_i64("last_seen_uid", max_uid as i64)?;
        return Ok(());
    }

    let mut new_items: Vec<_> = all_summaries
        .iter()
        .filter(|x| x.id > last_seen)
        .cloned()
        .collect();

    new_items.sort_by(|a, b| b.id.cmp(&a.id));
    new_items.dedup_by(|a, b| a.id == b.id);

    for it in new_items {
        if let Err(e) = notifier.notify_email(&it) {
            eprintln!("Notify error for UID {}: {e}", it.id);
        }
    }

    repo.set_meta_i64("last_seen_uid", max_uid as i64)?;
    Ok(())
}

fn handle_ipc_request(
    req: Request,
    repo: &dyn MailRepository,
    imap: &ImapClient,
    token_mgr: &TokenManager,
    cfg: &DaemonConfig,
) -> Response {
    match req {
        Request::Ping => Response {
            ok: true,
            message: Some("pong".into()),
        },

        Request::SyncPage { page, page_size } => {
            let access = match token_mgr.get_access_token() {
                Ok(t) => t,
                Err(e) => {
                    return Response {
                        ok: false,
                        message: Some(format!("token error: {e}")),
                    };
                }
            };

            let mut items = match imap.fetch_page(&access, page, page_size) {
                Ok(v) => v,
                Err(e) => {
                    return Response {
                        ok: false,
                        message: Some(format!("imap error: {e}")),
                    };
                }
            };

            items.sort_by(|a, b| b.id.cmp(&a.id));
            items.dedup_by(|a, b| a.id == b.id);

            if let Err(e) = repo.upsert_summaries(&items) {
                return Response {
                    ok: false,
                    message: Some(format!("store error: {e}")),
                };
            }

            // Fetch/store bodies for these items so TUI can read right away
            for s in &items {
                if repo.get_body(s.id).ok().flatten().is_some() {
                    continue;
                }
                if let Ok(b) = imap.fetch_body(&access, s.id) {
                    let _ = repo.upsert_body(&b);
                }
            }

            let _ = repo.prune_keep_recent(cfg.keep_recent);

            Response {
                ok: true,
                message: Some(format!("synced page {page}")),
            }
        }
    }
}

#[cfg(unix)]
fn setup_ipc_server() -> Result<(UnixListener, PathBuf)> {
    let sock_path = crate::ipc::socket_path()?;

    // If socket exists and we can connect => daemon already running
    if sock_path.exists() {
        if UnixStream::connect(&sock_path).is_ok() {
            return Err(anyhow!(
                "Daemon already running (socket {}). Exiting.",
                sock_path.display()
            ));
        }
        // stale socket
        let _ = std::fs::remove_file(&sock_path);
    }

    let listener = UnixListener::bind(&sock_path)?;
    listener.set_nonblocking(true)?;
    Ok((listener, sock_path))
}

#[cfg(unix)]
fn drain_ipc(
    listener: &UnixListener,
    repo: &dyn MailRepository,
    imap: &ImapClient,
    token_mgr: &TokenManager,
    cfg: &DaemonConfig,
) {
    loop {
        match listener.accept() {
            Ok((mut stream, _addr)) => {
                if let Ok(req) = read_len_prefixed_json::<Request>(&mut stream) {
                    let resp = handle_ipc_request(req, repo, imap, token_mgr, cfg);
                    let _ = write_len_prefixed_json(&mut stream, &resp);
                } else {
                    let resp = Response {
                        ok: false,
                        message: Some("bad request".into()),
                    };
                    let _ = write_len_prefixed_json(&mut stream, &resp);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(_) => break,
        }
    }
}

#[cfg(unix)]
fn read_len_prefixed_json<T: serde::de::DeserializeOwned>(stream: &mut UnixStream) -> Result<T> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let n = u32::from_be_bytes(len_buf) as usize;

    // basic sanity limit (1MB)
    if n > 1024 * 1024 {
        return Err(anyhow!("IPC message too large"));
    }

    let mut buf = vec![0u8; n];
    stream.read_exact(&mut buf)?;
    Ok(serde_json::from_slice(&buf)?)
}

#[cfg(unix)]
fn write_len_prefixed_json<T: serde::Serialize>(stream: &mut UnixStream, value: &T) -> Result<()> {
    let data = serde_json::to_vec(value)?;
    stream.write_all(&(data.len() as u32).to_be_bytes())?;
    stream.write_all(&data)?;
    stream.flush()?;
    Ok(())
}

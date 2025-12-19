mod config;
mod decoders;
mod imapsession;
mod oauth;
mod terminal;
mod token_store;
mod tokens_file;

use anyhow::{Result, anyhow};
use std::io::Read;
use std::time::{SystemTime, UNIX_EPOCH};
use terminal::run_terminal;

fn main() -> Result<()> {
    env_logger::init();

    // CLI utility: set client secret into keyring:
    // Usage: rs_mail_client --set-client-secret <client_id>
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() >= 2 && args[1] == "--set-client-secret" {
        if args.len() < 3 {
            eprintln!("Usage: --set-client-secret <client_id>");
            return Ok(());
        }
        let client_id = args[2].clone();
        eprintln!("Paste client secret (end with Ctrl-D):");
        let mut secret = String::new();
        std::io::stdin().read_to_string(&mut secret)?;
        let secret = secret.trim();
        token_store::save_client_secret(&client_id, secret)?;
        println!("Saved client secret for client_id {}", client_id);
        return Ok(());
    }

    // Try to load config (creates a template if missing)
    let cfg = match config::load_config() {
        Ok(c) => c,
        Err(e) => return Err(anyhow!("Configuration error: {}", e)),
    };

    let client_id = cfg.client_id.clone();
    let redirect = cfg
        .redirect_uri
        .clone()
        .unwrap_or_else(|| "http://127.0.0.1:8080/callback".to_string());
    let imap_server = cfg
        .imap_server
        .clone()
        .unwrap_or_else(|| "imap.gmail.com".to_string());
    let user_email = cfg
        .user_email
        .clone()
        .ok_or_else(|| anyhow!("user_email not set in config"))?;

    // Try to load secrets from keyring (client secret optional)
    let client_secret = token_store::load_client_secret(&client_id)?
        .or_else(|| std::env::var("OAUTH_CLIENT_SECRET").ok());

    // Try to load refresh token from keyring
    let refresh_token = token_store::load_refresh_token(&user_email)?;

    // Try to load cached access token + expiry
    let cached = tokens_file::load_tokens()?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;

    // Decide what to do:
    // 1) If cached access_token exists and not expired -> use it.
    // 2) Else if refresh_token exists -> refresh.
    // 3) Else -> interactive PKCE flow.
    let tokens = if let Some(tf) = cached {
        if let (Some(at), Some(exp)) = (tf.access_token, tf.expires_at_epoch) {
            if now < exp {
                println!("Using cached access token (not expired).");
                oauth::Tokens {
                    access_token: at,
                    refresh_token: None,
                    expires_in: Some((exp - now) as u64),
                }
            } else {
                // expired: try refresh if possible
                if let Some(rt) = refresh_token.clone() {
                    println!("Cached token expired; refreshing with refresh token...");
                    match oauth::refresh_access_token(&client_id, client_secret.as_deref(), &rt) {
                        Ok(t) => t,
                        Err(e) => {
                            eprintln!("Refresh failed: {}, falling back to interactive auth", e);
                            oauth::perform_pkce_flow(
                                &client_id,
                                client_secret.as_deref(),
                                &redirect,
                                "https://mail.google.com/",
                                &user_email,
                            )?
                        }
                    }
                } else {
                    println!(
                        "Cached token expired and no refresh token; running interactive PKCE auth flow..."
                    );
                    oauth::perform_pkce_flow(
                        &client_id,
                        client_secret.as_deref(),
                        &redirect,
                        "https://mail.google.com/",
                        &user_email,
                    )?
                }
            }
        } else {
            // no cached token, fallback as below
            if let Some(rt) = refresh_token.clone() {
                println!("No cached access token; refreshing with refresh token...");
                match oauth::refresh_access_token(&client_id, client_secret.as_deref(), &rt) {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!("Refresh failed: {}, falling back to interactive auth", e);
                        oauth::perform_pkce_flow(
                            &client_id,
                            client_secret.as_deref(),
                            &redirect,
                            "https://mail.google.com/",
                            &user_email,
                        )?
                    }
                }
            } else {
                println!(
                    "No cached access token or refresh token; running interactive PKCE auth flow..."
                );
                oauth::perform_pkce_flow(
                    &client_id,
                    client_secret.as_deref(),
                    &redirect,
                    "https://mail.google.com/",
                    &user_email,
                )?
            }
        }
    } else {
        // no cached file
        if let Some(rt) = refresh_token.clone() {
            println!("No cached tokens; refreshing with refresh token...");
            match oauth::refresh_access_token(&client_id, client_secret.as_deref(), &rt) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("Refresh failed: {}, falling back to interactive auth", e);
                    oauth::perform_pkce_flow(
                        &client_id,
                        client_secret.as_deref(),
                        &redirect,
                        "https://mail.google.com/",
                        &user_email,
                    )?
                }
            }
        } else {
            println!(
                "No cached tokens and no refresh token; running interactive PKCE auth flow..."
            );
            oauth::perform_pkce_flow(
                &client_id,
                client_secret.as_deref(),
                &redirect,
                "https://mail.google.com/",
                &user_email,
            )?
        }
    };

    // Persist refresh token into keyring (best-effort; don't fail the flow if this fails)
    if let Some(ref_tok) = &tokens.refresh_token {
        if let Err(e) = token_store::save_refresh_token(&user_email, ref_tok) {
            eprintln!("Warning: couldn't save refresh token to keyring: {}", e);
        } else {
            println!("Saved refresh token into keyring for user {}", user_email);
        }
    }

    // Persist access token + expiry (non-secret metadata) into tokens file if available
    if let Some(expires_in) = tokens.expires_in {
        let now_s = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_secs();
        let expiry_epoch = (now_s + expires_in) as i64;
        if let Err(e) = tokens_file::save_tokens(Some(&tokens.access_token), Some(expiry_epoch)) {
            eprintln!("Warning: couldn't save tokens metadata: {}", e);
        } else {
            println!("Saved token expiry epoch {}", expiry_epoch);
        }
    } else {
        // clear stored token metadata if provider didn't return expires_in
        let _ = tokens_file::save_tokens(None, None);
    }

    // Use the access token to authenticate to IMAP via XOAUTH2
    imapsession::list_recent_subjects(&imap_server, &user_email, &tokens.access_token)?;

    let _ = run_terminal();
    Ok(())
}

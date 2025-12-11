use anyhow::{Result, anyhow};
use keyring::{Entry, Error as KeyringError};

const SERVICE: &str = "rs_mail_client";

/// Save a refresh token into the OS keyring for the given username (email)
pub fn save_refresh_token(username: &str, refresh_token: &str) -> Result<()> {
    let entry = Entry::new(SERVICE, username);
    entry?
        .set_password(refresh_token)
        .map_err(|e| anyhow!(e.to_string()))?;
    Ok(())
}

/// Load a refresh token from the keyring for the given username (email)
pub fn load_refresh_token(username: &str) -> Result<Option<String>> {
    let entry = Entry::new(SERVICE, username);
    match entry?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(KeyringError::NoEntry) => Ok(None),
        Err(e) => Err(anyhow!(e.to_string())),
    }
}

/// Save a client secret into the keyring, keyed by client_id
pub fn save_client_secret(client_id: &str, client_secret: &str) -> Result<()> {
    let entry = Entry::new(SERVICE, client_id);
    entry?
        .set_password(client_secret)
        .map_err(|e| anyhow!(e.to_string()))?;
    Ok(())
}

/// Load client secret from keyring by client_id
pub fn load_client_secret(client_id: &str) -> Result<Option<String>> {
    let entry = Entry::new(SERVICE, client_id);
    match entry?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(KeyringError::NoEntry) => Ok(None),
        Err(e) => Err(anyhow!(e.to_string())),
    }
}

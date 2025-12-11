use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Non-secret tokens metadata stored in ~/.config/rs_mail_client/tokens.json
#[derive(Debug, Serialize, Deserialize)]
pub struct TokensFile {
    pub access_token: Option<String>,
    pub expires_at_epoch: Option<i64>, // epoch seconds
}

fn config_dir() -> Result<PathBuf> {
    Ok(dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("no config dir available"))?
        .join("rs_mail_client"))
}

fn tokens_path() -> Result<PathBuf> {
    let mut p = config_dir()?;
    fs::create_dir_all(&p)?;
    p.push("tokens.json");
    Ok(p)
}

/// Save access_token (non-secret) and expiry epoch
pub fn save_tokens(access_token: Option<&str>, expires_at_epoch: Option<i64>) -> Result<()> {
    let p = tokens_path()?;
    let tf = TokensFile {
        access_token: access_token.map(|s| s.to_string()),
        expires_at_epoch,
    };
    let s = serde_json::to_string_pretty(&tf)?;
    fs::write(&p, s)?;
    Ok(())
}

/// Load tokens file if present
pub fn load_tokens() -> Result<Option<TokensFile>> {
    let p = tokens_path()?;
    if !p.exists() {
        return Ok(None);
    }
    let s = fs::read_to_string(&p)?;
    let tf: TokensFile = serde_json::from_str(&s)?;
    Ok(Some(tf))
}

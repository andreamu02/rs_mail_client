use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub client_id: String,
    pub imap_server: Option<String>,
    pub user_email: Option<String>,
    pub redirect_uri: Option<String>,
    pub db_path: Option<String>,
}

fn config_dir() -> Result<PathBuf> {
    Ok(dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("no config dir available"))?
        .join("rs_mail_client"))
}

pub fn config_path() -> Result<PathBuf> {
    let mut p = config_dir()?;
    fs::create_dir_all(&p)?;
    p.push("config.toml");
    Ok(p)
}

pub fn default_db_path() -> Result<PathBuf> {
    let mut p = config_dir()?;
    fs::create_dir_all(&p)?;
    p.push("mail.db");
    Ok(p)
}

pub fn load_config() -> Result<Config> {
    let path = config_path()?;
    if !path.exists() {
        // create a template config for users to edit
        let sample = Config {
            client_id: "YOUR_CLIENT_ID.apps.googleusercontent.com".to_string(),
            imap_server: Some("imap.gmail.com".to_string()),
            user_email: Some("you@example.com".to_string()),
            redirect_uri: Some("http://127.0.0.1:8080/callback".to_string()),
            db_path: None,
        };
        let tom = toml::to_string_pretty(&sample)?;
        fs::write(&path, tom)?;
        return Err(anyhow::anyhow!(
            "Created template config at {} — edit it and run again",
            path.display()
        ));
    }
    let s = fs::read_to_string(path)?;
    let cfg: Config = toml::from_str(&s)?;
    Ok(cfg)
}

pub fn resolve_db_path(cfg: &Config) -> Result<PathBuf> {
    if let Some(p) = &cfg.db_path {
        Ok(PathBuf::from(p))
    } else {
        default_db_path()
    }
}

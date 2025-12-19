use anyhow::{Result, anyhow};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::auth::{oauth, token_store, tokens_file};
use crate::config::Config;

pub struct TokenManager {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub redirect_uri: String,
    pub user_email: String,
}

impl TokenManager {
    pub fn from_config(cfg: &Config) -> Result<Self> {
        let client_id = cfg.client_id.clone();
        let user_email = cfg
            .user_email
            .clone()
            .ok_or_else(|| anyhow!("user_email not set in config"))?;
        let redirect_uri = cfg
            .redirect_uri
            .clone()
            .unwrap_or_else(|| "http://127.0.0.1:8080/callback".to_string());

        let client_secret = token_store::load_client_secret(&client_id)?
            .or_else(|| std::env::var("OAUTH_CLIENT_SECRET").ok());

        Ok(Self {
            client_id,
            client_secret,
            redirect_uri,
            user_email,
        })
    }

    /// Returns a valid access token; refreshes/PKCE if needed.
    pub fn get_access_token(&self) -> Result<String> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;

        let refresh_token = token_store::load_refresh_token(&self.user_email)?;
        let cached = tokens_file::load_tokens()?;

        // 1) cached & not expired
        if let Some(tf) = cached {
            if let (Some(at), Some(exp)) = (tf.access_token, tf.expires_at_epoch) {
                if now < exp {
                    return Ok(at);
                }
            }
        }

        // 2) refresh token exists
        if let Some(rt) = refresh_token.clone() {
            let t =
                oauth::refresh_access_token(&self.client_id, self.client_secret.as_deref(), &rt)?;
            self.persist_tokens(&t)?;
            if let Some(new_rt) = &t.refresh_token {
                let _ = token_store::save_refresh_token(&self.user_email, new_rt);
            }
            return Ok(t.access_token);
        }

        // 3) interactive PKCE
        let t = oauth::perform_pkce_flow(
            &self.client_id,
            self.client_secret.as_deref(),
            &self.redirect_uri,
            "https://mail.google.com/",
            &self.user_email,
        )?;
        self.persist_tokens(&t)?;
        if let Some(new_rt) = &t.refresh_token {
            let _ = token_store::save_refresh_token(&self.user_email, new_rt);
        }
        Ok(t.access_token)
    }

    fn persist_tokens(&self, t: &oauth::Tokens) -> Result<()> {
        let now_s = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_secs() as i64;

        if let Some(expires_in) = t.expires_in {
            let expiry_epoch = now_s + expires_in as i64;
            let _ = tokens_file::save_tokens(Some(&t.access_token), Some(expiry_epoch));
        } else {
            let _ = tokens_file::save_tokens(Some(&t.access_token), None);
        }
        Ok(())
    }
}

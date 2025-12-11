use anyhow::{Result, anyhow};
use oauth2::TokenResponse;
use oauth2::basic::BasicClient;
use oauth2::reqwest::http_client;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, PkceCodeChallenge, RedirectUrl,
    RefreshToken, Scope, TokenUrl,
};
use std::time::{Duration, Instant};
use tiny_http::{Response, Server};
use url::Url;

use crate::token_store;

/// Tokens returned by the oauth flow (in-memory)
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<u64>,
}

/// Exchange a refresh token for a new access token using the oauth2 crate
pub fn refresh_access_token(
    client_id: &str,
    client_secret: Option<&str>,
    refresh_token: &str,
) -> Result<Tokens> {
    let client_id = ClientId::new(client_id.to_string());
    let client_secret = client_secret.map(|s| ClientSecret::new(s.to_string()));

    let auth_url = AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".to_string())?;
    let token_url = TokenUrl::new("https://oauth2.googleapis.com/token".to_string())?;

    let oauth_client = BasicClient::new(client_id, client_secret, auth_url, Some(token_url));

    let rt = RefreshToken::new(refresh_token.to_string());
    let token = oauth_client
        .exchange_refresh_token(&rt)
        .request(http_client)?;

    let access = token.access_token().secret().to_string();
    let refresh = token.refresh_token().map(|r| r.secret().to_string());
    let expires = token.expires_in().map(|d| d.as_secs());

    Ok(Tokens {
        access_token: access,
        refresh_token: refresh,
        expires_in: expires,
    })
}

/// Perform Authorization Code + PKCE flow. Opens system browser and captures code via tiny server.
pub fn perform_pkce_flow(
    client_id: &str,
    client_secret: Option<&str>,
    redirect_uri: &str,
    scope: &str,
    user_email: &str,
) -> Result<Tokens> {
    let client_id = ClientId::new(client_id.to_string());
    let client_secret = client_secret.map(|s| ClientSecret::new(s.to_string()));

    let auth_url = AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".to_string())?;
    let token_url = TokenUrl::new("https://oauth2.googleapis.com/token".to_string())?;

    let oauth_client = BasicClient::new(client_id, client_secret, auth_url, Some(token_url))
        .set_redirect_uri(RedirectUrl::new(redirect_uri.to_string())?);

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    let (auth_url, _csrf_token) = oauth_client
        .authorize_url(oauth2::CsrfToken::new_random)
        .add_scope(Scope::new(scope.to_string()))
        .set_pkce_challenge(pkce_challenge)
        .url();

    println!("Opening browser to: {}", auth_url.as_str());
    open::that(auth_url.as_str()).map_err(|e| anyhow!(e))?;

    let server = Server::http("127.0.0.1:8080").map_err(|e| anyhow!(e))?;
    let mut code_opt: Option<String> = None;
    let wait_until = Instant::now() + Duration::from_secs(120);

    while Instant::now() < wait_until {
        let Ok(maybe_request) = server.recv_timeout(Duration::from_millis(500)) else {
            continue;
        };
        if let Some(request) = maybe_request {
            let url = format!("http://localhost{}", request.url());
            if let Ok(parsed) = Url::parse(&url) {
                for (k, v) in parsed.query_pairs() {
                    if k == "code" {
                        code_opt = Some(v.into());
                    }
                }
                let _ = request.respond(Response::from_string(
                    "Authorization received. You can close this tab.",
                ));
                break;
            } else {
                let _ = request.respond(Response::from_string("Bad redirect"));
            }
        }
    }

    let code = code_opt.ok_or_else(|| anyhow!("No code received"))?;

    let token = match oauth_client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(pkce_verifier)
        .request(http_client)
    {
        Ok(tok) => tok,
        Err(err) => {
            // print detailed debug so we can see the server body / error
            eprintln!("Token exchange failed: {:#?}", err);
            // also print a short message for the user
            return Err(anyhow!("Token exchange failed: see stderr for details"));
        }
    };

    let access = token.access_token().secret().to_string();
    let refresh = token.refresh_token().map(|r| r.secret().to_string());
    let expires = token.expires_in().map(|d| d.as_secs());

    // store refresh token in keyring (if we have one)
    if let Some(ref_token) = &refresh {
        // best-effort: ignore keyring errors here but print a warning
        if let Err(e) = token_store::save_refresh_token(user_email, ref_token) {
            eprintln!("Warning: could not store refresh token in keyring: {}", e);
        }
    }

    Ok(Tokens {
        access_token: access,
        refresh_token: refresh,
        expires_in: expires,
    })
}

use anyhow::{Result, anyhow};
use oauth2::TokenResponse;
use oauth2::basic::BasicClient;
use oauth2::reqwest::http_client;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, PkceCodeChallenge, RedirectUrl,
    RefreshToken, Scope, TokenUrl,
};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
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

    // Parse redirect_uri so bind address matches exactly
    let redirect = Url::parse(redirect_uri)
        .map_err(|e| anyhow!("Invalid redirect_uri '{redirect_uri}': {e}"))?;

    let host = redirect
        .host_str()
        .ok_or_else(|| anyhow!("redirect_uri missing host: {redirect_uri}"))?;

    let port = redirect
        .port_or_known_default()
        .ok_or_else(|| anyhow!("redirect_uri missing/unknown port: {redirect_uri}"))?;

    // For local loopback flows, prefer binding explicitly to loopback.
    // If redirect host is "localhost" or "127.0.0.1", bind to 127.0.0.1.
    let bind_ip: IpAddr = match host {
        "localhost" | "127.0.0.1" => IpAddr::V4(Ipv4Addr::LOCALHOST),
        // If user put a specific IP, try it.
        other => other.parse::<IpAddr>().map_err(|_| {
            anyhow!("redirect_uri host must be localhost/127.0.0.1 or an IP: {other}")
        })?,
    };

    let bind_addr = SocketAddr::new(bind_ip, port);

    // 1) Start listening FIRST (fixes the race)
    let server = Server::http(bind_addr)
        .map_err(|e| anyhow!("Failed to bind OAuth callback server on {bind_addr}: {e:?}"))?;

    // 2) Configure client
    let oauth_client = BasicClient::new(client_id, client_secret, auth_url, Some(token_url))
        .set_redirect_uri(RedirectUrl::new(redirect_uri.to_string())?);

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    let (auth_url, _csrf_token) = oauth_client
        .authorize_url(oauth2::CsrfToken::new_random)
        .add_scope(Scope::new(scope.to_string()))
        .set_pkce_challenge(pkce_challenge)
        .url();

    println!("Open this URL in your browser:\n{auth_url}");
    // best-effort: don't fail if browser can't be opened
    if let Err(e) = open::that(auth_url.as_str()) {
        eprintln!("Warning: could not open browser automatically: {e}");
    }

    // 3) Wait for callback
    let mut code_opt: Option<String> = None;
    let wait_until = Instant::now() + Duration::from_secs(120);

    while Instant::now() < wait_until {
        let Ok(maybe_request) = server.recv_timeout(Duration::from_millis(500)) else {
            continue;
        };

        let Some(request) = maybe_request else {
            continue;
        };

        // request.url() is a path+query like "/callback?code=...&state=..."
        // Build a full URL using the SAME host/port as redirect_uri.
        let full = format!("http://{}:{}{}", host, port, request.url());

        match Url::parse(&full) {
            Ok(parsed) => {
                for (k, v) in parsed.query_pairs() {
                    if k == "code" {
                        code_opt = Some(v.into_owned());
                    }
                }

                if code_opt.is_some() {
                    let _ = request.respond(Response::from_string(
                        "Authorization received. You can close this tab.",
                    ));
                    break;
                } else {
                    let _ = request.respond(Response::from_string(
                        "No code found in redirect. You can close this tab.",
                    ));
                }
            }
            Err(_) => {
                let _ = request.respond(Response::from_string("Bad redirect"));
            }
        }
    }

    let code = code_opt.ok_or_else(|| anyhow!("No code received within timeout"))?;

    // 4) Exchange code for tokens
    let token = match oauth_client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(pkce_verifier)
        .request(http_client)
    {
        Ok(tok) => tok,
        Err(err) => {
            eprintln!("Token exchange failed: {:#?}", err);
            return Err(anyhow!("Token exchange failed: see stderr for details"));
        }
    };

    let access = token.access_token().secret().to_string();
    let refresh = token.refresh_token().map(|r| r.secret().to_string());
    let expires = token.expires_in().map(|d| d.as_secs());

    if let Some(ref_token) = &refresh
        && let Err(e) = token_store::save_refresh_token(user_email, ref_token)
    {
        eprintln!("Warning: could not store refresh token in keyring: {e}");
    }

    Ok(Tokens {
        access_token: access,
        refresh_token: refresh,
        expires_in: expires,
    })
}

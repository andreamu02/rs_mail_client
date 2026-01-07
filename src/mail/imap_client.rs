use crate::domain::email::{EmailBody, EmailId, EmailSummary};
use crate::mail::decoders::{decode_mime_words, decode_subject, normalize_snippet};
use anyhow::{Result, anyhow};
use base64::{Engine as _, engine::general_purpose};
use mailparse::MailHeaderMap;
use native_tls::TlsConnector;

/// Concrete session type we use everywhere.
pub type ImapSession = imap::Session<native_tls::TlsStream<std::net::TcpStream>>;

/// Build canonical auth string as bytes.
fn build_xoauth2_bytes(user: &str, access_token: &str) -> Vec<u8> {
    let user_field = format!("user={}", user);
    let auth_field = format!("auth=Bearer {}", access_token);
    let auth_string = format!("{}{}{}{}{}", user_field, "\x01", auth_field, "\x01", "\x01");
    auth_string.into_bytes()
}

struct OAuth2Authenticator {
    response: Vec<u8>,
}

impl imap::Authenticator for OAuth2Authenticator {
    type Response = Vec<u8>;
    fn process(&self, _challenge: &[u8]) -> Self::Response {
        self.response.clone()
    }
}

#[derive(Debug, Clone)]
pub struct ImapClient {
    pub server: String,
    pub user: String,
}

impl ImapClient {
    pub fn new(server: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            server: server.into(),
            user: user.into(),
        }
    }

    /// Public helper used by the daemon to keep a session open for IDLE.
    pub fn connect_authenticated(&self, access_token: &str) -> Result<ImapSession> {
        self.connect_and_auth(access_token)
    }

    fn connect_and_auth(&self, access_token: &str) -> Result<ImapSession> {
        let tls = TlsConnector::builder().build()?;
        let mut client = imap::connect((self.server.as_str(), 993), self.server.as_str(), &tls)?;

        let raw_payload = build_xoauth2_bytes(&self.user, access_token);

        // Try RAW first
        let auth_raw = OAuth2Authenticator {
            response: raw_payload.clone(),
        };
        match client.authenticate("XOAUTH2", &auth_raw) {
            Ok(session) => return Ok(session),
            Err((_e, returned_client)) => {
                client = returned_client;
            }
        }

        // Fallback BASE64
        let b64_bytes = general_purpose::STANDARD.encode(&raw_payload).into_bytes();
        let auth_b64 = OAuth2Authenticator {
            response: b64_bytes,
        };

        match client.authenticate("XOAUTH2", &auth_b64) {
            Ok(session) => Ok(session),
            Err((e, _)) => Err(anyhow!("XOAUTH2 failed (raw+base64): {e}")),
        }
    }

    /// Fetch a page of summaries (and also parse snippet from BODY.PEEK[] so it's reliable).
    /// Page 0 = newest, page 1 = next older, etc.
    pub fn fetch_page(
        &self,
        access_token: &str,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<EmailSummary>> {
        let mut session = self.connect_and_auth(access_token)?;
        session.select("INBOX")?;

        // uid_search returns a set in this crate version, convert to Vec and sort
        let mut uids: Vec<u32> = session.uid_search("ALL")?.into_iter().collect();
        if uids.is_empty() {
            let _ = session.logout();
            return Ok(vec![]);
        }
        uids.sort_unstable(); // ascending

        // Compute slice for page
        let total = uids.len() as i64;
        let ps = page_size as i64;
        let p = page as i64;

        let end = total - (p * ps);
        if end <= 0 {
            let _ = session.logout();
            return Ok(vec![]);
        }
        let start = (end - ps).max(0);

        let slice = &uids[start as usize..end as usize];

        // IMPORTANT:
        // Use BODY.PEEK[] (not RFC822) so we can always read bytes via fetch.body().
        let uid_set = slice
            .iter()
            .map(|u| u.to_string())
            .collect::<Vec<_>>()
            .join(",");

        let fetches = session.uid_fetch(uid_set, "(UID ENVELOPE BODY.PEEK[])")?;

        let mut out = Vec::new();

        for f in fetches.iter() {
            let uid = f
                .uid
                .ok_or_else(|| anyhow!("missing UID in fetch response"))?;

            let env = f.envelope();
            let subject = env
                .and_then(|e| e.subject)
                .map(decode_subject)
                .unwrap_or_else(|| "(no subject)".to_string());

            // --- FROM name (prefer display-name; fallback to mailbox@host) ---
            let from_name = env
                .and_then(|e| e.from.as_ref())
                .and_then(|list| list.first())
                .map(|addr| {
                    if let Some(name) = addr.name.as_ref() {
                        let n = decode_mime_words(name);
                        if !n.trim().is_empty() {
                            return n;
                        }
                    }

                    let mb = addr
                        .mailbox
                        .as_ref()
                        .map(|b| String::from_utf8_lossy(b).to_string());
                    let host = addr
                        .host
                        .as_ref()
                        .map(|b| String::from_utf8_lossy(b).to_string());
                    match (mb, host) {
                        (Some(m), Some(h)) => format!("{m}@{h}"),
                        (Some(m), None) => m,
                        _ => "(unknown)".to_string(),
                    }
                })
                .unwrap_or_else(|| "(unknown)".to_string());

            // --- BODY bytes (retry once if missing) ---
            let raw: Vec<u8> = if let Some(b) = f.body() {
                b.to_vec()
            } else {
                eprintln!("WARN: UID {uid} missing body on first fetch; retrying once");
                let retry = session.uid_fetch(uid.to_string(), "(UID BODY.PEEK[])")?;
                retry
                    .iter()
                    .next()
                    .and_then(|rf| rf.body())
                    .map(|b| b.to_vec())
                    .unwrap_or_default()
            };

            let (body_text, date_epoch) = extract_best_effort_body_and_date(&raw);
            let snippet = normalize_snippet(&body_text, 140);

            out.push(EmailSummary {
                id: uid as EmailId,
                from_name,
                subject,
                snippet,
                date_epoch,
            });
        }

        let _ = session.logout();

        // newest first + dedup safety
        out.sort_by(|a, b| b.id.cmp(&a.id));
        out.dedup_by(|a, b| a.id == b.id);

        Ok(out)
    }

    /// Fetch full body for a UID.
    pub fn fetch_body(&self, access_token: &str, id: EmailId) -> Result<EmailBody> {
        let mut session = self.connect_and_auth(access_token)?;
        session.select("INBOX")?;

        let uid = id;

        let fetches = session.uid_fetch(uid.to_string(), "(UID BODY.PEEK[])")?;
        let raw: Vec<u8> = if let Some(b) = fetches.iter().next().and_then(|f| f.body()) {
            b.to_vec()
        } else {
            eprintln!("WARN: UID {uid} missing body on first fetch; retrying once");
            let retry = session.uid_fetch(uid.to_string(), "(UID BODY.PEEK[])")?;
            retry
                .iter()
                .next()
                .and_then(|rf| rf.body())
                .map(|b| b.to_vec())
                .ok_or_else(|| anyhow!("UID {uid}: missing body even after retry"))?
        };

        let (body_text, _date_epoch) = extract_best_effort_body_and_date(&raw);

        let _ = session.logout();

        Ok(EmailBody {
            id,
            body: body_text,
        })
    }

    pub fn fetch_raw(&self, access_token: &str, id: EmailId) -> Result<Vec<u8>> {
        let mut session = self.connect_and_auth(access_token)?;
        session.select("INBOX")?;

        let uid = id;

        let fetches = session.uid_fetch(uid.to_string(), "(UID BODY.PEEK[])")?;
        let raw: Vec<u8> = if let Some(b) = fetches.iter().next().and_then(|f| f.body()) {
            b.to_vec()
        } else {
            eprintln!("WARN: UID {uid} missing body on first fetch; retrying once");
            let retry = session.uid_fetch(uid.to_string(), "(UID BODY.PEEK[])")?;
            retry
                .iter()
                .next()
                .and_then(|rf| rf.body())
                .map(|b| b.to_vec())
                .ok_or_else(|| anyhow!("UID {uid}: missing raw even after retry"))?
        };

        let _ = session.logout();
        Ok(raw)
    }
}

fn extract_best_effort_body_and_date(raw_rfc822: &[u8]) -> (String, i64) {
    match mailparse::parse_mail(raw_rfc822) {
        Ok(parsed) => {
            let date_epoch = parsed
                .headers
                .get_first_value("Date")
                .and_then(|d| mailparse::dateparse(&d).ok())
                .unwrap_or(0);

            let body = extract_text_part(&parsed).unwrap_or_else(|| {
                parsed
                    .get_body()
                    .unwrap_or_else(|_| String::from_utf8_lossy(raw_rfc822).into_owned())
            });

            (body, date_epoch)
        }
        Err(_) => (String::from_utf8_lossy(raw_rfc822).into_owned(), 0),
    }
}

fn extract_text_part(p: &mailparse::ParsedMail) -> Option<String> {
    let mime = p.ctype.mimetype.to_ascii_lowercase();
    if mime == "text/plain" {
        return p.get_body().ok();
    }

    for sp in &p.subparts {
        if let Some(t) = extract_text_part(sp) {
            return Some(t);
        }
    }

    if mime == "text/html"
        && let Ok(html) = p.get_body()
    {
        return Some(html_to_text(&html));
    }

    None
}

fn html_to_text(html: &str) -> String {
    match html2text::from_read(html.as_bytes(), 90) {
        Ok(s) => s,
        Err(_) => strip_html_minimal(html),
    }
}

fn strip_html_minimal(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

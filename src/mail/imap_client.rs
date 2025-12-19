use anyhow::{Result, anyhow};
use base64::{Engine as _, engine::general_purpose};
use native_tls::TlsConnector;

use crate::domain::email::{EmailBody, EmailId, EmailSummary};
use crate::mail::decoders::{decode_subject, normalize_snippet};

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

    fn connect_and_auth(
        &self,
        access_token: &str,
    ) -> Result<imap::Session<native_tls::TlsStream<std::net::TcpStream>>> {
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

    /// Fetch a page of summaries (and bodies too, because we want snippet reliably).
    /// Page 0 = newest, page 1 = next older, etc.
    pub fn fetch_page(
        &self,
        access_token: &str,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<EmailSummary>> {
        let mut session = self.connect_and_auth(access_token)?;
        session.select("INBOX")?;

        // Grab all UIDs (simple + reliable for paging).
        // NOTE: On huge mailboxes, you can optimize later.
        let mut uids = session.uid_search("ALL")?;
        if uids.is_empty() {
            session.logout()?;
            return Ok(vec![]);
        }
        uids.sort_unstable(); // ascending

        let total = uids.len() as i64;
        let ps = page_size as i64;
        let p = page as i64;

        let end = total - (p * ps);
        let start = (end - ps).max(0);

        if end <= 0 || start >= end {
            session.logout()?;
            return Ok(vec![]);
        }

        let slice = &uids[start as usize..end as usize];
        // for display we want newest first
        let mut page_uids: Vec<u32> = slice.iter().copied().collect();
        page_uids.sort_unstable_by(|a, b| b.cmp(a));

        let uid_set = page_uids
            .iter()
            .map(|u| u.to_string())
            .collect::<Vec<_>>()
            .join(",");

        // Fetch envelope + full RFC822 so we can parse plain text and snippet.
        let fetches = session.uid_fetch(uid_set, "UID ENVELOPE RFC822")?;

        let mut out = Vec::new();
        for f in fetches.iter() {
            let uid = f.uid.ok_or_else(|| anyhow!("missing UID in fetch"))? as EmailId;
            let subject = f
                .envelope()
                .and_then(|env| env.subject)
                .map(decode_subject)
                .unwrap_or_else(|| "(no subject)".to_string());

            let raw = f.body().ok_or_else(|| anyhow!("missing RFC822 body"))?;
            let (body_text, date_epoch) = extract_best_effort_body_and_date(raw);

            let snippet = normalize_snippet(&body_text, 140);

            out.push(EmailSummary {
                id: uid,
                subject,
                snippet,
                date_epoch,
            });
        }

        // Ensure sorted newest-first (UID desc) even if server returns different order
        out.sort_by(|a, b| b.id.cmp(&a.id));

        session.logout()?;
        Ok(out)
    }

    pub fn fetch_body(&self, access_token: &str, id: EmailId) -> Result<EmailBody> {
        let mut session = self.connect_and_auth(access_token)?;
        session.select("INBOX")?;

        let fetches = session.uid_fetch(id.to_string(), "UID RFC822")?;
        let f = fetches
            .iter()
            .next()
            .ok_or_else(|| anyhow!("email UID {id} not found"))?;
        let raw = f.body().ok_or_else(|| anyhow!("missing RFC822 body"))?;

        let (body_text, _date_epoch) = extract_best_effort_body_and_date(raw);

        session.logout()?;
        Ok(EmailBody {
            id,
            body: body_text,
        })
    }
}

fn extract_best_effort_body_and_date(raw_rfc822: &[u8]) -> (String, i64) {
    // Parse the message and pick the best text/plain part.
    match mailparse::parse_mail(raw_rfc822) {
        Ok(parsed) => {
            let date_epoch = parsed
                .headers
                .get_first_value("Date")
                .and_then(|d| mailparse::dateparse(&d).ok())
                .unwrap_or(0);

            let body = extract_text_part(&parsed).unwrap_or_else(|| {
                // fallback: attempt main body
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

    // Walk subparts
    for sp in &p.subparts {
        if let Some(t) = extract_text_part(sp) {
            return Some(t);
        }
    }

    // fallback to text/html if no plain found
    if mime == "text/html" {
        if let Ok(html) = p.get_body() {
            return Some(strip_html_minimal(&html));
        }
    }

    None
}

fn strip_html_minimal(html: &str) -> String {
    // Simple best-effort: remove tags. You can replace with a real html2text later.
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

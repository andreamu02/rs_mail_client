use crate::decoders::decode_subject;
use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};
use native_tls::TlsConnector;

/// Build canonical auth string as bytes:
fn build_xoauth2_bytes(user: &str, access_token: &str) -> Vec<u8> {
    let user_field = format!("user={}", user);
    let auth_field = format!("auth=Bearer {}", access_token);
    // three fields joined by SOH, plus final SOH so there are two trailing SOHs
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

pub fn list_recent_subjects(imap_server: &str, user_email: &str, access_token: &str) -> Result<()> {
    println!("Connecting to {}:993", imap_server);
    let tls = TlsConnector::builder().build()?;
    let mut client = imap::connect((imap_server, 993), imap_server, &tls)?;

    // Build the canonical payload bytes
    let raw_payload = build_xoauth2_bytes(user_email, access_token);

    // Try RAW first (many imap crate versions expect library to base64-encode)
    println!("Trying XOAUTH2 using RAW response (no base64)...");
    let auth_raw = OAuth2Authenticator {
        response: raw_payload.clone(),
    };
    match client.authenticate("XOAUTH2", &auth_raw) {
        Ok(mut session) => {
            println!("Authenticated with RAW response!");
            dump_inbox(&mut session)?;
            session.logout()?;
            return Ok(());
        }
        Err((e, returned_client)) => {
            eprintln!("RAW attempt failed: {}", e);
            // put the client back so we can try again
            client = returned_client;
        }
    }

    // Try BASE64 (server canonical form) as fallback
    let b64_bytes = general_purpose::STANDARD.encode(&raw_payload).into_bytes();
    println!(
        "Trying XOAUTH2 using BASE64 response (len {})...",
        b64_bytes.len()
    );
    let auth_b64 = OAuth2Authenticator {
        response: b64_bytes.clone(),
    };
    match client.authenticate("XOAUTH2", &auth_b64) {
        Ok(mut session) => {
            println!("Authenticated with BASE64 response!");
            dump_inbox(&mut session)?;
            session.logout()?;
            Ok(())
        }
        Err((e, _returned_client)) => Err(anyhow::anyhow!(
            "Both RAW and BASE64 XOAUTH2 attempts failed; last error: {}",
            e
        )),
    }
}

fn dump_inbox(
    session: &mut imap::Session<native_tls::TlsStream<std::net::TcpStream>>,
) -> Result<()> {
    let mailbox = session.select("INBOX")?;
    println!("INBOX has {} messages", mailbox.exists);

    let start = if mailbox.exists > 10 {
        mailbox.exists - 9
    } else {
        1
    };
    let seq = format!("{}:*", start);
    let messages = session.fetch(seq.as_str(), "ENVELOPE")?;
    for msg in messages.iter() {
        if let Some(env) = msg.envelope() {
            if let Some(subject_b) = env.subject {
                let pretty = decode_subject(subject_b);
                println!("Subject: {}", pretty);
            } else {
                println!("Subject: (none)");
            }
        }
    }
    Ok(())
}

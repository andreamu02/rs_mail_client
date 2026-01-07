use anyhow::Result;

use crate::domain::email::{EmailBody, EmailId, EmailSummary};

pub trait MailRepository: Send + Sync {
    fn upsert_summaries(&self, items: &[EmailSummary]) -> Result<()>;
    fn upsert_body(&self, body: &EmailBody) -> Result<()>;

    fn list_page(&self, page: u32, page_size: u32) -> Result<Vec<EmailSummary>>;
    fn get_body(&self, id: EmailId) -> Result<Option<EmailBody>>;

    // NEW: raw RFC822 bytes (for images / attachments)
    fn upsert_raw(&self, id: EmailId, raw: &[u8]) -> Result<()>;
    fn get_raw(&self, id: EmailId) -> Result<Option<Vec<u8>>>;

    fn prune_keep_recent(&self, keep: usize) -> Result<()>;

    fn get_meta_i64(&self, key: &str) -> Result<Option<i64>>;
    fn set_meta_i64(&self, key: &str, value: i64) -> Result<()>;
}

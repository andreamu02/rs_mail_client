use anyhow::Result;

use crate::domain::email::{EmailBody, EmailId, EmailSummary};

pub trait MailRepository: Send + Sync {
    fn upsert_summaries(&self, items: &[EmailSummary]) -> Result<()>;
    fn upsert_body(&self, body: &EmailBody) -> Result<()>;

    fn list_page(&self, page: u32, page_size: u32) -> Result<Vec<EmailSummary>>;
    fn get_body(&self, id: EmailId) -> Result<Option<EmailBody>>;

    fn prune_keep_recent(&self, keep: usize) -> Result<()>;

    fn get_meta_i64(&self, key: &str) -> Result<Option<i64>>;
    fn set_meta_i64(&self, key: &str, value: i64) -> Result<()>;
}

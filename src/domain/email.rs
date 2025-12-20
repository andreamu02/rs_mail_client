pub type EmailId = u32;

#[derive(Debug, Clone)]
pub struct EmailSummary {
    pub id: EmailId,
    pub from_name: String,
    pub subject: String,
    pub snippet: String,
    pub date_epoch: i64,
}

#[derive(Debug, Clone)]
pub struct EmailBody {
    pub id: EmailId,
    pub body: String,
}

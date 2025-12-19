use anyhow::{Result, anyhow};
use rusqlite::{Connection, params};

use crate::domain::email::{EmailBody, EmailId, EmailSummary};
use crate::store::repo::MailRepository;

pub struct SqliteRepo {
    conn: Connection,
}

impl SqliteRepo {
    pub fn open(path: &std::path::Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        let repo = Self { conn };
        repo.migrate()?;
        Ok(repo)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            PRAGMA journal_mode=WAL;

            CREATE TABLE IF NOT EXISTS emails (
                id          INTEGER PRIMARY KEY,
                subject     TEXT NOT NULL,
                snippet     TEXT NOT NULL,
                date_epoch  INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS bodies (
                id          INTEGER PRIMARY KEY,
                body        TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS meta (
                key   TEXT PRIMARY KEY,
                value INTEGER NOT NULL
            );
            "#,
        )?;
        Ok(())
    }
}

impl MailRepository for SqliteRepo {
    fn upsert_summaries(&self, items: &[EmailSummary]) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                r#"
                INSERT INTO emails (id, subject, snippet, date_epoch)
                VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT(id) DO UPDATE SET
                  subject=excluded.subject,
                  snippet=excluded.snippet,
                  date_epoch=excluded.date_epoch
                "#,
            )?;

            for it in items {
                stmt.execute(params![it.id, it.subject, it.snippet, it.date_epoch])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    fn upsert_body(&self, body: &EmailBody) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO bodies (id, body)
            VALUES (?1, ?2)
            ON CONFLICT(id) DO UPDATE SET
              body=excluded.body
            "#,
            params![body.id, body.body],
        )?;
        Ok(())
    }

    fn list_page(&self, page: u32, page_size: u32) -> Result<Vec<EmailSummary>> {
        let limit = page_size as i64;
        let offset = (page as i64) * (page_size as i64);

        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, subject, snippet, date_epoch
            FROM emails
            ORDER BY date_epoch DESC, id DESC
            LIMIT ?1 OFFSET ?2
            "#,
        )?;

        let mut rows = stmt.query(params![limit, offset])?;
        let mut out = Vec::new();

        while let Some(r) = rows.next()? {
            out.push(EmailSummary {
                id: r.get::<_, i64>(0)? as EmailId,
                subject: r.get(1)?,
                snippet: r.get(2)?,
                date_epoch: r.get(3)?,
            });
        }
        Ok(out)
    }

    fn get_body(&self, id: EmailId) -> Result<Option<EmailBody>> {
        let mut stmt = self
            .conn
            .prepare(r#"SELECT body FROM bodies WHERE id=?1"#)?;

        let mut rows = stmt.query(params![id])?;
        if let Some(r) = rows.next()? {
            let body: String = r.get(0)?;
            Ok(Some(EmailBody { id, body }))
        } else {
            Ok(None)
        }
    }

    fn prune_keep_recent(&self, keep: usize) -> Result<()> {
        let keep_i64 = keep as i64;
        let tx = self.conn.transaction()?;

        // Keep only latest N emails by date_epoch/id
        tx.execute(
            r#"
            DELETE FROM emails
            WHERE id NOT IN (
              SELECT id FROM emails
              ORDER BY date_epoch DESC, id DESC
              LIMIT ?1
            )
            "#,
            params![keep_i64],
        )?;

        // Remove bodies that no longer have a summary
        tx.execute(
            r#"
            DELETE FROM bodies
            WHERE id NOT IN (SELECT id FROM emails)
            "#,
            [],
        )?;

        tx.commit()?;
        Ok(())
    }

    fn get_meta_i64(&self, key: &str) -> Result<Option<i64>> {
        let mut stmt = self
            .conn
            .prepare(r#"SELECT value FROM meta WHERE key=?1"#)?;
        let mut rows = stmt.query(params![key])?;
        if let Some(r) = rows.next()? {
            Ok(Some(r.get(0)?))
        } else {
            Ok(None)
        }
    }

    fn set_meta_i64(&self, key: &str, value: i64) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO meta (key, value) VALUES (?1, ?2)
            ON CONFLICT(key) DO UPDATE SET value=excluded.value
            "#,
            params![key, value],
        )?;
        Ok(())
    }
}

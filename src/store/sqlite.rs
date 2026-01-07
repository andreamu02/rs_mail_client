use anyhow::Result;
use rusqlite::{Connection, params};
use std::sync::Mutex;

use crate::domain::email::{EmailBody, EmailId, EmailSummary};
use crate::store::repo::MailRepository;

pub struct SqliteRepo {
    conn: Mutex<Connection>,
}

impl SqliteRepo {
    pub fn open(path: &std::path::Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        let repo = Self {
            conn: Mutex::new(conn),
        };
        repo.migrate()?;
        Ok(repo)
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            r#"
            PRAGMA journal_mode=WAL;

            CREATE TABLE IF NOT EXISTS emails (
                id          INTEGER PRIMARY KEY,
                from_name   TEXT NOT NULL,
                subject     TEXT NOT NULL,
                snippet     TEXT NOT NULL,
                date_epoch  INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS bodies (
                id          INTEGER PRIMARY KEY,
                body        TEXT NOT NULL
            );

            -- NEW: store raw RFC822 bytes (BODY.PEEK[])
            CREATE TABLE IF NOT EXISTS raw_messages (
                id          INTEGER PRIMARY KEY,
                raw         BLOB NOT NULL
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
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                r#"
                INSERT INTO emails (id, from_name, subject, snippet, date_epoch)
                VALUES (?1, ?2, ?3, ?4, ?5)
                ON CONFLICT(id) DO UPDATE SET
                  from_name=excluded.from_name,
                  subject=excluded.subject,
                  snippet=excluded.snippet,
                  date_epoch=excluded.date_epoch
                "#,
            )?;

            for it in items {
                stmt.execute(params![
                    it.id,
                    it.from_name,
                    it.subject,
                    it.snippet,
                    it.date_epoch
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    fn upsert_body(&self, body: &EmailBody) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
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

    fn upsert_raw(&self, id: EmailId, raw: &[u8]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT INTO raw_messages (id, raw)
            VALUES (?1, ?2)
            ON CONFLICT(id) DO UPDATE SET
              raw=excluded.raw
            "#,
            params![id, raw],
        )?;
        Ok(())
    }

    fn get_raw(&self, id: EmailId) -> Result<Option<Vec<u8>>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(r#"SELECT raw FROM raw_messages WHERE id=?1"#)?;
        let mut rows = stmt.query(params![id])?;
        if let Some(r) = rows.next()? {
            let raw: Vec<u8> = r.get(0)?;
            Ok(Some(raw))
        } else {
            Ok(None)
        }
    }

    fn list_page(&self, page: u32, page_size: u32) -> Result<Vec<EmailSummary>> {
        let limit = page_size as i64;
        let offset = (page as i64) * (page_size as i64);
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, from_name, subject, snippet, date_epoch
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
                from_name: r.get(1)?,
                subject: r.get(2)?,
                snippet: r.get(3)?,
                date_epoch: r.get(4)?,
            });
        }
        Ok(out)
    }

    fn get_body(&self, id: EmailId) -> Result<Option<EmailBody>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(r#"SELECT body FROM bodies WHERE id=?1"#)?;
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
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

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

        tx.execute(
            r#"
            DELETE FROM bodies
            WHERE id NOT IN (SELECT id FROM emails)
            "#,
            [],
        )?;

        tx.execute(
            r#"
            DELETE FROM raw_messages
            WHERE id NOT IN (SELECT id FROM emails)
            "#,
            [],
        )?;

        tx.commit()?;
        Ok(())
    }

    fn get_meta_i64(&self, key: &str) -> Result<Option<i64>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(r#"SELECT value FROM meta WHERE key=?1"#)?;
        let mut rows = stmt.query(params![key])?;
        if let Some(r) = rows.next()? {
            Ok(Some(r.get(0)?))
        } else {
            Ok(None)
        }
    }

    fn set_meta_i64(&self, key: &str, value: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT INTO meta (key, value) VALUES (?1, ?2)
            ON CONFLICT(key) DO UPDATE SET value=excluded.value
            "#,
            params![key, value],
        )?;
        Ok(())
    }
}

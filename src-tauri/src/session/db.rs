use chrono::{Duration, Utc};
use rusqlite::{params, Connection};
use serde::Serialize;
use uuid::Uuid;

use super::encryption::{open_encrypted, open_encrypted_in_memory};
use super::export::{SessionExport, SessionExportEvent};
use super::saved::SavedSession;

pub const DEFAULT_SESSION_RETENTION_DAYS: u32 = 90;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub started_at: String,
    pub ended_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EventInsert {
    pub session_id: String,
    pub event_type: String,
    pub text: String,
    pub is_final: bool,
    pub started_at_ms: i64,
    pub ended_at_ms: i64,
}

pub struct SessionDb {
    conn: Connection,
}

impl SessionDb {
    pub fn open_in_memory() -> rusqlite::Result<Self> {
        let conn = open_encrypted_in_memory().map_err(rusqlite::Error::InvalidParameterName)?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    pub fn open(path: &std::path::Path) -> rusqlite::Result<Self> {
        let conn = open_encrypted(path).map_err(rusqlite::Error::InvalidParameterName)?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> rusqlite::Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                output_device_id TEXT NOT NULL,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS events (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                text TEXT NOT NULL,
                is_final INTEGER NOT NULL,
                started_at_ms INTEGER NOT NULL,
                ended_at_ms INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY(session_id) REFERENCES sessions(id)
            );
            CREATE TABLE IF NOT EXISTS saved_sessions (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                date TEXT NOT NULL,
                started_at TEXT NOT NULL,
                ended_at TEXT NOT NULL,
                payload TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            ",
        )
    }

    pub fn start_session(&self, title: &str, output_device_id: &str) -> rusqlite::Result<String> {
        let id = Uuid::new_v4().to_string();
        self.start_session_with_id(&id, title, output_device_id)?;
        Ok(id)
    }

    pub fn start_session_with_id(
        &self,
        id: &str,
        title: &str,
        output_device_id: &str,
    ) -> rusqlite::Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO sessions (id, title, output_device_id, started_at, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, title, output_device_id, now, now],
        )?;
        Ok(())
    }

    pub fn end_session(&self, session_id: &str) -> rusqlite::Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE sessions SET ended_at = ?1 WHERE id = ?2",
            params![now, session_id],
        )?;
        Ok(())
    }

    pub fn insert_event(&self, event: EventInsert) -> rusqlite::Result<()> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO events (id, session_id, event_type, text, is_final, started_at_ms, ended_at_ms, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                event.session_id,
                event.event_type,
                event.text,
                if event.is_final { 1 } else { 0 },
                event.started_at_ms,
                event.ended_at_ms,
                now
            ],
        )?;
        Ok(())
    }

    pub fn load_export(&self, session_id: &str) -> rusqlite::Result<SessionExport> {
        let mut session_stmt = self
            .conn
            .prepare("SELECT id, title, started_at, ended_at FROM sessions WHERE id = ?1")?;
        let (id, title, started_at, ended_at): (String, String, String, Option<String>) =
            session_stmt.query_row(params![session_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?;

        let mut event_stmt = self.conn.prepare(
            "SELECT event_type, text, is_final, started_at_ms, ended_at_ms FROM events WHERE session_id = ?1 ORDER BY created_at ASC",
        )?;
        let events = event_stmt
            .query_map(params![session_id], |row| {
                Ok(SessionExportEvent {
                    event_type: row.get(0)?,
                    text: row.get(1)?,
                    is_final: row.get::<_, i64>(2)? == 1,
                    started_at_ms: row.get(3)?,
                    ended_at_ms: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(SessionExport {
            id,
            title,
            started_at,
            ended_at,
            events,
        })
    }

    pub fn list_sessions(&self) -> rusqlite::Result<Vec<SessionSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, started_at, ended_at FROM sessions ORDER BY started_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SessionSummary {
                id: row.get(0)?,
                title: row.get(1)?,
                started_at: row.get(2)?,
                ended_at: row.get(3)?,
            })
        })?;
        rows.collect()
    }

    pub fn delete_session(&self, session_id: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "DELETE FROM events WHERE session_id = ?1",
            params![session_id],
        )?;
        self.conn
            .execute("DELETE FROM sessions WHERE id = ?1", params![session_id])?;
        Ok(())
    }

    pub fn purge_sessions_older_than_days(&self, days: u32) -> rusqlite::Result<u64> {
        let cutoff = (Utc::now() - Duration::days(days as i64)).to_rfc3339();
        self.conn.execute(
            "DELETE FROM events WHERE session_id IN (
                SELECT id FROM sessions
                WHERE COALESCE(ended_at, started_at) < ?1
            )",
            params![cutoff],
        )?;
        let deleted = self.conn.execute(
            "DELETE FROM sessions WHERE COALESCE(ended_at, started_at) < ?1",
            params![cutoff],
        )?;
        Ok(deleted as u64)
    }

    pub fn list_saved_sessions(&self) -> rusqlite::Result<Vec<SavedSession>> {
        let mut stmt = self
            .conn
            .prepare("SELECT payload FROM saved_sessions ORDER BY ended_at DESC")?;
        let rows = stmt.query_map([], |row| {
            let payload: String = row.get(0)?;
            serde_json::from_str(&payload).map_err(|err| {
                rusqlite::Error::ToSqlConversionFailure(Box::new(err))
            })
        })?;
        rows.collect()
    }

    pub fn upsert_saved_session(&self, session: &SavedSession) -> rusqlite::Result<()> {
        let payload = serde_json::to_string(session).map_err(|err| {
            rusqlite::Error::ToSqlConversionFailure(Box::new(err))
        })?;
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO saved_sessions (id, title, date, started_at, ended_at, payload, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
               title = excluded.title,
               date = excluded.date,
               started_at = excluded.started_at,
               ended_at = excluded.ended_at,
               payload = excluded.payload,
               updated_at = excluded.updated_at",
            params![
                session.id,
                session.title,
                session.date,
                session.started_at,
                session.ended_at,
                payload,
                now,
                now,
            ],
        )?;
        Ok(())
    }

    pub fn delete_saved_session(&self, session_id: &str) -> rusqlite::Result<()> {
        self.conn
            .execute("DELETE FROM saved_sessions WHERE id = ?1", params![session_id])?;
        Ok(())
    }

    pub fn purge_saved_sessions_older_than_days(&self, days: u32) -> rusqlite::Result<u64> {
        let cutoff = (Utc::now() - Duration::days(days as i64)).to_rfc3339();
        self.conn
            .execute("DELETE FROM saved_sessions WHERE ended_at < ?1", params![cutoff])
            .map(|count| count as u64)
    }

    pub fn set_session_times_for_test(
        &self,
        session_id: &str,
        started_at: &str,
        ended_at: &str,
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE sessions SET started_at = ?1, ended_at = ?2 WHERE id = ?3",
            params![started_at, ended_at, session_id],
        )?;
        Ok(())
    }
}

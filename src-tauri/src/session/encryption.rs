use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::secret_store::load_or_create_db_master_key;

const SQLITE_HEADER: &[u8] = b"SQLite format 3\0";

pub fn is_plaintext_sqlite(path: &Path) -> Result<bool, String> {
    use std::io::Read;
    let mut file =
        std::fs::File::open(path).map_err(|err| format!("Read database header failed: {err}"))?;
    let mut header = [0u8; 16];
    file.read_exact(&mut header)
        .map_err(|err| format!("Read database header failed: {err}"))?;
    Ok(header == SQLITE_HEADER)
}

fn apply_key(conn: &Connection, key: &str) -> rusqlite::Result<()> {
    conn.pragma_update(None, "key", key)
}

fn verify_encrypted_connection(conn: &Connection) -> Result<(), String> {
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |row| row.get::<_, i64>(0))
        .map_err(|err| format!("Encrypted database verification failed: {err}"))?;
    Ok(())
}

fn plaintext_backup_path(path: &Path) -> PathBuf {
    path.with_extension("sqlite3.plaintext.bak")
}

fn secure_delete_file(path: &Path) -> Result<(), String> {
    use std::io::Write;

    if !path.exists() {
        return Ok(());
    }

    let len = std::fs::metadata(path)
        .map_err(|err| format!("Read backup metadata failed: {err}"))?
        .len();
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|err| format!("Open backup for secure delete failed: {err}"))?;
    let chunk = vec![0u8; 64 * 1024];
    let mut remaining = len;
    while remaining > 0 {
        let write_len = std::cmp::min(remaining, chunk.len() as u64) as usize;
        file.write_all(&chunk[..write_len])
            .map_err(|err| format!("Overwrite backup failed: {err}"))?;
        remaining -= write_len as u64;
    }
    file.sync_all()
        .map_err(|err| format!("Flush backup overwrite failed: {err}"))?;
    drop(file);
    std::fs::remove_file(path)
        .map_err(|err| format!("Remove backup failed: {err}"))?;
    Ok(())
}

fn remove_plaintext_backup(path: &Path) -> Result<(), String> {
    secure_delete_file(&plaintext_backup_path(path))
}

fn migrate_plaintext_database(path: &Path, key: &str) -> Result<(), String> {
    let temp_path = path.with_extension("sqlite3.encrypting");
    let backup_path = plaintext_backup_path(path);

    if temp_path.exists() {
        std::fs::remove_file(&temp_path)
            .map_err(|err| format!("Remove stale encryption temp file failed: {err}"))?;
    }

    let conn = Connection::open(path).map_err(|err| format!("Open plaintext database failed: {err}"))?;
    let temp_display = temp_path.display().to_string().replace('\'', "''");
    conn.execute(
        &format!("ATTACH DATABASE '{temp_display}' AS encrypted KEY ?1"),
        [key],
    )
    .map_err(|err| format!("Attach encrypted database failed: {err}"))?;

    if let Err(err) = conn.execute_batch("SELECT sqlcipher_export('encrypted');") {
        let _ = conn.execute_batch("DETACH DATABASE encrypted;");
        let _ = std::fs::remove_file(&temp_path);
        return Err(format!("SQLCipher export failed: {err}"));
    }

    conn.execute_batch("DETACH DATABASE encrypted;")
        .map_err(|err| format!("Detach encrypted database failed: {err}"))?;
    drop(conn);

    let verify = Connection::open(&temp_path)
        .map_err(|err| format!("Open encrypted temp database failed: {err}"))?;
    apply_key(&verify, key).map_err(|err| format!("Apply key to encrypted temp database failed: {err}"))?;
    verify_encrypted_connection(&verify)?;
    drop(verify);

    std::fs::copy(path, &backup_path)
        .map_err(|err| format!("Backup plaintext database failed: {err}"))?;
    std::fs::rename(&temp_path, path)
        .map_err(|err| format!("Replace database with encrypted copy failed: {err}"))?;

    let conn = Connection::open(path).map_err(|err| format!("Open encrypted database failed: {err}"))?;
    apply_key(&conn, &key).map_err(|err| format!("Apply database key failed: {err}"))?;
    verify_encrypted_connection(&conn)?;
    drop(conn);

    remove_plaintext_backup(path)?;
    Ok(())
}

pub fn open_encrypted(path: &Path) -> Result<Connection, String> {
    let key = load_or_create_db_master_key()?;

    if path.exists() && is_plaintext_sqlite(path)? {
        migrate_plaintext_database(path, &key)?;
    }

    let conn = Connection::open(path).map_err(|err| format!("Open database failed: {err}"))?;
    apply_key(&conn, &key).map_err(|err| format!("Apply database key failed: {err}"))?;
    verify_encrypted_connection(&conn)?;
    remove_plaintext_backup(path)?;
    Ok(conn)
}

pub fn open_encrypted_in_memory() -> Result<Connection, String> {
    let key = load_or_create_db_master_key()?;
    let conn = Connection::open_in_memory().map_err(|err| format!("Open in-memory database failed: {err}"))?;
    apply_key(&conn, &key).map_err(|err| format!("Apply in-memory database key failed: {err}"))?;
    Ok(conn)
}

pub fn temp_plaintext_db_path() -> PathBuf {
    let unique = format!(
        "respondent-plaintext-test-{}.sqlite3",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    std::env::temp_dir().join(unique)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    static ENCRYPTION_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn encryption_test_lock() -> MutexGuard<'static, ()> {
        ENCRYPTION_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn migrates_plaintext_database_to_encrypted_sqlcipher() {
        let _guard = encryption_test_lock();
        std::env::set_var("RESPONDENT_SECRET_BACKEND", "memory");
        let path = temp_plaintext_db_path();
        {
            let conn = Connection::open(&path).expect("open plaintext db");
            conn.execute_batch(
                "CREATE TABLE probe (id INTEGER PRIMARY KEY, value TEXT NOT NULL);
                 INSERT INTO probe (value) VALUES ('secret');",
            )
            .expect("seed plaintext db");
        }

        let encrypted = open_encrypted(&path).expect("migrate to encrypted db");
        let value: String = encrypted
            .query_row("SELECT value FROM probe", [], |row| row.get(0))
            .expect("read migrated row");
        assert_eq!(value, "secret");
        assert!(!is_plaintext_sqlite(&path).expect("check header"));
        assert!(
            !plaintext_backup_path(&path).exists(),
            "plaintext backup must be removed after successful migration"
        );

        let _ = std::fs::remove_file(&path);
        std::env::remove_var("RESPONDENT_SECRET_BACKEND");
    }

    #[test]
    fn removes_stale_plaintext_backup_on_encrypted_open() {
        let _guard = encryption_test_lock();
        std::env::set_var("RESPONDENT_SECRET_BACKEND", "memory");
        let path = temp_plaintext_db_path();
        {
            let conn = Connection::open(&path).expect("open plaintext db");
            conn.execute_batch(
                "CREATE TABLE probe (id INTEGER PRIMARY KEY, value TEXT NOT NULL);
                 INSERT INTO probe (value) VALUES ('secret');",
            )
            .expect("seed plaintext db");
        }

        open_encrypted(&path).expect("migrate to encrypted db");
        let backup = plaintext_backup_path(&path);
        std::fs::write(&backup, b"stale plaintext backup from older build")
            .expect("seed stale backup");

        let _ = open_encrypted(&path).expect("reopen encrypted db");
        assert!(!backup.exists(), "stale plaintext backup must be purged on startup");

        let _ = std::fs::remove_file(&path);
        std::env::remove_var("RESPONDENT_SECRET_BACKEND");
    }
}

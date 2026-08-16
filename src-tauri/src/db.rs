use std::path::Path;

use rusqlite::Connection;

use crate::state::{DictEntry, HistoryEntry, Settings};

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            text TEXT NOT NULL,
            created_at TEXT NOT NULL,
            duration_ms INTEGER NOT NULL,
            source TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS dictionary (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            word TEXT NOT NULL UNIQUE,
            created_at TEXT NOT NULL
        );",
    )?;
    Ok(conn)
}

pub fn load_settings(conn: &Connection) -> Settings {
    let row: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = 'app'", [], |r| r.get(0))
        .ok();
    row.and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

pub fn save_settings(conn: &Connection, settings: &Settings) -> rusqlite::Result<()> {
    let json = serde_json::to_string(settings).unwrap_or_default();
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('app', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [json],
    )?;
    Ok(())
}

pub fn insert_history(conn: &Connection, entry: &HistoryEntry) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO history (text, created_at, duration_ms, source) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![
            entry.text,
            entry.created_at,
            entry.duration_ms as i64,
            entry.source
        ],
    )?;
    Ok(())
}

pub fn get_history(conn: &Connection) -> rusqlite::Result<Vec<HistoryEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, text, created_at, duration_ms, source FROM history ORDER BY id DESC LIMIT 500",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(HistoryEntry {
            id: r.get(0)?,
            text: r.get(1)?,
            created_at: r.get(2)?,
            duration_ms: r.get::<_, i64>(3)? as u64,
            source: r.get(4)?,
        })
    })?;
    rows.collect()
}

pub fn delete_history(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM history WHERE id = ?1", [id])?;
    Ok(())
}

pub fn clear_history(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM history", [])?;
    Ok(())
}

pub fn get_dictionary(conn: &Connection) -> rusqlite::Result<Vec<DictEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, word, created_at FROM dictionary ORDER BY word COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(DictEntry {
            id: r.get(0)?,
            word: r.get(1)?,
            created_at: r.get(2)?,
        })
    })?;
    rows.collect()
}

pub fn add_dictionary_word(conn: &Connection, word: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO dictionary (word, created_at) VALUES (?1, ?2)",
        rusqlite::params![word.to_lowercase(), now_timestamp()],
    )?;
    Ok(())
}

pub fn remove_dictionary_word(conn: &Connection, word: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM dictionary WHERE word = ?1", [word.to_lowercase()])?;
    Ok(())
}

pub fn now_timestamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_roundtrip_and_camelcase_migration() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('app', ?1)",
            [r#"{"hotkey":"ctrl+k","mic":"default","engine":"parakeet","language":"auto","onboarded":true,"sttModel":"parakeet-tdt-ctc-110m-int8"}"#],
        )
        .unwrap();
        let s = load_settings(&conn);
        assert_eq!(s.hotkey, "ctrl+k");
        assert!(s.onboarded);
        assert_eq!(s.stt_model, "parakeet-tdt-ctc-110m-int8");

        save_settings(&conn, &s).unwrap();
        let raw: String = conn
            .query_row("SELECT value FROM settings WHERE key='app'", [], |r| r.get(0))
            .unwrap();
        assert!(raw.contains("\"stt_model\""));
        assert!(!raw.contains("sttModel"));
    }
}

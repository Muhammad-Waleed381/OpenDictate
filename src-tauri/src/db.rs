use std::path::Path;

use rusqlite::Connection;

use crate::state::{DictEntry, HistoryEntry, Settings, SnippetEntry};

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
        );
        CREATE TABLE IF NOT EXISTS daily_stats (
            day TEXT PRIMARY KEY,
            words INTEGER NOT NULL DEFAULT 0,
            sessions INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS snippets (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            trigger TEXT NOT NULL UNIQUE COLLATE NOCASE,
            text TEXT NOT NULL,
            created_at TEXT NOT NULL
        );",
    )?;
    // One-time backfill of daily stats from history. Skipped when the user
    // explicitly reset their word stats: an empty `daily_stats` used to look
    // exactly like "first launch after upgrade", so the aggregates were
    // resurrected from history on the next launch, silently undoing the reset.
    let stats_cleared = conn
        .query_row(
            "SELECT 1 FROM settings WHERE key = 'stats_cleared'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .is_ok();
    let backfilled: i64 = conn.query_row("SELECT COUNT(*) FROM daily_stats", [], |r| r.get(0))?;
    if backfilled == 0 && !stats_cleared {
        conn.execute_batch(
            "INSERT INTO daily_stats (day, words, sessions)
             SELECT date(created_at, 'unixepoch', 'localtime'),
                    COALESCE(SUM(
                        CASE WHEN length(text) = 0 THEN 0
                             ELSE length(text) - length(replace(text, ' ', '')) + 1
                        END
                    ), 0),
                    COUNT(*)
             FROM history GROUP BY date(created_at, 'unixepoch', 'localtime');",
        )?;
    }
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
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO history (text, created_at, duration_ms, source) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![
            entry.text,
            entry.created_at,
            entry.duration_ms as i64,
            entry.source
        ],
    )?;
    tx.execute(
        "INSERT INTO daily_stats (day, words, sessions)
         VALUES (date(?1, 'unixepoch', 'localtime'),
                 CASE WHEN length(?2) = 0 THEN 0
                      ELSE length(?2) - length(replace(?2, ' ', '')) + 1
                 END,
                 1)
         ON CONFLICT(day) DO UPDATE SET
             words = words + excluded.words,
             sessions = sessions + excluded.sessions",
        rusqlite::params![entry.created_at, entry.text],
    )?;
    tx.commit()
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

/// Word count for `daily_stats` aggregation, matching the SQL heuristic
/// `length(text) - length(replace(text, ' ', '')) + 1` (with 0 for empty
/// text) so Rust-side adjustments stay consistent with SQL-side inserts.
fn count_words(text: &str) -> i64 {
    if text.is_empty() {
        0
    } else {
        text.split(' ').count() as i64
    }
}

pub fn delete_history(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    let row: Option<(String, String)> = tx
        .query_row(
            "SELECT text, created_at FROM history WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();
    if let Some((text, created_at)) = row {
        tx.execute("DELETE FROM history WHERE id = ?1", [id])?;
        // Keep the daily aggregate in sync with the history list.
        tx.execute(
            "UPDATE daily_stats SET
                 words = MAX(words - ?2, 0),
                 sessions = MAX(sessions - 1, 0)
             WHERE day = date(?1, 'unixepoch', 'localtime')",
            rusqlite::params![created_at, count_words(&text)],
        )?;
    }
    tx.commit()
}

pub fn update_history(conn: &Connection, id: i64, text: &str) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    let row: Option<(String, String)> = tx
        .query_row(
            "SELECT text, created_at FROM history WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();
    if let Some((old_text, created_at)) = row {
        let trimmed = text.trim();
        tx.execute(
            "UPDATE history SET text = ?1 WHERE id = ?2",
            rusqlite::params![trimmed, id],
        )?;
        let delta = count_words(trimmed) - count_words(&old_text);
        if delta != 0 {
            tx.execute(
                "UPDATE daily_stats SET words = MAX(words + ?2, 0)
                 WHERE day = date(?1, 'unixepoch', 'localtime')",
                rusqlite::params![created_at, delta],
            )?;
        }
    }
    tx.commit()
}

pub fn clear_history(conn: &Connection) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM history", [])?;
    tx.execute("DELETE FROM daily_stats", [])?;
    tx.commit()
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
        "INSERT INTO dictionary (word, created_at)
         SELECT ?1, ?2
         WHERE NOT EXISTS (
             SELECT 1 FROM dictionary WHERE word COLLATE NOCASE = ?1
         )",
        rusqlite::params![word.trim(), now_timestamp()],
    )?;
    Ok(())
}

pub fn remove_dictionary_word(conn: &Connection, word: &str) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM dictionary WHERE word COLLATE NOCASE = ?1",
        [word.trim()],
    )?;
    Ok(())
}

pub fn list_snippets(conn: &Connection) -> rusqlite::Result<Vec<SnippetEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, trigger, text, created_at FROM snippets ORDER BY trigger COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(SnippetEntry {
            id: r.get(0)?,
            trigger: r.get(1)?,
            text: r.get(2)?,
            created_at: r.get(3)?,
        })
    })?;
    rows.collect()
}

/// Inserts a snippet unless the trigger is already taken (case-insensitively).
/// Returns whether the row was inserted.
pub fn add_snippet(conn: &Connection, trigger: &str, text: &str) -> rusqlite::Result<bool> {
    let inserted = conn.execute(
        "INSERT INTO snippets (trigger, text, created_at)
         SELECT ?1, ?2, ?3
         WHERE NOT EXISTS (
             SELECT 1 FROM snippets WHERE trigger COLLATE NOCASE = ?1
         )",
        rusqlite::params![trigger.trim(), text.trim(), now_timestamp()],
    )?;
    Ok(inserted == 1)
}

/// Updates a snippet's trigger and text unless the id is missing or the new
/// trigger collides with a different row. Returns whether a row was updated.
pub fn update_snippet(
    conn: &Connection,
    id: i64,
    trigger: &str,
    text: &str,
) -> rusqlite::Result<bool> {
    let updated = conn.execute(
        "UPDATE snippets SET trigger = ?1, text = ?2
         WHERE id = ?3
           AND NOT EXISTS (
               SELECT 1 FROM snippets WHERE trigger COLLATE NOCASE = ?1 AND id != ?3
           )",
        rusqlite::params![trigger.trim(), text.trim(), id],
    )?;
    Ok(updated == 1)
}

pub fn remove_snippet(conn: &Connection, id: i64) -> rusqlite::Result<bool> {
    let removed = conn.execute("DELETE FROM snippets WHERE id = ?1", [id])?;
    Ok(removed == 1)
}

pub fn word_stats(conn: &Connection) -> rusqlite::Result<Vec<(String, i64)>> {
    let mut stmt = conn.prepare("SELECT day, words FROM daily_stats ORDER BY day")?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect()
}

pub fn reset_word_stats(conn: &Connection) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM daily_stats", [])?;
    // Persist the reset. Without this marker the startup backfill heuristic
    // ("empty daily_stats == first launch after upgrade") resurrected the
    // aggregates from history on the next launch, silently undoing the reset.
    tx.execute(
        "INSERT INTO settings (key, value) VALUES ('stats_cleared', '1')
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [],
    )?;
    tx.commit()
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

    fn stats_tables(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE history (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 text TEXT NOT NULL,
                 created_at TEXT NOT NULL,
                 duration_ms INTEGER NOT NULL,
                 source TEXT NOT NULL
             );
             CREATE TABLE daily_stats (
                 day TEXT PRIMARY KEY,
                 words INTEGER NOT NULL DEFAULT 0,
                 sessions INTEGER NOT NULL DEFAULT 0
             );",
        )
        .unwrap();
    }

    fn day_words(conn: &Connection, day: &str) -> (i64, i64) {
        conn.query_row(
            "SELECT words, sessions FROM daily_stats WHERE day = ?1",
            [day],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap_or((0, 0))
    }

    #[test]
    fn count_words_matches_sql_heuristic() {
        assert_eq!(count_words(""), 0);
        assert_eq!(count_words("hello"), 1);
        assert_eq!(count_words("hello world"), 2);
        // Double spaces count like the SQL length-diff heuristic does.
        assert_eq!(count_words("a  b"), 3);
    }

    #[test]
    fn reset_word_stats_persists_cleared_marker() {
        let conn = Connection::open_in_memory().unwrap();
        stats_tables(&conn);
        reset_word_stats(&conn).unwrap();
        let marker: i64 = conn
            .query_row(
                "SELECT 1 FROM settings WHERE key = 'stats_cleared'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(marker, 1);
        // The backfill heuristic in open() skips when this marker is present;
        // without it the reset was undone on the next launch.
        let backfill_would_run = conn
            .query_row("SELECT 1 FROM settings WHERE key = 'stats_cleared'", [], |r| {
                r.get::<_, i64>(0)
            })
            .is_err();
        assert!(!backfill_would_run);
    }

    #[test]
    fn history_edits_keep_daily_stats_in_sync() {
        let conn = Connection::open_in_memory().unwrap();
        stats_tables(&conn);
        // 1970-01-01T00:01:23Z
        let entry = HistoryEntry {
            id: 0,
            text: "one two three".into(),
            created_at: "83".into(),
            duration_ms: 1000,
            source: "dictate".into(),
        };
        insert_history(&conn, &entry).unwrap();
        assert_eq!(day_words(&conn, "1970-01-01"), (3, 1));

        // Editing the text adjusts the word count.
        update_history(&conn, 1, "one two three four five").unwrap();
        assert_eq!(day_words(&conn, "1970-01-01"), (5, 1));

        // Deleting the entry zeroes the day out (floored, no negatives).
        delete_history(&conn, 1).unwrap();
        assert_eq!(day_words(&conn, "1970-01-01"), (0, 0));
    }

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
        assert!(!s.spoken_punctuation);
        assert!(!s.audio_feedback);
        assert_eq!(s.audio_feedback_volume, 0.5);

        save_settings(&conn, &s).unwrap();
        let raw: String = conn
            .query_row("SELECT value FROM settings WHERE key='app'", [], |r| r.get(0))
            .unwrap();
        assert!(raw.contains("\"stt_model\""));
        assert!(!raw.contains("sttModel"));
        assert!(raw.contains("\"spoken_punctuation\""));
        assert!(raw.contains("\"audio_feedback\""));

        let mut patched = s;
        patched.audio_feedback = true;
        patched.audio_feedback_volume = 0.25;
        save_settings(&conn, &patched).unwrap();
        let reloaded = load_settings(&conn);
        assert!(reloaded.audio_feedback);
        assert_eq!(reloaded.audio_feedback_volume, 0.25);
    }

    #[test]
    fn dictionary_preserves_casing_and_deduplicates_case_insensitively() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE dictionary (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                word TEXT NOT NULL UNIQUE,
                created_at TEXT NOT NULL
            );",
        )
        .unwrap();

        add_dictionary_word(&conn, "iPhone").unwrap();
        add_dictionary_word(&conn, "iphone").unwrap();

        let words = get_dictionary(&conn).unwrap();
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].word, "iPhone");
    }

    #[test]
    fn snippets_crud_and_case_insensitive_triggers() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE snippets (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                trigger TEXT NOT NULL UNIQUE COLLATE NOCASE,
                text TEXT NOT NULL,
                created_at TEXT NOT NULL
            );",
        )
        .unwrap();

        assert!(add_snippet(&conn, "Signature", "Best regards").unwrap());
        assert!(!add_snippet(&conn, "signature", "Other").unwrap(), "duplicate trigger rejected");
        assert!(add_snippet(&conn, "Meeting notes", "Notes").unwrap());

        let mut list = list_snippets(&conn).unwrap();
        assert_eq!(list.len(), 2);
        list.sort_by(|a, b| a.trigger.cmp(&b.trigger));
        assert_eq!(list[0].trigger, "Meeting notes");

        let sig = list_snippets(&conn)
            .unwrap()
            .into_iter()
            .find(|s| s.trigger.eq_ignore_ascii_case("signature"))
            .unwrap();
        assert!(update_snippet(&conn, sig.id, "Signature", "Best, Waleed").unwrap());
        assert!(!update_snippet(&conn, sig.id, "meeting notes", "collides").unwrap(), "collision rejected");
        assert!(remove_snippet(&conn, sig.id).unwrap());
        assert_eq!(list_snippets(&conn).unwrap().len(), 1);
    }
}

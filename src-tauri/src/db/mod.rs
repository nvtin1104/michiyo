pub mod activity;
pub mod events;
pub mod habits;
pub mod notes;
pub mod reminders;
pub mod sync;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use ulid::Ulid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub messages: Vec<Message>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyHistory {
    #[serde(default)]
    conversations: Vec<Conversation>,
}

pub type DbResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub fn open(path: &Path) -> DbResult<Connection> {
    let mut connection = Connection::open(path)?;
    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    migrate(&mut connection)?;
    Ok(connection)
}

fn migrate(connection: &mut Connection) -> DbResult<()> {
    let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > 1 {
        return Err("Phiên bản dữ liệu không được hỗ trợ".into());
    }
    if version == 0 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(include_str!("migrations/0001_init.sql"))?;
        transaction.pragma_update(None, "user_version", 1)?;
        transaction.commit()?;
    }
    Ok(())
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

fn device_id(tx: &Transaction<'_>) -> DbResult<String> {
    let existing: Option<String> = tx
        .query_row(
            "SELECT value FROM sync_state WHERE key = 'device_id'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = existing {
        return Ok(id);
    }
    let id = Ulid::generate().to_string();
    tx.execute(
        "INSERT INTO sync_state (key, value) VALUES ('device_id', ?1)",
        [&id],
    )?;
    Ok(id)
}

fn write_conversation(
    tx: &Transaction<'_>,
    conversation: &Conversation,
    position: i64,
    device: &str,
) -> DbResult<()> {
    let timestamp = now();
    let data = serde_json::json!({"title": conversation.title, "position": position}).to_string();
    tx.execute("INSERT INTO conversations (id, updated_at, deleted_at, device_id, rev, data) VALUES (?1, ?2, NULL, ?3, 1, ?4) ON CONFLICT(id) DO UPDATE SET updated_at=excluded.updated_at, rev=conversations.rev+1, data=excluded.data", params![conversation.id, timestamp, device, data])?;
    tx.execute(
        "DELETE FROM messages WHERE json_extract(data, '$.conversation_id') = ?1",
        [&conversation.id],
    )?;
    for (position, message) in conversation.messages.iter().enumerate() {
        let data = serde_json::json!({"conversation_id": conversation.id, "position": position, "role": message.role, "content": message.content}).to_string();
        tx.execute("INSERT INTO messages (id, updated_at, deleted_at, device_id, rev, data) VALUES (?1, ?2, NULL, ?3, 1, ?4)", params![Ulid::generate().to_string(), timestamp, device, data])?;
    }
    Ok(())
}

/// Import only after parsing the whole legacy file; the marker and all rows commit together.
pub fn import_history(db: &mut Connection, path: &Path) -> DbResult<()> {
    let imported: Option<String> = db
        .query_row(
            "SELECT value FROM sync_state WHERE key='history_imported'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    let archive = if let Some(marker) = imported {
        marker == "1"
    } else {
        let (legacy, present) = match fs::read(path) {
            Ok(bytes) => (
                serde_json::from_slice::<LegacyHistory>(&bytes)?.conversations,
                true,
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Vec::new(), false),
            Err(e) => return Err(e.into()),
        };
        let tx = db.transaction()?;
        let device = device_id(&tx)?;
        for (position, conversation) in legacy.into_iter().enumerate() {
            let mut conversation = conversation;
            // Legacy UUIDs were not shared externally; convert them into ULIDs.
            conversation.id = Ulid::generate().to_string();
            write_conversation(&tx, &conversation, position as i64, &device)?;
        }
        tx.execute(
            "INSERT INTO sync_state (key, value) VALUES ('history_imported', ?1)",
            [if present { "1" } else { "none" }],
        )?;
        tx.commit()?;
        present
    };
    // A crash between commit and rename is safe: the marker prevents re-import.
    // Never overwrite an existing backup. If present, retain the original too.
    let backup = path.with_file_name("history.json.bak");
    if archive && path.exists() && !backup.exists() {
        fs::rename(path, backup)?;
    }
    Ok(())
}

pub fn list_history(db: &Connection) -> DbResult<Vec<Conversation>> {
    let mut statement = db.prepare("SELECT id, json_extract(data, '$.title') FROM conversations WHERE deleted_at IS NULL ORDER BY CAST(json_extract(data, '$.position') AS INTEGER), id")?;
    let rows = statement.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut result = Vec::new();
    for row in rows {
        let (id, title) = row?;
        let mut statement = db.prepare("SELECT json_extract(data, '$.role'), json_extract(data, '$.content') FROM messages WHERE deleted_at IS NULL AND json_extract(data, '$.conversation_id') = ?1 ORDER BY CAST(json_extract(data, '$.position') AS INTEGER), id")?;
        let messages = statement
            .query_map([&id], |r| {
                Ok(Message {
                    role: r.get(0)?,
                    content: r.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        result.push(Conversation {
            id,
            title,
            messages,
        });
    }
    Ok(result)
}

pub fn save_history(
    db: &mut Connection,
    conversation: &Conversation,
    limit: usize,
) -> DbResult<Vec<Conversation>> {
    if conversation.id.is_empty()
        || conversation
            .messages
            .iter()
            .any(|m| m.role != "user" && m.role != "assistant")
    {
        return Err("Hội thoại không hợp lệ".into());
    }
    let tx = db.transaction()?;
    let device = device_id(&tx)?;
    let position: Option<i64> = tx.query_row("SELECT CAST(json_extract(data, '$.position') AS INTEGER) FROM conversations WHERE id = ?1", [&conversation.id], |r| r.get(0)).optional()?;
    let position = match position {
        Some(position) => position,
        None => tx.query_row("SELECT COALESCE(MIN(CAST(json_extract(data, '$.position') AS INTEGER)), 0) - 1 FROM conversations", [], |r| r.get(0))?,
    };
    write_conversation(&tx, conversation, position, &device)?;
    let limit = limit.clamp(1, 1000) as i64;
    tx.execute("DELETE FROM messages WHERE json_extract(data, '$.conversation_id') IN (SELECT id FROM conversations ORDER BY CAST(json_extract(data, '$.position') AS INTEGER), id LIMIT -1 OFFSET ?1)", [limit])?;
    tx.execute("DELETE FROM conversations WHERE id IN (SELECT id FROM conversations ORDER BY CAST(json_extract(data, '$.position') AS INTEGER), id LIMIT -1 OFFSET ?1)", [limit])?;
    tx.commit()?;
    list_history(db)
}

pub fn delete_history(db: &mut Connection, id: &str) -> DbResult<Vec<Conversation>> {
    let tx = db.transaction()?;
    tx.execute(
        "DELETE FROM messages WHERE json_extract(data, '$.conversation_id') = ?1",
        [id],
    )?;
    tx.execute("DELETE FROM conversations WHERE id = ?1", [id])?;
    tx.commit()?;
    list_history(db)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn test_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("michiyo-test-{}", Ulid::generate()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
    #[test]
    fn migration_and_import_are_idempotent() -> DbResult<()> {
        let dir = test_dir();
        let legacy = dir.join("history.json");
        fs::write(
            &legacy,
            r#"{"conversations":[{"id":"old","title":"Chat","messages":[{"role":"user","content":"Hello"}]}]}"#,
        )?;
        let mut db = open(&dir.join("test.db"))?;
        import_history(&mut db, &legacy)?;
        import_history(&mut db, &legacy)?;
        let history = list_history(&db)?;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].messages[0].content, "Hello");
        assert!(Ulid::from_string(&history[0].id).is_ok());
        assert!(dir.join("history.json.bak").exists());
        db.execute(
            "INSERT INTO notes_fts (id, title, body) VALUES ('1', 'hello', 'world')",
            [],
        )?;
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM notes_fts WHERE notes_fts MATCH 'world'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            1
        );
        drop(db);
        fs::remove_dir_all(dir)?;
        Ok(())
    }
    #[test]
    fn malformed_history_never_marks_import_or_removes_original() -> DbResult<()> {
        let dir = test_dir();
        let legacy = dir.join("history.json");
        fs::write(&legacy, "not json")?;
        let mut db = open(&dir.join("test.db"))?;
        assert!(import_history(&mut db, &legacy).is_err());
        assert!(legacy.exists());
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM sync_state WHERE key='history_imported'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            0
        );
        drop(db);
        fs::remove_dir_all(dir)?;
        Ok(())
    }
    #[test]
    fn absent_history_and_existing_backup_are_preserved() -> DbResult<()> {
        let dir = test_dir();
        let legacy = dir.join("history.json");
        let mut db = open(&dir.join("test.db"))?;
        import_history(&mut db, &legacy)?;
        fs::write(&legacy, "later file")?;
        import_history(&mut db, &legacy)?;
        assert_eq!(fs::read_to_string(&legacy)?, "later file");
        drop(db);
        fs::remove_dir_all(dir)?;

        let dir = test_dir();
        let legacy = dir.join("history.json");
        fs::write(&legacy, r#"{"conversations":[]}"#)?;
        fs::write(dir.join("history.json.bak"), "older backup")?;
        let mut db = open(&dir.join("test.db"))?;
        import_history(&mut db, &legacy)?;
        assert!(legacy.exists());
        assert_eq!(
            fs::read_to_string(dir.join("history.json.bak"))?,
            "older backup"
        );
        drop(db);
        fs::remove_dir_all(dir)?;
        Ok(())
    }
    #[test]
    fn chat_ipc_json_round_trip() -> DbResult<()> {
        let json = r#"{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","title":"Chào","messages":[{"role":"assistant","content":"Xin chào"}]}"#;
        let conversation: Conversation = serde_json::from_str(json)?;
        let encoded = serde_json::to_string(&conversation)?;
        assert_eq!(
            serde_json::from_str::<Conversation>(&encoded)?,
            conversation
        );
        Ok(())
    }
    #[test]
    fn save_update_limit_and_delete_are_transactional() -> DbResult<()> {
        let mut db = Connection::open_in_memory()?;
        migrate(&mut db)?;
        let first = Conversation {
            id: Ulid::generate().to_string(),
            title: "first".into(),
            messages: vec![Message {
                role: "user".into(),
                content: "one".into(),
            }],
        };
        let mut second = Conversation {
            id: Ulid::generate().to_string(),
            title: "second".into(),
            messages: vec![],
        };
        save_history(&mut db, &first, 2)?;
        save_history(&mut db, &second, 2)?;
        second.messages.push(Message {
            role: "assistant".into(),
            content: "two".into(),
        });
        assert_eq!(save_history(&mut db, &second, 2)?[0], second);
        let third = Conversation {
            id: Ulid::generate().to_string(),
            title: "third".into(),
            messages: vec![],
        };
        let limited = save_history(&mut db, &third, 2)?;
        assert_eq!(limited, vec![third.clone(), second.clone()]);
        let invalid = Conversation {
            messages: vec![Message {
                role: "invalid".into(),
                content: "bad".into(),
            }],
            ..second.clone()
        };
        assert!(save_history(&mut db, &invalid, 2).is_err());
        assert_eq!(list_history(&db)?, limited);
        assert_eq!(delete_history(&mut db, &second.id)?, vec![third]);
        Ok(())
    }
}

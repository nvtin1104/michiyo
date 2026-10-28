use super::{device_id, now, DbResult};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Note {
    pub id: String,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
    pub device_id: String,
    pub rev: i64,
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
    pub pinned: bool,
}
#[derive(Debug, Deserialize)]
pub struct NoteInput {
    pub id: Option<String>,
    pub rev: Option<i64>,
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
    pub pinned: bool,
}
fn row_note(row: &rusqlite::Row<'_>) -> rusqlite::Result<Note> {
    let data: String = row.get(5)?;
    let data: serde_json::Value = serde_json::from_str(&data).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let tags = serde_json::from_value(data["tags"].clone()).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(Note {
        id: row.get(0)?,
        updated_at: row.get(1)?,
        deleted_at: row.get(2)?,
        device_id: row.get(3)?,
        rev: row.get(4)?,
        title: data["title"].as_str().unwrap_or_default().to_string(),
        body: data["body"].as_str().unwrap_or_default().to_string(),
        tags,
        pinned: data["pinned"].as_bool().unwrap_or(false),
    })
}
fn get(tx: &Transaction<'_>, id: &str) -> DbResult<Option<Note>> {
    Ok(tx
        .query_row(
            "SELECT id, updated_at, deleted_at, device_id, rev, data FROM notes WHERE id = ?1",
            [id],
            row_note,
        )
        .optional()?)
}
fn record_change(tx: &Transaction<'_>, note: &Note, action: &str) -> DbResult<()> {
    let body = serde_json::to_string(note)?;
    tx.execute(
        "INSERT INTO outbox (id, kind, rev, data) VALUES (?1, 'note', ?2, ?3)",
        params![note.id, note.rev, body],
    )?;
    let activity_id = Ulid::generate().to_string();
    let data = serde_json::json!({"kind": action, "occurred_at": note.updated_at, "data": {"note_id": note.id, "title": note.title}});
    tx.execute("INSERT INTO activity (id, updated_at, deleted_at, device_id, rev, data) VALUES (?1, ?2, NULL, ?3, 1, ?4)", params![activity_id, note.updated_at, note.device_id, data.to_string()])?;
    let activity = serde_json::json!({"id": activity_id, "updated_at": note.updated_at, "deleted_at": null, "device_id": note.device_id, "rev": 1, "kind": action, "occurred_at": note.updated_at, "data": {"note_id": note.id, "title": note.title}});
    tx.execute(
        "INSERT INTO outbox (id, kind, rev, data) VALUES (?1, 'activity', 1, ?2)",
        params![activity_id, activity.to_string()],
    )?;
    Ok(())
}
pub fn list(db: &Connection, query: &str) -> DbResult<Vec<Note>> {
    let words: Vec<String> = query
        .split_whitespace()
        .take(20)
        .map(|w| format!("\"{}\"", w.replace('"', "\"\"")))
        .collect();
    let sql = if words.is_empty() {
        "SELECT id, updated_at, deleted_at, device_id, rev, data FROM notes WHERE deleted_at IS NULL ORDER BY json_extract(data, '$.pinned') DESC, updated_at DESC, id"
    } else {
        "SELECT id, updated_at, deleted_at, device_id, rev, data FROM notes WHERE deleted_at IS NULL AND id IN (SELECT id FROM notes_fts WHERE notes_fts MATCH ?1) ORDER BY json_extract(data, '$.pinned') DESC, updated_at DESC, id"
    };
    let mut stmt = db.prepare(sql)?;
    let mut notes = Vec::new();
    if words.is_empty() {
        for row in stmt.query_map([], row_note)? {
            notes.push(row?);
        }
    } else {
        for row in stmt.query_map([words.join(" AND ")], row_note)? {
            notes.push(row?);
        }
    }
    Ok(notes)
}
pub fn save(db: &mut Connection, input: NoteInput) -> DbResult<Note> {
    if input.title.trim().is_empty()
        || input.title.chars().count() > 200
        || input.body.len() > 1_000_000
        || input.tags.len() > 30
        || input
            .tags
            .iter()
            .any(|t| t.is_empty() || t.chars().count() > 50)
    {
        return Err("Ghi chú không hợp lệ".into());
    }
    let tx = db.transaction()?;
    let device = device_id(&tx)?;
    let id = input.id.unwrap_or_else(|| Ulid::generate().to_string());
    if Ulid::from_string(&id).is_err() {
        return Err("Mã ghi chú không hợp lệ".into());
    }
    let existing = get(&tx, &id)?;
    if let Some(ref note) = existing {
        if note.deleted_at.is_some() || input.rev != Some(note.rev) {
            return Err("Ghi chú đã thay đổi; hãy tải lại".into());
        }
    } else if input.rev.is_some() {
        return Err("Không tìm thấy ghi chú".into());
    }
    let note = Note {
        id,
        updated_at: now(),
        deleted_at: None,
        device_id: device,
        rev: existing.map_or(1, |n| n.rev + 1),
        title: input.title.trim().into(),
        body: input.body,
        tags: input.tags,
        pinned: input.pinned,
    };
    let data = serde_json::json!({"title": note.title, "body": note.body, "tags": note.tags, "pinned": note.pinned});
    tx.execute("INSERT INTO notes (id, updated_at, deleted_at, device_id, rev, data) VALUES (?1, ?2, NULL, ?3, ?4, ?5) ON CONFLICT(id) DO UPDATE SET updated_at=excluded.updated_at, device_id=excluded.device_id, rev=excluded.rev, data=excluded.data", params![note.id, note.updated_at, note.device_id, note.rev, data.to_string()])?;
    tx.execute("DELETE FROM notes_fts WHERE id=?1", [&note.id])?;
    tx.execute(
        "INSERT INTO notes_fts (id, title, body) VALUES (?1, ?2, ?3)",
        params![note.id, note.title, note.body],
    )?;
    record_change(&tx, &note, "note_saved")?;
    tx.commit()?;
    Ok(note)
}
pub fn delete(db: &mut Connection, id: &str, rev: i64) -> DbResult<()> {
    let tx = db.transaction()?;
    let mut note = get(&tx, id)?.ok_or("Không tìm thấy ghi chú")?;
    if note.deleted_at.is_some() || note.rev != rev {
        return Err("Ghi chú đã thay đổi; hãy tải lại".into());
    }
    note.device_id = device_id(&tx)?;
    note.updated_at = now();
    note.deleted_at = Some(note.updated_at);
    note.rev += 1;
    tx.execute(
        "UPDATE notes SET updated_at=?1, deleted_at=?1, device_id=?2, rev=?3 WHERE id=?4",
        params![note.updated_at, note.device_id, note.rev, note.id],
    )?;
    tx.execute("DELETE FROM notes_fts WHERE id=?1", [&note.id])?;
    record_change(&tx, &note, "note_deleted")?;
    tx.commit()?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notes_search_conflicts_tombstones_and_outbox() -> DbResult<()> {
        let mut db = Connection::open_in_memory()?;
        super::super::migrate(&mut db)?;
        let first = save(
            &mut db,
            NoteInput {
                id: None,
                rev: None,
                title: "Cây xanh".into(),
                body: "Mùa xuân".into(),
                tags: vec!["vườn".into()],
                pinned: true,
            },
        )?;
        assert_eq!(list(&db, "xuân")?.len(), 1);
        assert!(save(
            &mut db,
            NoteInput {
                id: Some(first.id.clone()),
                rev: Some(0),
                title: "old".into(),
                body: "".into(),
                tags: vec![],
                pinned: false
            }
        )
        .is_err());
        delete(&mut db, &first.id, first.rev)?;
        assert!(list(&db, "xuân")?.is_empty());
        let tombstone: String = db.query_row(
            "SELECT data FROM outbox WHERE id=?1 ORDER BY rev DESC LIMIT 1",
            [&first.id],
            |r| r.get(0),
        )?;
        let parsed: Note = serde_json::from_str(&tombstone)?;
        assert!(parsed.deleted_at.is_some());
        assert_eq!(parsed.rev, 2);
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM activity", [], |r| r.get::<_, i64>(0))?,
            2
        );
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
            4
        );
        Ok(())
    }
}

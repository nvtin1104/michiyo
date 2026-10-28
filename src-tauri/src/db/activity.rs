use super::{device_id, now, DbResult};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Activity {
    pub id: String,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
    pub device_id: String,
    pub rev: i64,
    pub kind: String,
    pub occurred_at: i64,
    pub data: Value,
}
fn row_activity(row: &rusqlite::Row<'_>) -> rusqlite::Result<Activity> {
    let value: String = row.get(5)?;
    let data: Value = serde_json::from_str(&value).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(Activity {
        id: row.get(0)?,
        updated_at: row.get(1)?,
        deleted_at: row.get(2)?,
        device_id: row.get(3)?,
        rev: row.get(4)?,
        kind: data["kind"].as_str().unwrap_or_default().into(),
        occurred_at: data["occurred_at"].as_i64().unwrap_or(0),
        data: data["data"].clone(),
    })
}
pub fn list(
    db: &Connection,
    kind: Option<&str>,
    from: Option<i64>,
    until: Option<i64>,
) -> DbResult<Vec<Activity>> {
    let mut stmt = db.prepare("SELECT id, updated_at, deleted_at, device_id, rev, data FROM activity WHERE deleted_at IS NULL AND (?1 IS NULL OR json_extract(data, '$.kind') = ?1) AND (?2 IS NULL OR CAST(json_extract(data, '$.occurred_at') AS INTEGER) >= ?2) AND (?3 IS NULL OR CAST(json_extract(data, '$.occurred_at') AS INTEGER) < ?3) ORDER BY CAST(json_extract(data, '$.occurred_at') AS INTEGER) DESC, id DESC LIMIT 1000")?;
    let rows = stmt
        .query_map(params![kind, from, until], row_activity)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}
pub fn record(db: &mut Connection, kind: &str, data: Value) -> DbResult<()> {
    let tx = db.transaction()?;
    let timestamp = now();
    let item = Activity {
        id: ulid::Ulid::generate().to_string(),
        updated_at: timestamp,
        deleted_at: None,
        device_id: device_id(&tx)?,
        rev: 1,
        kind: kind.into(),
        occurred_at: timestamp,
        data,
    };
    let payload =
        serde_json::json!({"kind": item.kind, "occurred_at": item.occurred_at, "data": item.data});
    tx.execute("INSERT INTO activity (id, updated_at, deleted_at, device_id, rev, data) VALUES (?1, ?2, NULL, ?3, 1, ?4)", params![item.id, item.updated_at, item.device_id, payload.to_string()])?;
    tx.execute(
        "INSERT INTO outbox (id, kind, rev, data) VALUES (?1, 'activity', 1, ?2)",
        params![item.id, serde_json::to_string(&item)?],
    )?;
    tx.commit()?;
    Ok(())
}
pub fn delete(db: &mut Connection, ids: &[String]) -> DbResult<()> {
    if ids.len() > 1000 {
        return Err("Quá nhiều mục lịch sử".into());
    }
    let tx = db.transaction()?;
    let device = device_id(&tx)?;
    for id in ids {
        let item = tx.query_row("SELECT id, updated_at, deleted_at, device_id, rev, data FROM activity WHERE id=?1 AND deleted_at IS NULL", [id], row_activity).optional()?;
        if let Some(mut item) = item {
            item.updated_at = now();
            item.deleted_at = Some(item.updated_at);
            item.rev += 1;
            item.device_id = device.clone();
            tx.execute("UPDATE activity SET updated_at=?1, deleted_at=?1, device_id=?2, rev=?3 WHERE id=?4", params![item.updated_at, item.device_id, item.rev, item.id])?;
            tx.execute(
                "INSERT INTO outbox (id, kind, rev, data) VALUES (?1, 'activity', ?2, ?3)",
                params![item.id, item.rev, serde_json::to_string(&item)?],
            )?;
        }
    }
    tx.commit()?;
    Ok(())
}
pub fn delete_range(db: &mut Connection, from: i64, until: i64) -> DbResult<usize> {
    if from >= until {
        return Err("Khoảng ngày không hợp lệ".into());
    }
    let tx = db.transaction()?;
    let ids: Vec<String> = {
        let mut stmt = tx.prepare("SELECT id FROM activity WHERE deleted_at IS NULL AND CAST(json_extract(data, '$.occurred_at') AS INTEGER) >= ?1 AND CAST(json_extract(data, '$.occurred_at') AS INTEGER) < ?2")?;
        let rows = stmt
            .query_map(params![from, until], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        rows
    };
    let device = device_id(&tx)?;
    for id in &ids {
        let mut item = tx.query_row(
            "SELECT id, updated_at, deleted_at, device_id, rev, data FROM activity WHERE id=?1",
            [id],
            row_activity,
        )?;
        item.updated_at = now();
        item.deleted_at = Some(item.updated_at);
        item.rev += 1;
        item.device_id = device.clone();
        tx.execute(
            "UPDATE activity SET updated_at=?1, deleted_at=?1, device_id=?2, rev=?3 WHERE id=?4",
            params![item.updated_at, item.device_id, item.rev, item.id],
        )?;
        tx.execute(
            "INSERT INTO outbox (id, kind, rev, data) VALUES (?1, 'activity', ?2, ?3)",
            params![item.id, item.rev, serde_json::to_string(&item)?],
        )?;
    }
    tx.commit()?;
    Ok(ids.len())
}
use rusqlite::OptionalExtension;
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::notes::{self, NoteInput};
    #[test]
    fn record_tracks_metadata_without_chat_content() -> DbResult<()> {
        let mut db = Connection::open_in_memory()?;
        super::super::migrate(&mut db)?;
        record(&mut db, "chat", serde_json::json!({}))?;
        let rows = list(&db, Some("chat"), None, None)?;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].data, serde_json::json!({}));
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
            1
        );
        Ok(())
    }
    #[test]
    fn range_deletes_all_matching_items_and_preserves_outside() -> DbResult<()> {
        let mut db = Connection::open_in_memory()?;
        super::super::migrate(&mut db)?;
        for i in 0..3 {
            db.execute("INSERT INTO activity(id, updated_at, deleted_at, device_id, rev, data) VALUES (?1, ?2, NULL, 'device', 1, ?3)", params![format!("item-{i}"), i, serde_json::json!({"kind":"test", "occurred_at": i, "data":{}}).to_string()])?;
        }
        assert_eq!(delete_range(&mut db, 0, 2)?, 2);
        assert_eq!(list(&db, None, None, None)?.len(), 1);
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
            2
        );
        assert!(delete_range(&mut db, 2, 2).is_err());
        Ok(())
    }
    #[test]
    fn filter_and_delete_preserve_tombstones() -> DbResult<()> {
        let mut db = Connection::open_in_memory()?;
        super::super::migrate(&mut db)?;
        notes::save(
            &mut db,
            NoteInput {
                id: None,
                rev: None,
                title: "Note".into(),
                body: "".into(),
                tags: vec![],
                pinned: false,
            },
        )?;
        let rows = list(&db, Some("note_saved"), None, None)?;
        assert_eq!(rows.len(), 1);
        assert!(list(&db, Some("note_deleted"), None, None)?.is_empty());
        delete(&mut db, &[rows[0].id.clone()])?;
        assert!(list(&db, None, None, None)?.is_empty());
        let data: String = db.query_row(
            "SELECT data FROM outbox WHERE id=?1 ORDER BY rev DESC LIMIT 1",
            [&rows[0].id],
            |r| r.get(0),
        )?;
        assert!(serde_json::from_str::<Activity>(&data)?
            .deleted_at
            .is_some());
        Ok(())
    }
}

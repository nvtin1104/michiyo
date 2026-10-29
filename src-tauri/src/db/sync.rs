//! Local half of synchronisation: which outbox rows to push, and how remote records merge in.
//! Conflict rule (identical on the server): the record with the larger `(updated_at, device_id)` wins.
use super::{device_id, now, DbResult};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use ulid::Ulid;

pub const PUSH_BATCH: usize = 500;
const META: [&str; 5] = ["id", "updated_at", "deleted_at", "device_id", "rev"];

fn table(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "note" => "notes",
        "habit" => "habits",
        "habit_log" => "habit_logs",
        "event" => "events",
        "reminder" => "reminders",
        "activity" => "activity",
        _ => return None,
    })
}

/// Record kinds covered by the enabled scopes (`notes`, `habits`, `calendar`, `activity`; all on by default).
pub fn kinds_for(scopes: &Value) -> Vec<&'static str> {
    let on = |name: &str| scopes[name].as_bool().unwrap_or(true);
    let mut kinds = Vec::new();
    if on("notes") {
        kinds.push("note");
    }
    if on("habits") {
        kinds.extend(["habit", "habit_log"]);
    }
    if on("calendar") {
        kinds.extend(["event", "reminder"]);
    }
    if on("activity") {
        kinds.push("activity");
    }
    kinds
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ref {
    pub kind: String,
    pub id: String,
    pub rev: i64,
}

#[derive(Debug, Clone)]
pub struct PushItem {
    pub reference: Ref,
    pub record: Value,
}

fn placeholders(n: usize) -> String {
    vec!["?"; n].join(",")
}

pub fn pending_count(db: &Connection, kinds: &[&str]) -> DbResult<i64> {
    if kinds.is_empty() {
        return Ok(0);
    }
    let sql = format!("SELECT COUNT(*) FROM (SELECT 1 FROM outbox WHERE kind IN ({}) GROUP BY kind, id)", placeholders(kinds.len()));
    Ok(db.query_row(&sql, rusqlite::params_from_iter(kinds), |r| r.get(0))?)
}

/// Latest queued revision of each record, oldest queue position first.
pub fn collect_push(db: &Connection, kinds: &[&str]) -> DbResult<Vec<PushItem>> {
    if kinds.is_empty() {
        return Ok(vec![]);
    }
    let sql = format!(
        "SELECT o.kind, o.id, o.rev, o.data FROM outbox o WHERE o.kind IN ({}) AND o.rev = (SELECT MAX(rev) FROM outbox WHERE kind = o.kind AND id = o.id) ORDER BY o.rowid LIMIT {PUSH_BATCH}",
        placeholders(kinds.len())
    );
    let mut stmt = db.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(kinds), |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, String>(3)?))
    })?;
    let mut items = Vec::new();
    for row in rows {
        let (kind, id, rev, data) = row?;
        items.push(PushItem { reference: Ref { kind, id, rev }, record: serde_json::from_str(&data)? });
    }
    Ok(items)
}

/// The server stored these (or already had them): drop every queued revision up to the one sent.
pub fn mark_pushed(db: &mut Connection, accepted: &[Ref]) -> DbResult<()> {
    let tx = db.transaction()?;
    for r in accepted {
        tx.execute("DELETE FROM outbox WHERE kind = ?1 AND id = ?2 AND rev <= ?3", params![r.kind, r.id, r.rev])?;
    }
    tx.commit()?;
    Ok(())
}

fn upsert_note_fts(tx: &Transaction<'_>, id: &str, data: &Value, deleted: bool) -> DbResult<()> {
    tx.execute("DELETE FROM notes_fts WHERE id = ?1", [id])?;
    if !deleted {
        tx.execute(
            "INSERT INTO notes_fts (id, title, body) VALUES (?1, ?2, ?3)",
            params![id, data["title"].as_str().unwrap_or_default(), data["body"].as_str().unwrap_or_default()],
        )?;
    }
    Ok(())
}

/// Keeps the losing version of a note as a new note instead of discarding it.
fn conflict_copy(tx: &Transaction<'_>, id: &str) -> DbResult<bool> {
    let local: Option<(Option<i64>, String)> = tx
        .query_row("SELECT deleted_at, data FROM notes WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    let Some((None, data)) = local else { return Ok(false) };
    let data: Value = serde_json::from_str(&data)?;
    let title: String = format!("{} (xung đột)", data["title"].as_str().unwrap_or_default()).chars().take(200).collect();
    let copy_id = Ulid::generate().to_string();
    let (device, at) = (device_id(tx)?, now());
    let copy = json!({"title": title, "body": data["body"], "tags": data["tags"], "pinned": false});
    tx.execute(
        "INSERT INTO notes (id, updated_at, deleted_at, device_id, rev, data) VALUES (?1, ?2, NULL, ?3, 1, ?4)",
        params![copy_id, at, device, copy.to_string()],
    )?;
    upsert_note_fts(tx, &copy_id, &copy, false)?;
    let mut record = copy.as_object().cloned().unwrap_or_default();
    record.extend([("id".into(), copy_id.clone().into()), ("updated_at".into(), at.into()), ("deleted_at".into(), Value::Null), ("device_id".into(), device.into()), ("rev".into(), 1.into())]);
    tx.execute("INSERT INTO outbox (id, kind, rev, data) VALUES (?1, 'note', 1, ?2)", params![copy_id, Value::Object(record).to_string()])?;
    Ok(true)
}

/// The server preferred another device's write for these: keep notes as conflict copies, then let the pull deliver the winner.
pub fn handle_rejected(db: &mut Connection, rejected: &[Ref]) -> DbResult<usize> {
    let tx = db.transaction()?;
    let mut copies = 0;
    for r in rejected {
        if r.kind == "note" && conflict_copy(&tx, &r.id)? {
            copies += 1;
        }
        tx.execute("DELETE FROM outbox WHERE kind = ?1 AND id = ?2 AND rev <= ?3", params![r.kind, r.id, r.rev])?;
    }
    tx.commit()?;
    Ok(copies)
}

#[derive(Debug, Default, PartialEq)]
pub struct Applied {
    pub kinds: BTreeSet<String>,
    pub applied: usize,
    pub conflicts: usize,
}

fn wins(remote: (i64, &str), local: (i64, &str)) -> bool {
    remote > local
}

/// Merges records pulled from the server. Records with no queued local change simply follow LWW;
/// a queued local change that loses is preserved (notes) before the remote version replaces it.
pub fn apply_remote(db: &mut Connection, records: &[(String, Value)], kinds: &[&str]) -> DbResult<Applied> {
    let tx = db.transaction()?;
    let mut report = Applied::default();
    for (kind, record) in records {
        let (Some(table), true) = (table(kind), kinds.contains(&kind.as_str())) else { continue };
        let Some(object) = record.as_object() else { continue };
        let (Some(id), Some(updated_at), Some(device), Some(rev)) =
            (object.get("id").and_then(Value::as_str), object.get("updated_at").and_then(Value::as_i64), object.get("device_id").and_then(Value::as_str), object.get("rev").and_then(Value::as_i64))
        else {
            continue;
        };
        if id.is_empty() || id.len() > 64 || device.is_empty() {
            continue;
        }
        let deleted_at = object.get("deleted_at").and_then(Value::as_i64);
        let local: Option<(i64, String, i64, Option<i64>, String)> = tx
            .query_row(&format!("SELECT updated_at, device_id, rev, deleted_at, data FROM {table} WHERE id = ?1"), [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .optional()?;
        let queued: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM outbox WHERE kind = ?1 AND id = ?2)", params![kind, id], |r| r.get(0))?;
        let data: Map<String, Value> = object.iter().filter(|(k, _)| !META.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect();
        let data = Value::Object(data);
        // Once a rejected write is removed from the outbox, the server is authoritative
        // for an equal timestamp/device tuple. Otherwise a same-millisecond edit can
        // survive locally after its conflict copy was created and never converge.
        let remote_wins = local.as_ref().is_none_or(|(at, dev, local_rev, local_deleted, local_data)| {
            wins((updated_at, device), (*at, dev)) ||
                (!queued && updated_at == *at && device == dev &&
                    (rev != *local_rev || deleted_at != *local_deleted || serde_json::from_str::<Value>(local_data).ok().as_ref() != Some(&data)))
        });
        if !remote_wins {
            continue;
        }
        if queued {
            if kind == "note" && conflict_copy(&tx, id)? {
                report.conflicts += 1;
            }
            tx.execute("DELETE FROM outbox WHERE kind = ?1 AND id = ?2", params![kind, id])?;
        }
        tx.execute(
            &format!("INSERT INTO {table} (id, updated_at, deleted_at, device_id, rev, data) VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT(id) DO UPDATE SET updated_at=excluded.updated_at, deleted_at=excluded.deleted_at, device_id=excluded.device_id, rev=excluded.rev, data=excluded.data"),
            params![id, updated_at, deleted_at, device, rev, data.to_string()],
        )?;
        if kind == "note" {
            upsert_note_fts(&tx, id, &data, deleted_at.is_some())?;
        }
        report.kinds.insert(kind.clone());
        report.applied += 1;
    }
    tx.commit()?;
    Ok(report)
}

pub fn state(db: &Connection, key: &str) -> DbResult<Option<String>> {
    Ok(db.query_row("SELECT value FROM sync_state WHERE key = ?1", [key], |r| r.get(0)).optional()?)
}

pub fn set_state(db: &Connection, key: &str, value: &str) -> DbResult<()> {
    db.execute("INSERT INTO sync_state (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value", params![key, value])?;
    Ok(())
}

/// Pull cursor for the current scope set. Changing scopes restarts from 0 (applying is idempotent),
/// so records of a newly enabled scope arrive even though the cursor had moved past them.
pub fn cursor(db: &Connection, kinds: &[&str]) -> DbResult<String> {
    let signature = kinds.join(",");
    if state(db, "pull_scopes")?.as_deref() != Some(&signature) {
        set_state(db, "pull_scopes", &signature)?;
        set_state(db, "pull_cursor", "0")?;
    }
    Ok(state(db, "pull_cursor")?.unwrap_or_else(|| "0".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{events, habits, notes};
    use std::collections::HashMap;

    /// Same rules as cloud/src/sync.ts.
    #[derive(Default)]
    struct Server {
        seq: i64,
        rows: HashMap<(String, String), (i64, Value)>,
    }
    impl Server {
        fn push(&mut self, items: &[PushItem]) -> (Vec<Ref>, Vec<Ref>) {
            let (mut accepted, mut rejected) = (vec![], vec![]);
            for item in items {
                let key = (item.reference.kind.clone(), item.reference.id.clone());
                let tuple = |v: &Value| (v["updated_at"].as_i64().unwrap(), v["device_id"].as_str().unwrap().to_string());
                let ok = match self.rows.get(&key) {
                    Some((_, current)) if tuple(&item.record) > tuple(current) => true,
                    Some((_, current)) => tuple(&item.record) == tuple(current),
                    None => true,
                };
                let stored_new = self.rows.get(&key).is_none_or(|(_, c)| tuple(&item.record) > tuple(c));
                if ok && stored_new {
                    self.seq += 1;
                    self.rows.insert(key, (self.seq, item.record.clone()));
                }
                (if ok { &mut accepted } else { &mut rejected }).push(item.reference.clone());
            }
            (accepted, rejected)
        }
        fn pull(&self, cursor: i64) -> (Vec<(String, Value)>, i64) {
            let mut rows: Vec<_> = self.rows.iter().filter(|(_, (s, _))| *s > cursor).collect();
            rows.sort_by_key(|(_, (s, _))| *s);
            let last = rows.last().map_or(cursor, |(_, (s, _))| *s);
            (rows.into_iter().map(|((k, _), (_, v))| (k.clone(), v.clone())).collect(), last)
        }
    }

    fn device() -> Connection {
        let mut db = Connection::open_in_memory().unwrap();
        super::super::migrate(&mut db).unwrap();
        db
    }
    fn round(db: &mut Connection, server: &mut Server, kinds: &[&str]) -> DbResult<Applied> {
        let items = collect_push(db, kinds)?;
        let (accepted, rejected) = server.push(&items);
        mark_pushed(db, &accepted)?;
        handle_rejected(db, &rejected)?;
        let cursor: i64 = cursor(db, kinds)?.parse()?;
        let (records, last) = server.pull(cursor);
        let report = apply_remote(db, &records, kinds)?;
        set_state(db, "pull_cursor", &last.to_string())?;
        Ok(report)
    }
    fn add_note(db: &mut Connection, title: &str, body: &str) -> notes::Note {
        notes::save(db, notes::NoteInput { id: None, rev: None, title: title.into(), body: body.into(), tags: vec![], pinned: false }).unwrap()
    }
    fn edit_note(db: &mut Connection, n: &notes::Note, body: &str) -> notes::Note {
        notes::save(db, notes::NoteInput { id: Some(n.id.clone()), rev: Some(n.rev), title: n.title.clone(), body: body.into(), tags: vec![], pinned: false }).unwrap()
    }
    fn first_note(db: &Connection) -> notes::Note {
        notes::list(db, "").unwrap().remove(0)
    }
    const ALL: [&str; 6] = ["note", "habit", "habit_log", "event", "reminder", "activity"];
    fn tick() {
        std::thread::sleep(std::time::Duration::from_millis(3));
    }

    #[test]
    fn two_devices_converge_and_search_works_on_the_receiver() -> DbResult<()> {
        let (mut a, mut b, mut server) = (device(), device(), Server::default());
        let note = add_note(&mut a, "Cây xanh", "mùa xuân");
        assert!(pending_count(&a, &ALL)? > 0);
        round(&mut a, &mut server, &ALL)?;
        assert_eq!(pending_count(&a, &ALL)?, 0);
        let report = round(&mut b, &mut server, &ALL)?;
        assert!(report.kinds.contains("note") && report.kinds.contains("activity"));
        assert_eq!(notes::list(&b, "xuân")?.len(), 1);
        // The receiver queues nothing for what it merely received.
        assert_eq!(pending_count(&b, &ALL)?, 0);
        tick();
        let edited = { let n = first_note(&b); edit_note(&mut b, &n, "mùa hè") };
        assert_eq!(edited.rev, 2);
        round(&mut b, &mut server, &ALL)?;
        round(&mut a, &mut server, &ALL)?;
        let on_a = notes::list(&a, "hè")?;
        assert_eq!(on_a.len(), 1);
        assert_eq!(on_a[0].id, note.id);
        assert!(notes::list(&a, "xuân")?.is_empty());
        // Deleting propagates as a tombstone and leaves the search index.
        tick();
        notes::delete(&mut a, &on_a[0].id, on_a[0].rev)?;
        round(&mut a, &mut server, &ALL)?;
        round(&mut b, &mut server, &ALL)?;
        assert!(notes::list(&b, "")?.is_empty());
        Ok(())
    }

    #[test]
    fn concurrent_note_edits_keep_the_loser_as_a_conflict_copy() -> DbResult<()> {
        let (mut a, mut b, mut server) = (device(), device(), Server::default());
        add_note(&mut a, "Kế hoạch", "gốc");
        round(&mut a, &mut server, &ALL)?;
        round(&mut b, &mut server, &ALL)?;
        tick();
        let n = first_note(&a);
        edit_note(&mut a, &n, "bản của A");
        tick();
        let n = first_note(&b);
        edit_note(&mut b, &n, "bản của B");
        round(&mut b, &mut server, &ALL)?; // B is newer and reaches the server first
        round(&mut a, &mut server, &ALL)?; // A's older edit is rejected
        round(&mut b, &mut server, &ALL)?;
        round(&mut a, &mut server, &ALL)?;
        round(&mut b, &mut server, &ALL)?;
        for db in [&a, &b] {
            let all = notes::list(db, "")?;
            assert_eq!(all.len(), 2);
            let winner = all.iter().find(|n| n.title == "Kế hoạch").unwrap();
            assert_eq!(winner.body, "bản của B");
            let copy = all.iter().find(|n| n.title == "Kế hoạch (xung đột)").unwrap();
            assert_eq!(copy.body, "bản của A");
        }
        assert_eq!(pending_count(&a, &ALL)? + pending_count(&b, &ALL)?, 0);
        Ok(())
    }

    #[test]
    fn queued_local_change_versus_incoming_record() -> DbResult<()> {
        let mut a = device();
        let local = add_note(&mut a, "Ghi chú", "cục bộ");
        let remote = |at: i64, body: &str| {
            ("note".to_string(), json!({"id": local.id, "updated_at": at, "deleted_at": null, "device_id": "ZZZ", "rev": 5, "title": "Ghi chú", "body": body, "tags": [], "pinned": false}))
        };
        // Older remote record: the queued local edit wins and stays queued.
        let report = apply_remote(&mut a, &[remote(1, "cũ")], &["note"])?;
        assert_eq!(report.applied, 0);
        assert_eq!(notes::list(&a, "")?[0].body, "cục bộ");
        assert!(pending_count(&a, &["note"])? > 0);
        // Newer remote record: it replaces the local one, which survives as a conflict copy; queue cleared for the original.
        let report = apply_remote(&mut a, &[remote(i64::MAX / 2, "mới")], &["note"])?;
        assert_eq!((report.applied, report.conflicts), (1, 1));
        let all = notes::list(&a, "")?;
        assert_eq!(all.iter().find(|n| n.id == local.id).unwrap().body, "mới");
        assert_eq!(all.iter().find(|n| n.title.ends_with("(xung đột)")).unwrap().body, "cục bộ");
        let queued: Vec<String> = collect_push(&a, &["note"])?.into_iter().map(|i| i.reference.id).collect();
        assert!(!queued.contains(&local.id) && queued.len() == 1);
        // Replaying the same record changes nothing.
        assert_eq!(apply_remote(&mut a, &[remote(i64::MAX / 2, "mới")], &["note"])?.applied, 0);
        Ok(())
    }

    #[test]
    fn rejected_same_timestamp_edit_converges_to_server_and_keeps_copy() -> DbResult<()> {
        let mut db = device();
        let original = add_note(&mut db, "Ghi chú", "trên server");
        let server_record = serde_json::to_value(&original)?;
        let edited = edit_note(&mut db, &original, "bản sửa") ;
        db.execute("UPDATE notes SET updated_at = ?1 WHERE id = ?2", params![original.updated_at, original.id])?;
        handle_rejected(&mut db, &[Ref { kind: "note".into(), id: original.id.clone(), rev: edited.rev }])?;
        let remote = vec![("note".to_string(), server_record)];
        assert_eq!(apply_remote(&mut db, &remote, &["note"])?.applied, 1);
        let notes = notes::list(&db, "")?;
        assert_eq!(notes.iter().find(|n| n.id == original.id).unwrap().body, "trên server");
        assert!(notes.iter().any(|n| n.title.ends_with("(xung đột)") && n.body == "bản sửa"));
        assert_eq!(apply_remote(&mut db, &remote, &["note"])?.applied, 0);
        Ok(())
    }

    #[test]
    fn habits_and_events_sync_and_scopes_filter_both_directions() -> DbResult<()> {
        let (mut a, mut b, mut server) = (device(), device(), Server::default());
        let habit = habits::save(&mut a, habits::HabitInput { id: None, rev: None, title: "Đọc sách".into(), frequency: "daily".into() })?;
        habits::check_in(&mut a, &habit.id, "2026-10-28", true)?;
        events::save(&mut a, events::EventInput { id: None, rev: None, title: "Họp".into(), kind: "event".into(), starts_at: 1_800_000_000_000, ends_at: None, rrule: None, done: false, remind_before: Some(10) })?;
        add_note(&mut a, "Riêng tư", "x");

        let only_calendar = kinds_for(&json!({"notes": false, "habits": false, "activity": false}));
        assert_eq!(only_calendar, vec!["event", "reminder"]);
        round(&mut a, &mut server, &only_calendar)?;
        assert!(server.rows.keys().all(|(k, _)| k == "event" || k == "reminder"));
        assert!(pending_count(&a, &["note"])? > 0, "out-of-scope changes stay queued");

        round(&mut b, &mut server, &ALL)?;
        let window = events::range(&b, 1_799_000_000_000, 1_801_000_000_000, 0)?;
        assert_eq!((window.len(), window[0].remind_before), (1, Some(10)));
        assert!(habits::list(&b, "2026-10-28")?.is_empty());

        // Enabling more scopes later sends the queued data and restarts the pull.
        round(&mut a, &mut server, &ALL)?;
        round(&mut b, &mut server, &ALL)?;
        let views = habits::list(&b, "2026-10-28")?;
        assert_eq!((views.len(), views[0].streak, views[0].done_today), (1, 1, true));
        assert_eq!(notes::list(&b, "")?.len(), 1);
        Ok(())
    }

    #[test]
    fn cursor_restarts_when_scopes_change() -> DbResult<()> {
        let db = device();
        assert_eq!(cursor(&db, &["note"])?, "0");
        set_state(&db, "pull_cursor", "42")?;
        assert_eq!(cursor(&db, &["note"])?, "42");
        assert_eq!(cursor(&db, &["note", "event"])?, "0");
        Ok(())
    }

    #[test]
    fn push_batches_use_only_the_latest_revision() -> DbResult<()> {
        let mut a = device();
        let n = add_note(&mut a, "t", "1");
        let n = edit_note(&mut a, &n, "2");
        let items = collect_push(&a, &["note"])?;
        assert_eq!(items.len(), 1);
        assert_eq!((items[0].reference.rev, items[0].record["body"].as_str()), (2, Some("2")));
        mark_pushed(&mut a, &[items[0].reference.clone()])?;
        assert_eq!(pending_count(&a, &["note"])?, 0);
        // A write that lands between collect and mark is not lost.
        let n = edit_note(&mut a, &n, "3");
        mark_pushed(&mut a, &[Ref { kind: "note".into(), id: n.id.clone(), rev: 2 }])?;
        assert_eq!(collect_push(&a, &["note"])?[0].reference.rev, 3);
        Ok(())
    }
}

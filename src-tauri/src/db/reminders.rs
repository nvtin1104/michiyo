use super::events::{self, Event};
use super::{device_id, now, DbResult};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::Serialize;
use std::collections::HashMap;
use ulid::Ulid;

/// An occurrence that started longer ago than this (e.g. the app was closed) is not announced.
const GRACE_MS: i64 = 15 * 60_000;
pub const MAX_MINUTES_BEFORE: i64 = 7 * 24 * 60;

#[derive(Debug, Clone, PartialEq)]
struct Reminder {
    id: String,
    rev: i64,
    event_id: String,
    minutes_before: i64,
    /// Start of the last occurrence announced.
    fired_for: Option<i64>,
    /// Announced but not yet dismissed or snoozed.
    pending_start: Option<i64>,
    snooze_until: Option<i64>,
    snooze_start: Option<i64>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Pending {
    pub reminder_id: String,
    pub event_id: String,
    pub title: String,
    pub kind: String,
    pub occurrence_start: i64,
}

fn parse(row: &rusqlite::Row<'_>) -> rusqlite::Result<(Reminder, Option<i64>)> {
    let data: String = row.get(3)?;
    let data: serde_json::Value = serde_json::from_str(&data).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok((
        Reminder {
            id: row.get(0)?,
            rev: row.get(1)?,
            event_id: data["event_id"].as_str().unwrap_or_default().into(),
            minutes_before: data["minutes_before"].as_i64().unwrap_or(0),
            fired_for: data["fired_for"].as_i64(),
            pending_start: data["pending_start"].as_i64(),
            snooze_until: data["snooze_until"].as_i64(),
            snooze_start: data["snooze_start"].as_i64(),
        },
        row.get(2)?,
    ))
}

const SELECT: &str = "SELECT id, rev, deleted_at, data FROM reminders";

fn by_event(tx: &Transaction<'_>, event_id: &str) -> DbResult<Option<(Reminder, Option<i64>)>> {
    Ok(tx
        .query_row(
            &format!("{SELECT} WHERE json_extract(data, '$.event_id') = ?1"),
            [event_id],
            parse,
        )
        .optional()?)
}

fn write(tx: &Transaction<'_>, r: &Reminder, deleted: bool, device: &str, at: i64) -> DbResult<()> {
    let deleted_at = deleted.then_some(at);
    let data = serde_json::json!({"event_id": r.event_id, "minutes_before": r.minutes_before, "fired_for": r.fired_for, "pending_start": r.pending_start, "snooze_until": r.snooze_until, "snooze_start": r.snooze_start});
    tx.execute(
        "INSERT INTO reminders (id, updated_at, deleted_at, device_id, rev, data) VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT(id) DO UPDATE SET updated_at=excluded.updated_at, deleted_at=excluded.deleted_at, device_id=excluded.device_id, rev=excluded.rev, data=excluded.data",
        params![r.id, at, deleted_at, device, r.rev, data.to_string()],
    )?;
    let mut full = data;
    full["id"] = r.id.clone().into();
    full["updated_at"] = at.into();
    full["deleted_at"] = deleted_at.into();
    full["device_id"] = device.into();
    full["rev"] = r.rev.into();
    tx.execute(
        "INSERT INTO outbox (id, kind, rev, data) VALUES (?1, 'reminder', ?2, ?3)",
        params![r.id, r.rev, full.to_string()],
    )?;
    Ok(())
}

/// Creates, updates or removes the reminder of `event_id`. Changing the event resets fired/snooze state.
pub fn set_for_event(
    tx: &Transaction<'_>,
    event_id: &str,
    minutes_before: Option<i64>,
    device: &str,
    at: i64,
) -> DbResult<()> {
    let existing = by_event(tx, event_id)?;
    match (existing, minutes_before) {
        (None, None) | (Some((_, Some(_))), None) => {}
        (Some((mut r, None)), None) => {
            r.rev += 1;
            write(tx, &r, true, device, at)?;
        }
        (existing, Some(minutes)) => {
            let mut r = existing.map(|(r, _)| r).unwrap_or(Reminder {
                id: Ulid::generate().to_string(),
                rev: 0,
                event_id: event_id.into(),
                minutes_before: minutes,
                fired_for: None,
                pending_start: None,
                snooze_until: None,
                snooze_start: None,
            });
            r.rev += 1;
            r.minutes_before = minutes;
            (r.fired_for, r.pending_start, r.snooze_until, r.snooze_start) =
                (None, None, None, None);
            write(tx, &r, false, device, at)?;
        }
    }
    Ok(())
}

/// Live reminder lead times, keyed by event id.
pub fn minutes_by_event(db: &Connection) -> DbResult<HashMap<String, i64>> {
    let mut stmt = db.prepare(&format!("{SELECT} WHERE deleted_at IS NULL"))?;
    let mut map = HashMap::new();
    for row in stmt.query_map([], parse)? {
        let (r, _) = row?;
        map.insert(r.event_id, r.minutes_before);
    }
    Ok(map)
}

fn live(db: &Connection) -> DbResult<Vec<Reminder>> {
    let mut stmt = db.prepare(&format!("{SELECT} WHERE deleted_at IS NULL"))?;
    let rows = stmt.query_map([], parse)?;
    Ok(rows
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|(r, _)| r)
        .collect())
}

fn live_event(db: &Connection, id: &str) -> DbResult<Option<Event>> {
    Ok(events::find_live(db, id)?)
}

fn pending_of(r: &Reminder, e: &Event, start: i64) -> Pending {
    Pending {
        reminder_id: r.id.clone(),
        event_id: e.id.clone(),
        title: e.title.clone(),
        kind: e.kind.clone(),
        occurrence_start: start,
    }
}

/// Announces every reminder that has become due at `at` and marks it pending. Each occurrence is
/// announced once; a snoozed one is announced again when its snooze ends.
pub fn fire_due(db: &mut Connection, at: i64, tz_offset: i64) -> DbResult<Vec<Pending>> {
    let candidates = live(db)?;
    let mut fired = Vec::new();
    let tx = db.transaction()?;
    let device = device_id(&tx)?;
    for mut r in candidates {
        let Some(event) = live_event(&tx, &r.event_id)? else {
            continue;
        };
        if event.done {
            continue;
        }
        let start = if let (Some(until), Some(start)) = (r.snooze_until, r.snooze_start) {
            if until > at {
                continue;
            }
            r.snooze_until = None;
            r.snooze_start = None;
            Some(start)
        } else if r.pending_start.is_some() {
            continue;
        } else {
            let lead = r.minutes_before * 60_000;
            events::occurrences(&event, at - GRACE_MS, at + lead + 1, tz_offset)
                .into_iter()
                .map(|o| o.occurrence_start)
                .filter(|s| *s >= at - GRACE_MS && s - lead <= at && Some(*s) > r.fired_for)
                .min()
        };
        let Some(start) = start else { continue };
        r.fired_for = Some(r.fired_for.map_or(start, |f| f.max(start)));
        r.pending_start = Some(start);
        r.rev += 1;
        write(&tx, &r, false, &device, at)?;
        fired.push(pending_of(&r, &event, start));
    }
    tx.commit()?;
    Ok(fired)
}

pub fn pending(db: &Connection) -> DbResult<Vec<Pending>> {
    let mut out = Vec::new();
    for r in live(db)? {
        let Some(start) = r.pending_start else {
            continue;
        };
        if let Some(event) = live_event(db, &r.event_id)? {
            out.push(pending_of(&r, &event, start));
        }
    }
    out.sort_by_key(|p| (p.occurrence_start, p.reminder_id.clone()));
    Ok(out)
}

fn resolve(db: &mut Connection, id: &str, snooze_minutes: Option<i64>) -> DbResult<()> {
    let tx = db.transaction()?;
    let device = device_id(&tx)?;
    let (mut r, _) = tx
        .query_row(
            &format!("{SELECT} WHERE id = ?1 AND deleted_at IS NULL"),
            [id],
            parse,
        )
        .optional()?
        .ok_or("Không tìm thấy nhắc nhở")?;
    let start = r
        .pending_start
        .take()
        .ok_or("Nhắc nhở không còn chờ xử lý")?;
    let at = now();
    if let Some(minutes) = snooze_minutes {
        r.snooze_until = Some(at + minutes * 60_000);
        r.snooze_start = Some(start);
    }
    r.rev += 1;
    write(&tx, &r, false, &device, at)?;
    tx.commit()?;
    Ok(())
}

pub fn dismiss(db: &mut Connection, id: &str) -> DbResult<()> {
    resolve(db, id, None)
}

pub fn snooze(db: &mut Connection, id: &str, minutes: i64) -> DbResult<()> {
    if !(1..=1440).contains(&minutes) {
        return Err("Thời gian hoãn không hợp lệ".into());
    }
    resolve(db, id, Some(minutes))
}

#[cfg(test)]
mod tests {
    use super::super::events::{save, EventInput};
    use super::*;

    const MIN: i64 = 60_000;
    const T0: i64 = 1_800_000_000_000; // arbitrary instant

    fn db() -> Connection {
        let mut db = Connection::open_in_memory().unwrap();
        super::super::migrate(&mut db).unwrap();
        db
    }
    fn add(
        db: &mut Connection,
        kind: &str,
        start: i64,
        rrule: Option<&str>,
        before: Option<i64>,
    ) -> Event {
        save(
            db,
            EventInput {
                id: None,
                rev: None,
                title: "Họp".into(),
                kind: kind.into(),
                starts_at: start,
                ends_at: None,
                rrule: rrule.map(str::to_string),
                done: false,
                remind_before: before,
            },
        )
        .unwrap()
    }

    #[test]
    fn fires_once_within_lead_time_then_dismisses() -> DbResult<()> {
        let mut db = db();
        add(&mut db, "event", T0 + 60 * MIN, None, Some(10));
        assert!(fire_due(&mut db, T0 + 49 * MIN, 0)?.is_empty());
        let fired = fire_due(&mut db, T0 + 50 * MIN, 0)?;
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].occurrence_start, T0 + 60 * MIN);
        assert!(fire_due(&mut db, T0 + 51 * MIN, 0)?.is_empty());
        assert_eq!(pending(&db)?.len(), 1);
        dismiss(&mut db, &fired[0].reminder_id)?;
        assert!(pending(&db)?.is_empty());
        assert!(dismiss(&mut db, &fired[0].reminder_id).is_err());
        assert!(fire_due(&mut db, T0 + 61 * MIN, 0)?.is_empty());
        Ok(())
    }

    #[test]
    fn missed_events_are_not_announced_late() -> DbResult<()> {
        let mut db = db();
        add(&mut db, "event", T0, None, Some(0));
        assert!(fire_due(&mut db, T0 + 16 * MIN, 0)?.is_empty());
        assert_eq!(fire_due(&mut db, T0 + 5 * MIN, 0)?.len(), 1);
        Ok(())
    }

    #[test]
    fn snooze_refires_the_same_occurrence_later() -> DbResult<()> {
        let mut db = db();
        add(&mut db, "event", T0 + 10 * MIN, None, Some(0));
        let id = fire_due(&mut db, T0 + 10 * MIN, 0)?.remove(0).reminder_id;
        assert!(snooze(&mut db, &id, 0).is_err());
        snooze(&mut db, &id, 5)?;
        assert!(pending(&db)?.is_empty());
        // `snooze` uses the wall clock, so move far past it to model the snooze ending.
        let later = now() + 6 * MIN;
        let again = fire_due(&mut db, later, 0)?;
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].occurrence_start, T0 + 10 * MIN);
        assert!(fire_due(&mut db, later + MIN, 0)?.is_empty());
        Ok(())
    }

    #[test]
    fn recurring_announces_each_occurrence_and_skips_done_todos() -> DbResult<()> {
        let mut db = db();
        let day = 86_400_000;
        add(&mut db, "event", T0, Some("FREQ=DAILY"), Some(5));
        let first = fire_due(&mut db, T0 - 5 * MIN, 0)?;
        assert_eq!(first.len(), 1);
        dismiss(&mut db, &first[0].reminder_id)?;
        let next = fire_due(&mut db, T0 + day - 3 * MIN, 0)?;
        assert_eq!(next[0].occurrence_start, T0 + day);

        let todo = add(&mut db, "todo", T0 + 3 * day, None, Some(0));
        events::set_done(&mut db, &todo.id, todo.rev, true)?;
        assert!(fire_due(&mut db, T0 + 3 * day, 0)?
            .iter()
            .all(|p| p.event_id != todo.id));
        Ok(())
    }

    #[test]
    fn removing_the_reminder_or_event_stops_announcements() -> DbResult<()> {
        let mut db = db();
        let e = add(&mut db, "event", T0 + 10 * MIN, None, Some(0));
        assert_eq!(minutes_by_event(&db)?.get(&e.id), Some(&0));
        let e = save(
            &mut db,
            EventInput {
                id: Some(e.id.clone()),
                rev: Some(e.rev),
                title: "Họp".into(),
                kind: "event".into(),
                starts_at: e.starts_at,
                ends_at: None,
                rrule: None,
                done: false,
                remind_before: None,
            },
        )?;
        assert!(minutes_by_event(&db)?.is_empty());
        assert!(fire_due(&mut db, T0 + 10 * MIN, 0)?.is_empty());
        let e = save(
            &mut db,
            EventInput {
                id: Some(e.id.clone()),
                rev: Some(e.rev),
                title: "Họp".into(),
                kind: "event".into(),
                starts_at: e.starts_at,
                ends_at: None,
                rrule: None,
                done: false,
                remind_before: Some(0),
            },
        )?;
        events::delete(&mut db, &e.id, e.rev)?;
        assert!(fire_due(&mut db, T0 + 10 * MIN, 0)?.is_empty());
        assert!(minutes_by_event(&db)?.is_empty());
        let rev: i64 = db.query_row(
            "SELECT MAX(rev) FROM outbox WHERE kind='reminder'",
            [],
            |r| r.get(0),
        )?;
        assert!(rev >= 4);
        Ok(())
    }
}

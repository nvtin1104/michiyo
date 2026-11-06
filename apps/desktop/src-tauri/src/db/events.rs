use super::{device_id, now, DbResult};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

const DAY_MS: i64 = 86_400_000;
const MAX_OCCURRENCES: usize = 1000;
const MAX_STEPS: usize = 200_000;

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

/// 0 = Sunday … 6 = Saturday.
fn weekday(day: i64) -> i64 {
    (day + 4).rem_euclid(7)
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

/// The supported subset of RFC 5545: FREQ, INTERVAL, COUNT, UNTIL and BYDAY (weekly only).
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    freq: Freq,
    interval: i64,
    count: Option<u32>,
    until: Option<i64>,
    by_day: Vec<i64>,
}

impl Rule {
    pub fn parse(text: &str) -> Option<Self> {
        let mut freq = None;
        let (mut interval, mut count, mut until, mut by_day) = (1, None, None, Vec::new());
        for part in text.strip_prefix("RRULE:").unwrap_or(text).split(';') {
            let (key, value) = part.split_once('=')?;
            match key {
                "FREQ" => {
                    freq = Some(match value {
                        "DAILY" => Freq::Daily,
                        "WEEKLY" => Freq::Weekly,
                        "MONTHLY" => Freq::Monthly,
                        "YEARLY" => Freq::Yearly,
                        _ => return None,
                    })
                }
                "INTERVAL" => interval = value.parse().ok().filter(|n| (1..=1000).contains(n))?,
                "COUNT" => count = Some(value.parse().ok().filter(|n| (1..=10_000).contains(n))?),
                "UNTIL" => until = Some(parse_until(value)?),
                "BYDAY" => {
                    for d in value.split(',') {
                        let n = ["SU", "MO", "TU", "WE", "TH", "FR", "SA"]
                            .iter()
                            .position(|x| *x == d)? as i64;
                        if !by_day.contains(&n) {
                            by_day.push(n);
                        }
                    }
                }
                _ => return None,
            }
        }
        let freq = freq?;
        if (!by_day.is_empty() && freq != Freq::Weekly) || (count.is_some() && until.is_some()) {
            return None;
        }
        by_day.sort_by_key(|d| (d + 6) % 7); // Monday first
        Some(Self {
            freq,
            interval,
            count,
            until,
            by_day,
        })
    }
}

fn parse_until(value: &str) -> Option<i64> {
    let digits = |s: &str| -> Option<i64> {
        (s.bytes().all(|b| b.is_ascii_digit()))
            .then(|| s.parse().ok())
            .flatten()
    };
    if value.len() < 8 {
        return None;
    }
    let (y, m, d) = (
        digits(&value[0..4])?,
        digits(&value[4..6])?,
        digits(&value[6..8])?,
    );
    let day = days_from_civil(y, m, d);
    if !(1..=12).contains(&m) || civil_from_days(day) != (y, m, d) {
        return None;
    }
    match value.len() {
        8 => Some((day + 1) * DAY_MS - 1),
        16 if value.as_bytes()[8] == b'T' && value.ends_with('Z') => {
            let (h, mi, s) = (
                digits(&value[9..11])?,
                digits(&value[11..13])?,
                digits(&value[13..15])?,
            );
            (h < 24 && mi < 60 && s < 61).then_some(day * DAY_MS + ((h * 60 + mi) * 60 + s) * 1000)
        }
        _ => None,
    }
}

/// Calls `emit` with each occurrence's local day number in increasing order, starting at `first`,
/// until it returns false or the safety cap is reached.
fn walk(rule: &Rule, first: i64, emit: &mut dyn FnMut(i64) -> bool) {
    let mut steps = 0;
    match rule.freq {
        Freq::Daily => {
            let mut day = first;
            while steps < MAX_STEPS && emit(day) {
                day += rule.interval;
                steps += 1;
            }
        }
        Freq::Weekly => {
            let by_day = if rule.by_day.is_empty() {
                vec![weekday(first)]
            } else {
                rule.by_day.clone()
            };
            let first_monday = (first + 3).div_euclid(7) * 7 - 3;
            let mut monday = first_monday;
            'weeks: while steps < MAX_STEPS {
                for wd in &by_day {
                    let day = monday + (wd + 6) % 7;
                    if day >= first && !emit(day) {
                        break 'weeks;
                    }
                }
                monday += 7 * rule.interval;
                steps += 1;
            }
        }
        Freq::Monthly | Freq::Yearly => {
            let (year, month, dom) = civil_from_days(first);
            let step = if rule.freq == Freq::Monthly {
                rule.interval
            } else {
                rule.interval * 12
            };
            let mut index = year * 12 + month - 1;
            while steps < MAX_STEPS {
                let (y, m) = (index.div_euclid(12), index.rem_euclid(12) + 1);
                let day = days_from_civil(y, m, dom);
                // Months/years that lack this date (31st, 29 Feb) are skipped, per RFC 5545.
                if civil_from_days(day) == (y, m, dom) && !emit(day) {
                    break;
                }
                index += step;
                steps += 1;
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Event {
    pub id: String,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
    pub device_id: String,
    pub rev: i64,
    pub title: String,
    pub kind: String,
    pub starts_at: i64,
    pub ends_at: Option<i64>,
    pub rrule: Option<String>,
    pub done: bool,
}

#[derive(Debug, Deserialize)]
pub struct EventInput {
    pub id: Option<String>,
    pub rev: Option<i64>,
    pub title: String,
    pub kind: String,
    pub starts_at: i64,
    pub ends_at: Option<i64>,
    pub rrule: Option<String>,
    pub done: bool,
    /// Minutes before the start to announce; `None` for no reminder.
    #[serde(default)]
    pub remind_before: Option<i64>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Occurrence {
    #[serde(flatten)]
    pub event: Event,
    pub occurrence_start: i64,
    pub occurrence_end: Option<i64>,
    pub remind_before: Option<i64>,
}

/// Occurrences overlapping `[from, until)`. `tz_offset` is minutes east of UTC; recurrence is
/// evaluated on local calendar days using that fixed offset (daylight-saving shifts are not tracked).
pub fn occurrences(event: &Event, from: i64, until: i64, tz_offset: i64) -> Vec<Occurrence> {
    let duration = event.ends_at.map(|end| end - event.starts_at);
    let make = |start: i64| Occurrence {
        event: event.clone(),
        occurrence_start: start,
        occurrence_end: duration.map(|d| start + d),
        remind_before: None,
    };
    let overlaps = |start: i64| start < until && start + duration.unwrap_or(0).max(1) > from;
    let Some(rule) = event.rrule.as_deref().and_then(Rule::parse) else {
        return if overlaps(event.starts_at) {
            vec![make(event.starts_at)]
        } else {
            vec![]
        };
    };
    let local = event.starts_at + tz_offset * 60_000;
    let first = local.div_euclid(DAY_MS);
    let time_of_day = local.rem_euclid(DAY_MS);
    let mut found = Vec::new();
    let mut produced = 0u32;
    walk(&rule, first, &mut |day| {
        let start = day * DAY_MS + time_of_day - tz_offset * 60_000;
        if rule.count.is_some_and(|c| produced >= c)
            || rule.until.is_some_and(|u| start > u)
            || start >= until
            || found.len() >= MAX_OCCURRENCES
        {
            return false;
        }
        produced += 1;
        if overlaps(start) {
            found.push(make(start));
        }
        true
    });
    found
}

fn bad_column(e: impl std::error::Error + Send + Sync + 'static) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e))
}

fn row_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<Event> {
    let data: String = row.get(5)?;
    let data: serde_json::Value = serde_json::from_str(&data).map_err(bad_column)?;
    Ok(Event {
        id: row.get(0)?,
        updated_at: row.get(1)?,
        deleted_at: row.get(2)?,
        device_id: row.get(3)?,
        rev: row.get(4)?,
        title: data["title"].as_str().unwrap_or_default().to_string(),
        kind: data["kind"].as_str().unwrap_or("event").to_string(),
        starts_at: data["starts_at"].as_i64().unwrap_or(0),
        ends_at: data["ends_at"].as_i64(),
        rrule: data["rrule"].as_str().map(str::to_string),
        done: data["done"].as_bool().unwrap_or(false),
    })
}

pub(super) fn find_live(db: &Connection, id: &str) -> DbResult<Option<Event>> {
    Ok(db
        .query_row(
            "SELECT id, updated_at, deleted_at, device_id, rev, data FROM events WHERE id = ?1 AND deleted_at IS NULL",
            [id],
            row_event,
        )
        .optional()?)
}

fn get(tx: &Transaction<'_>, id: &str) -> DbResult<Option<Event>> {
    Ok(tx
        .query_row(
            "SELECT id, updated_at, deleted_at, device_id, rev, data FROM events WHERE id = ?1",
            [id],
            row_event,
        )
        .optional()?)
}

fn write(tx: &Transaction<'_>, event: &Event, activity: &str) -> DbResult<()> {
    let data = serde_json::json!({"title": event.title, "kind": event.kind, "starts_at": event.starts_at, "ends_at": event.ends_at, "rrule": event.rrule, "done": event.done});
    tx.execute(
        "INSERT INTO events (id, updated_at, deleted_at, device_id, rev, data) VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT(id) DO UPDATE SET updated_at=excluded.updated_at, deleted_at=excluded.deleted_at, device_id=excluded.device_id, rev=excluded.rev, data=excluded.data",
        params![event.id, event.updated_at, event.deleted_at, event.device_id, event.rev, data.to_string()],
    )?;
    tx.execute(
        "INSERT INTO outbox (id, kind, rev, data) VALUES (?1, 'event', ?2, ?3)",
        params![event.id, event.rev, serde_json::to_string(event)?],
    )?;
    let id = Ulid::generate().to_string();
    let detail = serde_json::json!({"event_id": event.id, "title": event.title});
    let stored =
        serde_json::json!({"kind": activity, "occurred_at": event.updated_at, "data": detail});
    tx.execute(
        "INSERT INTO activity (id, updated_at, deleted_at, device_id, rev, data) VALUES (?1, ?2, NULL, ?3, 1, ?4)",
        params![id, event.updated_at, event.device_id, stored.to_string()],
    )?;
    let full = serde_json::json!({"id": id, "updated_at": event.updated_at, "deleted_at": null, "device_id": event.device_id, "rev": 1, "kind": activity, "occurred_at": event.updated_at, "data": detail});
    tx.execute(
        "INSERT INTO outbox (id, kind, rev, data) VALUES (?1, 'activity', 1, ?2)",
        params![id, full.to_string()],
    )?;
    Ok(())
}

/// Expanded occurrences in `[from, until)`, soonest first. The window is limited to 400 days.
pub fn range(db: &Connection, from: i64, until: i64, tz_offset: i64) -> DbResult<Vec<Occurrence>> {
    if until <= from || until - from > 400 * DAY_MS || tz_offset.abs() > 14 * 60 {
        return Err("Khoảng thời gian không hợp lệ".into());
    }
    let mut stmt = db.prepare("SELECT id, updated_at, deleted_at, device_id, rev, data FROM events WHERE deleted_at IS NULL")?;
    let reminders = super::reminders::minutes_by_event(db)?;
    let mut all = Vec::new();
    for event in stmt.query_map([], row_event)? {
        let event = event?;
        let minutes = reminders.get(&event.id).copied();
        all.extend(
            occurrences(&event, from, until, tz_offset)
                .into_iter()
                .map(|mut o| {
                    o.remind_before = minutes;
                    o
                }),
        );
    }
    all.sort_by(|a, b| (a.occurrence_start, &a.event.id).cmp(&(b.occurrence_start, &b.event.id)));
    all.truncate(MAX_OCCURRENCES);
    Ok(all)
}

pub fn save(db: &mut Connection, input: EventInput) -> DbResult<Event> {
    let invalid = input.title.trim().is_empty()
        || input.title.chars().count() > 200
        || !["event", "todo"].contains(&input.kind.as_str())
        || input.starts_at < 0
        || input.ends_at.is_some_and(|end| end < input.starts_at)
        || (input.kind == "todo" && input.ends_at.is_some())
        || input
            .rrule
            .as_deref()
            .is_some_and(|r| Rule::parse(r).is_none())
        || input
            .remind_before
            .is_some_and(|m| !(0..=super::reminders::MAX_MINUTES_BEFORE).contains(&m));
    if invalid {
        return Err("Sự kiện không hợp lệ".into());
    }
    let tx = db.transaction()?;
    let device = device_id(&tx)?;
    let id = input.id.unwrap_or_else(|| Ulid::generate().to_string());
    if Ulid::from_string(&id).is_err() {
        return Err("Mã sự kiện không hợp lệ".into());
    }
    let existing = get(&tx, &id)?;
    if let Some(ref event) = existing {
        if event.deleted_at.is_some() || input.rev != Some(event.rev) {
            return Err("Sự kiện đã thay đổi; hãy tải lại".into());
        }
    } else if input.rev.is_some() {
        return Err("Không tìm thấy sự kiện".into());
    }
    let event = Event {
        id,
        updated_at: now(),
        deleted_at: None,
        device_id: device,
        rev: existing.map_or(1, |e| e.rev + 1),
        title: input.title.trim().into(),
        kind: input.kind,
        starts_at: input.starts_at,
        ends_at: input.ends_at,
        rrule: input.rrule.filter(|r| !r.is_empty()),
        done: input.done,
    };
    write(&tx, &event, "event_saved")?;
    super::reminders::set_for_event(
        &tx,
        &event.id,
        input.remind_before,
        &event.device_id,
        event.updated_at,
    )?;
    tx.commit()?;
    Ok(event)
}

pub fn delete(db: &mut Connection, id: &str, rev: i64) -> DbResult<()> {
    let tx = db.transaction()?;
    let mut event = get(&tx, id)?.ok_or("Không tìm thấy sự kiện")?;
    if event.deleted_at.is_some() || event.rev != rev {
        return Err("Sự kiện đã thay đổi; hãy tải lại".into());
    }
    event.device_id = device_id(&tx)?;
    event.updated_at = now();
    event.deleted_at = Some(event.updated_at);
    event.rev += 1;
    write(&tx, &event, "event_deleted")?;
    super::reminders::set_for_event(&tx, &event.id, None, &event.device_id, event.updated_at)?;
    tx.commit()?;
    Ok(())
}

pub fn set_done(db: &mut Connection, id: &str, rev: i64, done: bool) -> DbResult<Event> {
    let tx = db.transaction()?;
    let mut event = get(&tx, id)?.ok_or("Không tìm thấy sự kiện")?;
    if event.deleted_at.is_some() || event.rev != rev || event.kind != "todo" {
        return Err("Sự kiện đã thay đổi; hãy tải lại".into());
    }
    event.device_id = device_id(&tx)?;
    event.updated_at = now();
    event.rev += 1;
    event.done = done;
    write(
        &tx,
        &event,
        if done { "todo_done" } else { "todo_reopened" },
    )?;
    tx.commit()?;
    Ok(event)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(y: i64, m: i64, d: i64, h: i64) -> i64 {
        days_from_civil(y, m, d) * DAY_MS + h * 3_600_000
    }
    fn event(start: i64, end: Option<i64>, rrule: &str) -> Event {
        Event {
            id: "1".into(),
            updated_at: 0,
            deleted_at: None,
            device_id: "d".into(),
            rev: 1,
            title: "t".into(),
            kind: "event".into(),
            starts_at: start,
            ends_at: end,
            rrule: Some(rrule.into()),
            done: false,
        }
    }
    fn starts(e: &Event, from: i64, until: i64, tz: i64) -> Vec<i64> {
        occurrences(e, from, until, tz)
            .iter()
            .map(|o| o.occurrence_start)
            .collect()
    }

    #[test]
    fn civil_round_trip() {
        for day in [-1, 0, 59, 11_016, 20_754, 47_540] {
            let (y, m, d) = civil_from_days(day);
            assert_eq!(days_from_civil(y, m, d), day);
        }
        assert_eq!(civil_from_days(20_754), (2026, 10, 28));
    }

    #[test]
    fn parses_and_rejects_rules() {
        assert!(Rule::parse("RRULE:FREQ=WEEKLY;BYDAY=MO,WE;INTERVAL=2").is_some());
        assert!(Rule::parse("FREQ=DAILY;UNTIL=20261231").is_some());
        assert!(Rule::parse("FREQ=DAILY;UNTIL=20261231T235959Z").is_some());
        for bad in [
            "",
            "FREQ=HOURLY",
            "FREQ=DAILY;INTERVAL=0",
            "FREQ=DAILY;COUNT=0",
            "FREQ=DAILY;BYDAY=MO",
            "FREQ=DAILY;COUNT=3;UNTIL=20261231",
            "FREQ=WEEKLY;BYDAY=XX",
            "FREQ=DAILY;UNTIL=20261301",
            "FREQ=DAILY;BYSETPOS=1",
            "INTERVAL=2",
        ] {
            assert!(Rule::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn daily_with_count_and_window() {
        let e = event(
            at(2026, 10, 28, 9),
            Some(at(2026, 10, 28, 10)),
            "FREQ=DAILY;COUNT=3",
        );
        let all = starts(&e, at(2026, 10, 1, 0), at(2026, 12, 1, 0), 0);
        assert_eq!(
            all,
            vec![
                at(2026, 10, 28, 9),
                at(2026, 10, 29, 9),
                at(2026, 10, 30, 9)
            ]
        );
        // COUNT is measured from the first occurrence, not from the window start.
        assert_eq!(
            starts(&e, at(2026, 10, 30, 0), at(2026, 12, 1, 0), 0),
            vec![at(2026, 10, 30, 9)]
        );
        // An occurrence that started earlier but is still running is included.
        assert_eq!(
            starts(&e, at(2026, 10, 28, 9) + 1800_000, at(2026, 10, 28, 12), 0).len(),
            1
        );
    }

    #[test]
    fn weekly_by_day_with_interval() {
        // Wed 2026-10-28, every other week on Mon and Wed.
        let e = event(
            at(2026, 10, 28, 8),
            None,
            "FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE",
        );
        let got = starts(&e, at(2026, 10, 26, 0), at(2026, 11, 20, 0), 0);
        assert_eq!(
            got,
            vec![at(2026, 10, 28, 8), at(2026, 11, 9, 8), at(2026, 11, 11, 8)]
        );
    }

    #[test]
    fn monthly_skips_missing_days_and_yearly_leap_day() {
        let e = event(at(2026, 1, 31, 8), None, "FREQ=MONTHLY");
        let got = starts(&e, at(2026, 1, 1, 0), at(2026, 6, 1, 0), 0);
        assert_eq!(
            got,
            vec![at(2026, 1, 31, 8), at(2026, 3, 31, 8), at(2026, 5, 31, 8)]
        );
        let leap = event(at(2024, 2, 29, 8), None, "FREQ=YEARLY");
        assert_eq!(
            starts(&leap, at(2024, 1, 1, 0), at(2033, 1, 1, 0), 0),
            vec![at(2024, 2, 29, 8), at(2028, 2, 29, 8), at(2032, 2, 29, 8)]
        );
    }

    #[test]
    fn until_and_timezone() {
        let e = event(at(2026, 10, 28, 9), None, "FREQ=DAILY;UNTIL=20261030");
        assert_eq!(
            starts(&e, at(2026, 10, 1, 0), at(2026, 12, 1, 0), 0).len(),
            3
        );
        // 18:00 UTC+7 on Wed 28 is 11:00 UTC; weekly Mon/Wed keeps the local weekday across midnight UTC.
        let tz = 7 * 60;
        let late = event(at(2026, 10, 28, 20), None, "FREQ=WEEKLY;BYDAY=TH");
        // 20:00 UTC is 03:00 local on Thursday 29th, so the first occurrence is that day.
        assert_eq!(
            starts(&late, at(2026, 10, 1, 0), at(2026, 11, 10, 0), tz)[0],
            at(2026, 10, 28, 20)
        );
        assert_eq!(
            starts(&late, at(2026, 10, 1, 0), at(2026, 11, 10, 0), tz)[1],
            at(2026, 11, 4, 20)
        );
    }

    #[test]
    fn crud_conflicts_todos_and_outbox() -> DbResult<()> {
        let mut db = Connection::open_in_memory()?;
        super::super::migrate(&mut db)?;
        let input = |kind: &str, end: Option<i64>, rrule: Option<&str>| EventInput {
            id: None,
            rev: None,
            title: "Họp".into(),
            kind: kind.into(),
            starts_at: at(2026, 10, 28, 9),
            ends_at: end,
            rrule: rrule.map(str::to_string),
            done: false,
            remind_before: None,
        };
        assert!(save(&mut db, input("event", Some(0), None)).is_err());
        assert!(save(&mut db, input("todo", Some(at(2026, 10, 29, 0)), None)).is_err());
        assert!(save(&mut db, input("event", None, Some("FREQ=BAD"))).is_err());
        let meeting = save(&mut db, input("event", None, Some("FREQ=DAILY")))?;
        let todo = save(&mut db, input("todo", None, None))?;
        let window = range(&db, at(2026, 10, 28, 0), at(2026, 10, 30, 0), 0)?;
        assert_eq!(window.len(), 3); // two meeting days + one todo
        assert!(range(&db, 10, 5, 0).is_err());
        assert!(set_done(&mut db, &meeting.id, meeting.rev, true).is_err());
        let done = set_done(&mut db, &todo.id, todo.rev, true)?;
        assert!(done.done);
        assert!(set_done(&mut db, &todo.id, todo.rev, false).is_err());
        delete(&mut db, &meeting.id, meeting.rev)?;
        assert_eq!(
            range(&db, at(2026, 10, 28, 0), at(2026, 10, 30, 0), 0)?.len(),
            1
        );
        let latest: i64 = db.query_row(
            "SELECT MAX(rev) FROM outbox WHERE id=?1 AND kind='event'",
            [&meeting.id],
            |r| r.get(0),
        )?;
        assert_eq!(latest, 2);
        Ok(())
    }
}

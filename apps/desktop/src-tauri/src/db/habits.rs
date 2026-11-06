use super::{device_id, now, DbResult};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

/// How often a habit is expected. Stored as a string so it matches the shared `Habit.frequency`:
/// `daily`, `weekdays:1,3,5` (0 = Sunday … 6 = Saturday) or `weekly:3` (N check-ins per Monday-start week).
#[derive(Debug, Clone, PartialEq)]
pub enum Frequency {
    Daily,
    Weekdays(Vec<u8>),
    Weekly(u8),
}

impl Frequency {
    pub fn parse(text: &str) -> Option<Self> {
        if text == "daily" {
            return Some(Self::Daily);
        }
        if let Some(rest) = text.strip_prefix("weekdays:") {
            let mut days = Vec::new();
            for part in rest.split(',') {
                let day: u8 = part.parse().ok()?;
                if day > 6 || days.contains(&day) {
                    return None;
                }
                days.push(day);
            }
            days.sort_unstable();
            return (!days.is_empty()).then_some(Self::Weekdays(days));
        }
        let times: u8 = text.strip_prefix("weekly:")?.parse().ok()?;
        (1..=7).contains(&times).then_some(Self::Weekly(times))
    }
}

/// Days since 1970-01-01 for a `YYYY-MM-DD` string (proleptic Gregorian).
pub fn day_number(date: &str) -> Option<i64> {
    let bytes = date.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year: i64 = date[0..4].parse().ok()?;
    let month: i64 = date[5..7].parse().ok()?;
    let day: i64 = date[8..10].parse().ok()?;
    if !(1970..=9999).contains(&year) || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let month_days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if day > month_days[(month - 1) as usize] {
        return None;
    }
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

/// 0 = Sunday … 6 = Saturday (1970-01-01 was a Thursday).
fn weekday(day: i64) -> u8 {
    (day + 4).rem_euclid(7) as u8
}

/// Monday-start week index.
fn week_of(day: i64) -> i64 {
    (day + 3).div_euclid(7)
}

/// Consecutive completed periods ending at `today`. A period still in progress never breaks the streak.
pub fn streak(frequency: &Frequency, done: &[i64], today: i64) -> u32 {
    let has = |d: i64| done.contains(&d);
    let mut count = 0;
    match frequency {
        Frequency::Daily | Frequency::Weekdays(_) => {
            let scheduled = |d: i64| match frequency {
                Frequency::Weekdays(days) => days.contains(&weekday(d)),
                _ => true,
            };
            let mut day = today;
            // Today not yet checked in does not end the streak.
            if scheduled(day) && !has(day) {
                day -= 1;
            }
            // Give up after ten years; guards against a pathological schedule.
            for _ in 0..3660 {
                if !scheduled(day) {
                    day -= 1;
                    continue;
                }
                if !has(day) {
                    break;
                }
                count += 1;
                day -= 1;
            }
        }
        Frequency::Weekly(times) => {
            let needed = *times as usize;
            let in_week = |w: i64| done.iter().filter(|d| week_of(**d) == w).count();
            let mut week = week_of(today);
            if in_week(week) < needed {
                week -= 1;
            }
            for _ in 0..520 {
                if in_week(week) < needed {
                    break;
                }
                count += 1;
                week -= 1;
            }
        }
    }
    count
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Habit {
    pub id: String,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
    pub device_id: String,
    pub rev: i64,
    pub title: String,
    pub frequency: String,
}

#[derive(Debug, Serialize)]
pub struct HabitView {
    #[serde(flatten)]
    pub habit: Habit,
    pub streak: u32,
    pub done_today: bool,
    /// Check-in dates (`YYYY-MM-DD`) from the last 365 days, oldest first.
    pub dates: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct HabitInput {
    pub id: Option<String>,
    pub rev: Option<i64>,
    pub title: String,
    pub frequency: String,
}

fn bad_column(index: usize, e: impl std::error::Error + Send + Sync + 'static) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, Box::new(e))
}

fn row_habit(row: &rusqlite::Row<'_>) -> rusqlite::Result<Habit> {
    let data: String = row.get(5)?;
    let data: serde_json::Value = serde_json::from_str(&data).map_err(|e| bad_column(5, e))?;
    Ok(Habit {
        id: row.get(0)?,
        updated_at: row.get(1)?,
        deleted_at: row.get(2)?,
        device_id: row.get(3)?,
        rev: row.get(4)?,
        title: data["title"].as_str().unwrap_or_default().to_string(),
        frequency: data["frequency"].as_str().unwrap_or("daily").to_string(),
    })
}

fn get(tx: &Transaction<'_>, id: &str) -> DbResult<Option<Habit>> {
    Ok(tx
        .query_row(
            "SELECT id, updated_at, deleted_at, device_id, rev, data FROM habits WHERE id = ?1",
            [id],
            row_habit,
        )
        .optional()?)
}

fn push_outbox(tx: &Transaction<'_>, kind: &str, id: &str, rev: i64, body: &str) -> DbResult<()> {
    tx.execute(
        "INSERT INTO outbox (id, kind, rev, data) VALUES (?1, ?2, ?3, ?4)",
        params![id, kind, rev, body],
    )?;
    Ok(())
}

fn record_activity(
    tx: &Transaction<'_>,
    kind: &str,
    at: i64,
    device: &str,
    data: serde_json::Value,
) -> DbResult<()> {
    let id = Ulid::generate().to_string();
    let stored = serde_json::json!({"kind": kind, "occurred_at": at, "data": data});
    tx.execute(
        "INSERT INTO activity (id, updated_at, deleted_at, device_id, rev, data) VALUES (?1, ?2, NULL, ?3, 1, ?4)",
        params![id, at, device, stored.to_string()],
    )?;
    let full = serde_json::json!({"id": id, "updated_at": at, "deleted_at": null, "device_id": device, "rev": 1, "kind": kind, "occurred_at": at, "data": data});
    push_outbox(tx, "activity", &id, 1, &full.to_string())
}

fn write_habit(tx: &Transaction<'_>, habit: &Habit) -> DbResult<()> {
    let data = serde_json::json!({"title": habit.title, "frequency": habit.frequency});
    tx.execute(
        "INSERT INTO habits (id, updated_at, deleted_at, device_id, rev, data) VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT(id) DO UPDATE SET updated_at=excluded.updated_at, deleted_at=excluded.deleted_at, device_id=excluded.device_id, rev=excluded.rev, data=excluded.data",
        params![habit.id, habit.updated_at, habit.deleted_at, habit.device_id, habit.rev, data.to_string()],
    )?;
    push_outbox(
        tx,
        "habit",
        &habit.id,
        habit.rev,
        &serde_json::to_string(habit)?,
    )
}

pub fn list(db: &Connection, today: &str) -> DbResult<Vec<HabitView>> {
    let today_number = day_number(today).ok_or("Ngày không hợp lệ")?;
    let mut stmt = db.prepare("SELECT id, updated_at, deleted_at, device_id, rev, data FROM habits WHERE deleted_at IS NULL ORDER BY updated_at DESC, id")?;
    let habits = stmt
        .query_map([], row_habit)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut logs = db.prepare("SELECT json_extract(data, '$.date') FROM habit_logs WHERE deleted_at IS NULL AND json_extract(data, '$.habit_id') = ?1")?;
    let mut views = Vec::new();
    for habit in habits {
        let mut dates = logs
            .query_map([&habit.id], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        dates.sort();
        dates.dedup();
        let numbers: Vec<i64> = dates.iter().filter_map(|d| day_number(d)).collect();
        let current = Frequency::parse(&habit.frequency)
            .map(|f| streak(&f, &numbers, today_number))
            .unwrap_or(0);
        dates.retain(|d| day_number(d).is_some_and(|n| n > today_number - 365));
        views.push(HabitView {
            done_today: numbers.contains(&today_number),
            streak: current,
            dates,
            habit,
        });
    }
    Ok(views)
}

pub fn save(db: &mut Connection, input: HabitInput) -> DbResult<Habit> {
    if input.title.trim().is_empty()
        || input.title.chars().count() > 100
        || Frequency::parse(&input.frequency).is_none()
    {
        return Err("Thói quen không hợp lệ".into());
    }
    let tx = db.transaction()?;
    let device = device_id(&tx)?;
    let id = input.id.unwrap_or_else(|| Ulid::generate().to_string());
    if Ulid::from_string(&id).is_err() {
        return Err("Mã thói quen không hợp lệ".into());
    }
    let existing = get(&tx, &id)?;
    if let Some(ref habit) = existing {
        if habit.deleted_at.is_some() || input.rev != Some(habit.rev) {
            return Err("Thói quen đã thay đổi; hãy tải lại".into());
        }
    } else if input.rev.is_some() {
        return Err("Không tìm thấy thói quen".into());
    }
    let habit = Habit {
        id,
        updated_at: now(),
        deleted_at: None,
        device_id: device,
        rev: existing.map_or(1, |h| h.rev + 1),
        title: input.title.trim().into(),
        frequency: input.frequency,
    };
    write_habit(&tx, &habit)?;
    record_activity(
        &tx,
        "habit_saved",
        habit.updated_at,
        &habit.device_id,
        serde_json::json!({"habit_id": habit.id, "title": habit.title}),
    )?;
    tx.commit()?;
    Ok(habit)
}

pub fn delete(db: &mut Connection, id: &str, rev: i64) -> DbResult<()> {
    let tx = db.transaction()?;
    let mut habit = get(&tx, id)?.ok_or("Không tìm thấy thói quen")?;
    if habit.deleted_at.is_some() || habit.rev != rev {
        return Err("Thói quen đã thay đổi; hãy tải lại".into());
    }
    habit.device_id = device_id(&tx)?;
    habit.updated_at = now();
    habit.deleted_at = Some(habit.updated_at);
    habit.rev += 1;
    write_habit(&tx, &habit)?;
    // Logs of a deleted habit become tombstones so other devices drop them too.
    let mut stmt = tx.prepare("SELECT id, rev, data FROM habit_logs WHERE deleted_at IS NULL AND json_extract(data, '$.habit_id') = ?1")?;
    let logs = stmt
        .query_map([id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    for (log_id, log_rev, data) in logs {
        write_log(&tx, &log_id, log_rev + 1, &habit, &data, true)?;
    }
    record_activity(
        &tx,
        "habit_deleted",
        habit.updated_at,
        &habit.device_id,
        serde_json::json!({"habit_id": habit.id, "title": habit.title}),
    )?;
    tx.commit()?;
    Ok(())
}

fn write_log(
    tx: &Transaction<'_>,
    id: &str,
    rev: i64,
    habit: &Habit,
    data: &str,
    deleted: bool,
) -> DbResult<()> {
    let stamp = habit.updated_at;
    let deleted_at = deleted.then_some(stamp);
    tx.execute(
        "INSERT INTO habit_logs (id, updated_at, deleted_at, device_id, rev, data) VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT(id) DO UPDATE SET updated_at=excluded.updated_at, deleted_at=excluded.deleted_at, device_id=excluded.device_id, rev=excluded.rev",
        params![id, stamp, deleted_at, habit.device_id, rev, data],
    )?;
    let mut full: serde_json::Value = serde_json::from_str(data)?;
    full["id"] = id.into();
    full["updated_at"] = stamp.into();
    full["deleted_at"] = deleted_at.into();
    full["device_id"] = habit.device_id.clone().into();
    full["rev"] = rev.into();
    push_outbox(tx, "habit_log", id, rev, &full.to_string())
}

/// Sets or clears the check-in of `habit_id` on `date`. Repeating the same call is a no-op.
pub fn check_in(db: &mut Connection, habit_id: &str, date: &str, done: bool) -> DbResult<()> {
    if day_number(date).is_none() {
        return Err("Ngày không hợp lệ".into());
    }
    let tx = db.transaction()?;
    let mut habit = get(&tx, habit_id)?.ok_or("Không tìm thấy thói quen")?;
    if habit.deleted_at.is_some() {
        return Err("Không tìm thấy thói quen".into());
    }
    habit.device_id = device_id(&tx)?;
    habit.updated_at = now();
    let found: Option<(String, i64, Option<i64>, String)> = tx
        .query_row(
            "SELECT id, rev, deleted_at, data FROM habit_logs WHERE json_extract(data, '$.habit_id') = ?1 AND json_extract(data, '$.date') = ?2 ORDER BY deleted_at IS NOT NULL, updated_at DESC LIMIT 1",
            params![habit_id, date],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    match (found, done) {
        (Some((_, _, None, _)), true) | (None, false) | (Some((_, _, Some(_), _)), false) => {
            return Ok(())
        }
        (Some((id, rev, _, data)), _) => write_log(&tx, &id, rev + 1, &habit, &data, !done)?,
        (None, true) => {
            let data = serde_json::json!({"habit_id": habit_id, "date": date}).to_string();
            write_log(&tx, &Ulid::generate().to_string(), 1, &habit, &data, false)?;
        }
    }
    record_activity(
        &tx,
        if done {
            "habit_checkin"
        } else {
            "habit_uncheck"
        },
        habit.updated_at,
        &habit.device_id,
        serde_json::json!({"habit_id": habit.id, "title": habit.title, "date": date}),
    )?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(date: &str) -> i64 {
        day_number(date).unwrap()
    }

    #[test]
    fn day_numbers_and_weekdays() {
        assert_eq!(d("1970-01-01"), 0);
        assert_eq!(d("2000-03-01") - d("2000-02-28"), 2);
        assert_eq!(d("2026-10-28"), 20_754);
        assert_eq!(weekday(d("2026-10-28")), 3); // Wednesday
        assert_eq!(weekday(d("2026-11-01")), 0); // Sunday
        assert!(day_number("2026-02-29").is_none());
        assert!(day_number("2026-13-01").is_none());
        assert!(day_number("26-1-1").is_none());
    }

    #[test]
    fn frequency_parsing() {
        assert_eq!(Frequency::parse("daily"), Some(Frequency::Daily));
        assert_eq!(
            Frequency::parse("weekdays:5,1"),
            Some(Frequency::Weekdays(vec![1, 5]))
        );
        assert_eq!(Frequency::parse("weekly:3"), Some(Frequency::Weekly(3)));
        for bad in [
            "",
            "weekly:0",
            "weekly:8",
            "weekdays:",
            "weekdays:7",
            "weekdays:1,1",
            "hourly",
        ] {
            assert!(Frequency::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn daily_streak_survives_an_unchecked_today_but_not_a_gap() {
        let today = d("2026-10-28");
        let done = [d("2026-10-25"), d("2026-10-26"), d("2026-10-27")];
        assert_eq!(streak(&Frequency::Daily, &done, today), 3);
        assert_eq!(
            streak(&Frequency::Daily, &[today - 2, today - 1, today], today),
            3
        );
        assert_eq!(streak(&Frequency::Daily, &[today - 3, today - 1], today), 1);
        assert_eq!(streak(&Frequency::Daily, &[today - 3], today), 0);
        assert_eq!(streak(&Frequency::Daily, &[], today), 0);
    }

    #[test]
    fn weekday_streak_skips_unscheduled_days() {
        // Mon/Wed/Fri habit; today is Wednesday 2026-10-28.
        let f = Frequency::Weekdays(vec![1, 3, 5]);
        let done = [d("2026-10-23"), d("2026-10-26"), d("2026-10-28")];
        assert_eq!(streak(&f, &done, d("2026-10-28")), 3);
        // Missing Monday breaks it even though Tue/Thu/Sat are free.
        assert_eq!(
            streak(&f, &[d("2026-10-23"), d("2026-10-28")], d("2026-10-28")),
            1
        );
        // Checking in on an unscheduled day neither helps nor hurts.
        assert_eq!(
            streak(&f, &[d("2026-10-26"), d("2026-10-27")], d("2026-10-27")),
            1
        );
    }

    #[test]
    fn weekly_streak_counts_met_weeks() {
        // Weeks start Monday; 2026-10-26 is a Monday.
        let f = Frequency::Weekly(2);
        let done = [
            d("2026-10-12"),
            d("2026-10-14"),
            d("2026-10-19"),
            d("2026-10-21"),
        ];
        // The current week (Oct 26–Nov 1) is still in progress.
        assert_eq!(streak(&f, &done, d("2026-10-28")), 2);
        let met_now = [done.as_slice(), &[d("2026-10-26"), d("2026-10-27")]].concat();
        assert_eq!(streak(&f, &met_now, d("2026-10-28")), 3);
        assert_eq!(streak(&f, &[d("2026-10-19")], d("2026-10-28")), 0);
    }

    #[test]
    fn check_in_is_idempotent_undoable_and_synced() -> DbResult<()> {
        let mut db = Connection::open_in_memory()?;
        super::super::migrate(&mut db)?;
        assert!(save(
            &mut db,
            HabitInput {
                id: None,
                rev: None,
                title: " ".into(),
                frequency: "daily".into()
            }
        )
        .is_err());
        let habit = save(
            &mut db,
            HabitInput {
                id: None,
                rev: None,
                title: "Đọc sách".into(),
                frequency: "daily".into(),
            },
        )?;
        let stale = save(
            &mut db,
            HabitInput {
                id: Some(habit.id.clone()),
                rev: Some(0),
                title: "x".into(),
                frequency: "daily".into(),
            },
        );
        assert!(stale.is_err());

        check_in(&mut db, &habit.id, "2026-10-27", true)?;
        check_in(&mut db, &habit.id, "2026-10-27", true)?;
        check_in(&mut db, &habit.id, "2026-10-28", true)?;
        let views = list(&db, "2026-10-28")?;
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].streak, 2);
        assert!(views[0].done_today);
        assert_eq!(views[0].dates, vec!["2026-10-27", "2026-10-28"]);

        check_in(&mut db, &habit.id, "2026-10-28", false)?;
        check_in(&mut db, &habit.id, "2026-10-28", false)?;
        let views = list(&db, "2026-10-28")?;
        assert!(!views[0].done_today);
        assert_eq!(views[0].streak, 1);
        // Re-checking revives the same log rather than adding a duplicate row.
        check_in(&mut db, &habit.id, "2026-10-28", true)?;
        let logs: i64 = db.query_row("SELECT COUNT(*) FROM habit_logs", [], |r| r.get(0))?;
        assert_eq!(logs, 2);
        assert!(check_in(&mut db, &habit.id, "bad", true).is_err());

        let latest: i64 = db.query_row(
            "SELECT MAX(rev) FROM outbox WHERE kind='habit_log'",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(latest, 3);

        let current = list(&db, "2026-10-28")?.remove(0).habit;
        delete(&mut db, &current.id, current.rev)?;
        assert!(list(&db, "2026-10-28")?.is_empty());
        let live: i64 = db.query_row(
            "SELECT COUNT(*) FROM habit_logs WHERE deleted_at IS NULL",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(live, 0);
        assert!(check_in(&mut db, &current.id, "2026-10-28", true).is_err());
        Ok(())
    }
}

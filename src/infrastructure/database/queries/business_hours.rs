use chrono::NaiveDate;
use sqlx::types::Json;
use sqlx::{SqliteConnection, SqlitePool};

use crate::domain::commerce::business_hours::{CommercialSettings, Schedule, Windows};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::begin_immediate;

pub async fn load(pool: &SqlitePool, establishment: &str) -> AppResult<Schedule> {
    let mut connection = pool.acquire().await?;
    load_in(&mut connection, establishment).await
}

#[derive(sqlx::FromRow)]
struct ScheduleRow {
    time_zone: Option<String>,
    delivery: Option<bool>,
    pickup: Option<bool>,
    minimum_centavos: Option<i64>,
    acceptance_minutes: Option<i64>,
    paused: Option<bool>,
    paused_until: Option<i64>,
    weekly: Json<Vec<(usize, String)>>,
    exceptions: Json<Vec<(NaiveDate, String)>>,
}

pub(crate) async fn load_in(
    conn: &mut SqliteConnection,
    establishment: &str,
) -> AppResult<Schedule> {
    // The tenant row keeps hours available even before the first settings save.
    let row: ScheduleRow = sqlx::query_as(
        "SELECT s.time_zone, s.delivery, s.pickup, s.minimum_centavos, s.acceptance_minutes,
                s.paused, s.paused_until,
                (SELECT json_group_array(json_array(weekday, windows_json))
                 FROM business_hours WHERE establishment_id = ?1) AS weekly,
                (SELECT json_group_array(json_array(local_date, windows_json))
                 FROM business_hour_exceptions WHERE establishment_id = ?1) AS exceptions
         FROM (SELECT ?1 AS establishment_id) tenant
         LEFT JOIN commercial_settings s USING (establishment_id)",
    )
    .bind(establishment)
    .fetch_one(conn)
    .await?;

    let mut schedule = Schedule::default();
    let settings = &mut schedule.settings;
    if let Some(time_zone) = row.time_zone {
        settings.time_zone = time_zone;
        settings.delivery = row.delivery.unwrap_or(settings.delivery);
        settings.pickup = row.pickup.unwrap_or(settings.pickup);
        settings.minimum_centavos = row.minimum_centavos.unwrap_or(settings.minimum_centavos);
        settings.acceptance_minutes = row
            .acceptance_minutes
            .unwrap_or(settings.acceptance_minutes);
        settings.paused = row.paused.unwrap_or(settings.paused);
        settings.paused_until = row.paused_until;
    }
    let decode = |raw: &str| {
        Windows::validated_from_json(raw)
            .map_err(|_| AppError::Internal("invalid persisted business hours".into()))
    };
    for (day, json) in row.weekly.0 {
        schedule.weekly[day] = decode(&json)?;
    }
    for (date, json) in row.exceptions.0 {
        schedule.exceptions.insert(date, decode(&json)?);
    }
    Ok(schedule)
}

pub async fn save_settings(
    pool: &SqlitePool,
    establishment: &str,
    settings: &CommercialSettings,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO commercial_settings (establishment_id, time_zone, delivery, pickup, minimum_centavos, acceptance_minutes)
         VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(establishment_id) DO UPDATE SET time_zone = excluded.time_zone,
         delivery = excluded.delivery, pickup = excluded.pickup, minimum_centavos = excluded.minimum_centavos,
         acceptance_minutes = excluded.acceptance_minutes",
    ).bind(establishment).bind(&settings.time_zone).bind(settings.delivery).bind(settings.pickup)
        .bind(settings.minimum_centavos).bind(settings.acceptance_minutes).execute(pool).await?;
    Ok(())
}

pub async fn set_pause(
    pool: &SqlitePool,
    establishment: &str,
    paused: bool,
    until: Option<i64>,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO commercial_settings (establishment_id, paused, paused_until) VALUES (?, ?, ?)
         ON CONFLICT(establishment_id) DO UPDATE SET paused = excluded.paused, paused_until = excluded.paused_until",
    ).bind(establishment).bind(paused).bind(until).execute(pool).await?;
    Ok(())
}

pub async fn save_weekly(
    pool: &SqlitePool,
    establishment: &str,
    week: &[Windows; 7],
) -> AppResult<()> {
    let mut tx = begin_immediate(pool).await?;
    for (weekday, windows) in week.iter().enumerate() {
        let json = encode(windows)?;
        sqlx::query(
            "INSERT INTO business_hours (establishment_id, weekday, windows_json) VALUES (?, ?, ?)
             ON CONFLICT(establishment_id, weekday) DO UPDATE SET windows_json = excluded.windows_json",
        ).bind(establishment).bind(weekday as i64).bind(json).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn save_exception(
    pool: &SqlitePool,
    establishment: &str,
    date: NaiveDate,
    windows: &Windows,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO business_hour_exceptions (establishment_id, local_date, windows_json) VALUES (?, ?, ?)
         ON CONFLICT(establishment_id, local_date) DO UPDATE SET windows_json = excluded.windows_json",
    ).bind(establishment).bind(date).bind(encode(windows)?).execute(pool).await?;
    Ok(())
}

pub async fn delete_exception(
    pool: &SqlitePool,
    establishment: &str,
    date: NaiveDate,
) -> AppResult<()> {
    sqlx::query(
        "DELETE FROM business_hour_exceptions WHERE local_date = ? AND establishment_id = ?",
    )
    .bind(date)
    .bind(establishment)
    .execute(pool)
    .await?;
    Ok(())
}

fn encode(windows: &Windows) -> AppResult<String> {
    serde_json::to_string(windows).map_err(|error| AppError::Internal(error.to_string()))
}

use chrono::NaiveDate;
use sqlx::SqlitePool;

use crate::domain::commerce::business_hours::{CommercialSettings, Schedule, Windows};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::begin_immediate;

pub async fn load(pool: &SqlitePool, establishment: &str) -> AppResult<Schedule> {
    let mut tx = pool.begin().await?;
    type SettingsRow = (String, bool, bool, i64, i64, bool, Option<i64>);
    let settings: Option<SettingsRow> = sqlx::query_as(
        "SELECT time_zone, delivery, pickup, minimum_centavos, acceptance_minutes, paused, paused_until
         FROM commercial_settings WHERE establishment_id = ?",
    )
    .bind(establishment)
    .fetch_optional(&mut *tx)
    .await?;
    let weekly: Vec<(i64, String)> = sqlx::query_as(
        "SELECT weekday, windows_json FROM business_hours WHERE establishment_id = ? ORDER BY weekday",
    ).bind(establishment).fetch_all(&mut *tx).await?;
    let exceptions: Vec<(NaiveDate, String)> = sqlx::query_as(
        "SELECT local_date, windows_json FROM business_hour_exceptions WHERE establishment_id = ? ORDER BY local_date",
    ).bind(establishment).fetch_all(&mut *tx).await?;
    tx.commit().await?;

    let mut schedule = Schedule::default();
    if let Some((
        time_zone,
        delivery,
        pickup,
        minimum_centavos,
        acceptance_minutes,
        paused,
        paused_until,
    )) = settings
    {
        schedule.settings = CommercialSettings {
            time_zone,
            delivery,
            pickup,
            minimum_centavos,
            acceptance_minutes,
            paused,
            paused_until,
        };
    }
    let decode = |raw: &str| {
        Windows::validated_from_json(raw)
            .map_err(|_| AppError::Internal("invalid persisted business hours".into()))
    };
    for (day, json) in weekly {
        schedule.weekly[day as usize] = decode(&json)?;
    }
    for (date, json) in exceptions {
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

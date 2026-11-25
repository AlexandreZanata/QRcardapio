use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, Timelike, Utc};
use serde::{Deserialize, Serialize};

pub const SUPPORTED_ZONES: [&str; 5] = [
    "America/Sao_Paulo",
    "America/Cuiaba",
    "America/Manaus",
    "America/Rio_Branco",
    "America/Noronha",
];

pub fn zone(name: &str) -> Option<jiff::tz::TimeZone> {
    Some(match name {
        "America/Sao_Paulo" => jiff::tz::get!("America/Sao_Paulo"),
        "America/Cuiaba" => jiff::tz::get!("America/Cuiaba"),
        "America/Manaus" => jiff::tz::get!("America/Manaus"),
        "America/Rio_Branco" => jiff::tz::get!("America/Rio_Branco"),
        "America/Noronha" => jiff::tz::get!("America/Noronha"),
        _ => return None,
    })
}

#[derive(Clone, Copy)]
pub enum ServiceMode {
    Delivery,
    Pickup,
}

#[derive(Debug, Clone)]
pub struct CommercialSettings {
    pub time_zone: String,
    pub delivery: bool,
    pub pickup: bool,
    pub minimum_centavos: i64,
    pub acceptance_minutes: i64,
    pub paused: bool,
    pub paused_until: Option<i64>,
}

impl Default for CommercialSettings {
    fn default() -> Self {
        Self {
            time_zone: SUPPORTED_ZONES[0].into(),
            delivery: false,
            pickup: false,
            minimum_centavos: 0,
            acceptance_minutes: 10,
            paused: false,
            paused_until: None,
        }
    }
}

impl CommercialSettings {
    pub fn paused_at(&self, time: DateTime<Utc>) -> bool {
        self.paused
            && self
                .paused_until
                .is_none_or(|until| time.timestamp() < until)
    }
}

/// Sorted minutes since local midnight. An end below its start denotes the following day.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Windows(Vec<(u16, u16)>);

impl FromStr for Windows {
    type Err = ScheduleError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if text.trim().is_empty() {
            return Ok(Self::default());
        }
        let parts = text
            .split(',')
            .map(|interval| {
                let (opens, closes) = interval.trim().split_once('-').ok_or(ScheduleError)?;
                let start = parse_minute(opens, false)?;
                let end = parse_minute(closes, true)?;
                if start == end {
                    return Err(ScheduleError);
                }
                Ok((start, end))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Self::validated(parts)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("invalid business hour intervals")]
pub struct ScheduleError;

impl Windows {
    fn validated(mut parts: Vec<(u16, u16)>) -> Result<Self, ScheduleError> {
        if parts.len() > 12
            || parts
                .iter()
                .any(|&(start, end)| start >= 1440 || end > 1440 || start == end)
        {
            return Err(ScheduleError);
        }
        parts.sort_unstable();
        if parts
            .windows(2)
            .any(|pair| pair[0].1 < pair[0].0 || pair[0].1 > pair[1].0)
        {
            return Err(ScheduleError);
        }
        Ok(Self(parts))
    }

    pub fn validated_from_json(raw: &str) -> Result<Self, ScheduleError> {
        Self::validated(serde_json::from_str(raw).map_err(|_| ScheduleError)?)
    }

    fn on_day(&self, minute: u16) -> bool {
        self.0
            .iter()
            .any(|&(start, end)| minute >= start && (end < start || minute < end))
    }

    fn next_day(&self, minute: u16) -> bool {
        self.0
            .iter()
            .any(|&(start, end)| end < start && minute < end)
    }
}

impl fmt::Display for Windows {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, &(start, end)) in self.0.iter().enumerate() {
            if index > 0 {
                formatter.write_str(", ")?;
            }
            write!(
                formatter,
                "{:02}:{:02}-{:02}:{:02}",
                start / 60,
                start % 60,
                end / 60,
                end % 60
            )?;
        }
        Ok(())
    }
}

fn parse_minute(raw: &str, close: bool) -> Result<u16, ScheduleError> {
    let raw = raw.trim();
    if raw == "24:00" && close {
        return Ok(1440);
    }
    if raw.len() != 5 {
        return Err(ScheduleError);
    }
    NaiveTime::parse_from_str(raw, "%H:%M")
        .ok()
        .and_then(|time| u16::try_from(time.hour() * 60 + time.minute()).ok())
        .ok_or(ScheduleError)
}

#[derive(Debug, Default)]
pub struct Schedule {
    pub settings: CommercialSettings,
    pub weekly: [Windows; 7],
    pub exceptions: BTreeMap<NaiveDate, Windows>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenService {
    pub shift_date: NaiveDate,
    pub business_date: NaiveDate,
}

impl Schedule {
    pub fn service_at(&self, instant: DateTime<Utc>, mode: ServiceMode) -> Option<OpenService> {
        let enabled = match mode {
            ServiceMode::Delivery => self.settings.delivery,
            ServiceMode::Pickup => self.settings.pickup,
        };
        if !enabled || self.settings.paused_at(instant) {
            return None;
        }
        let time_zone = zone(&self.settings.time_zone)?;
        let timestamp = jiff::Timestamp::from_second(instant.timestamp()).ok()?;
        let local = time_zone.to_datetime(timestamp);
        let date = NaiveDate::from_ymd_opt(
            i32::from(local.year()),
            local.month().try_into().ok()?,
            local.day().try_into().ok()?,
        )?;
        let previous = date.pred_opt()?;
        let minute =
            u16::try_from(i32::from(local.hour()) * 60 + i32::from(local.minute())).ok()?;
        let shift_date = if let Some(exception) = self.exceptions.get(&date) {
            exception.on_day(minute).then_some(date)?
        } else if self.windows(previous).next_day(minute) {
            previous
        } else {
            self.windows(date).on_day(minute).then_some(date)?
        };
        Some(OpenService {
            shift_date,
            business_date: if minute < 4 * 60 { previous } else { date },
        })
    }

    fn windows(&self, date: NaiveDate) -> &Windows {
        self.exceptions
            .get(&date)
            .unwrap_or(&self.weekly[date.weekday().num_days_from_monday() as usize])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overnight_shift_local_cutoff_exceptions_and_pause() {
        let mut schedule = Schedule::default();
        schedule.settings.time_zone = "America/Cuiaba".into();
        schedule.settings.delivery = true;
        schedule.weekly[0] = "11:00-14:00, 18:00-05:00".parse().unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 11, 23).unwrap();
        let after_midnight: DateTime<Utc> = "2026-11-24T05:30:00Z".parse().unwrap();
        let after_cutoff: DateTime<Utc> = "2026-11-24T08:30:00Z".parse().unwrap();
        assert_eq!(
            schedule.service_at(after_midnight, ServiceMode::Delivery),
            Some(OpenService {
                shift_date: date,
                business_date: date
            })
        );
        assert_eq!(
            schedule.service_at(after_cutoff, ServiceMode::Delivery),
            Some(OpenService {
                shift_date: date,
                business_date: date.succ_opt().unwrap()
            })
        );
        assert!(
            schedule
                .service_at(after_midnight, ServiceMode::Pickup)
                .is_none()
        );
        schedule.settings.paused = true;
        schedule.settings.paused_until = Some(after_cutoff.timestamp());
        assert!(
            schedule
                .service_at(after_midnight, ServiceMode::Delivery)
                .is_none()
        );
        assert!(
            schedule
                .service_at(after_cutoff, ServiceMode::Delivery)
                .is_some()
        );
        schedule
            .exceptions
            .insert(date.succ_opt().unwrap(), Windows::default());
        assert!(
            schedule
                .service_at(after_cutoff, ServiceMode::Delivery)
                .is_none()
        );
        assert!("11:00-15:00, 12:00-13:00".parse::<Windows>().is_err());
        assert!("09:00-09:00".parse::<Windows>().is_err());
    }
}

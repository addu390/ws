use chrono::{DateTime, NaiveDate, Utc};

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

/// The trading day in Eastern Time, where TSX and US markets run, wherever the user is.
#[must_use]
pub fn exchange_day(now: DateTime<Utc>) -> NaiveDate {
    now.with_timezone(&EXCHANGE_TIME).date_naive()
}

const EXCHANGE_TIME: chrono_tz::Tz = chrono_tz::America::New_York;

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FixedClock(DateTime<Utc>);

impl FixedClock {
    #[must_use]
    pub fn at(instant: DateTime<Utc>) -> Self {
        Self(instant)
    }
}

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn exchange_day_follows_eastern_time() {
        let late_evening = Utc.with_ymd_and_hms(2026, 10, 3, 2, 0, 0).single().unwrap_or_default();
        assert_eq!(exchange_day(late_evening), NaiveDate::from_ymd_opt(2026, 10, 2).unwrap_or_default());
    }
}

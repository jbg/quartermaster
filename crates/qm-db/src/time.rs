use std::str::FromStr;

use jiff::{civil::Date, Timestamp, Zoned};

pub fn now_timestamp() -> Timestamp {
    Timestamp::now()
}

pub fn format_timestamp(ts: Timestamp) -> String {
    format!("{ts:.3}")
}

pub fn now_utc_rfc3339() -> String {
    format_timestamp(now_timestamp())
}

pub fn parse_timestamp(s: &str) -> Result<Timestamp, sqlx::Error> {
    Timestamp::from_str(s).map_err(|e| sqlx::Error::Decode(Box::new(e)))
}

pub fn parse_date(s: &str) -> Result<Date, sqlx::Error> {
    Date::from_str(s).map_err(|e| sqlx::Error::Decode(Box::new(e)))
}

pub fn format_date(date: Date) -> String {
    date.to_string()
}

pub fn format_zoned_with_offset(zoned: &Zoned) -> String {
    zoned.timestamp().display_with_offset(zoned.offset()).to_string()
}

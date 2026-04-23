use chrono::{DateTime, Utc};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TimeError {
    #[error("invalid rfc3339 timestamp")]
    Invalid(#[from] chrono::ParseError),
}

pub fn now_unix() -> i64 {
    Utc::now().timestamp()
}

pub fn parse_rfc3339(ts: &str) -> Result<DateTime<Utc>, TimeError> {
    Ok(DateTime::parse_from_rfc3339(ts)?.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rfc3339_timestamp() {
        let out = parse_rfc3339("2026-01-01T00:00:00Z").expect("must parse");
        assert_eq!(out.timestamp(), 1767225600);
    }
}

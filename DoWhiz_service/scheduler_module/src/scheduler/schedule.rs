use chrono::{DateTime, Utc};
use cron::Schedule as CronSchedule;
use std::str::FromStr;

use super::types::SchedulerError;

const WEEKDAY_REQUEST_MARKERS: &[&str] = &[
    "weekday",
    "weekdays",
    "business day",
    "business days",
    "monday through friday",
    "monday to friday",
    "mon-fri",
    "mon thru fri",
];

pub(crate) fn validate_cron_expression(expression: &str) -> Result<(), SchedulerError> {
    let fields = expression.split_whitespace().count();
    if fields != 6 {
        return Err(SchedulerError::InvalidCron(fields));
    }
    Ok(())
}

pub(crate) fn next_run_after(
    expression: &str,
    after: DateTime<Utc>,
) -> Result<DateTime<Utc>, SchedulerError> {
    validate_cron_expression(expression)?;
    let schedule = CronSchedule::from_str(expression)?;
    for datetime in schedule.after(&after) {
        if datetime > after {
            return Ok(datetime);
        }
    }
    Err(SchedulerError::NoNextRun)
}

/// Normalize legacy ambiguous weekday cron expressions for explicit weekday requests.
///
/// The cron parser used by the scheduler does not follow the common Unix-cron numeric weekday
/// convention. In practice, numeric weekday ranges like `1-5` are ambiguous for model output and
/// have historically produced Sunday-Thursday runs when the user asked for Monday-Friday
/// weekdays. When the request text clearly asks for a Monday-Friday cadence, rewrite legacy
/// numeric weekday fields to the unambiguous `MON-FRI`.
pub(crate) fn normalize_weekday_cron_expression(
    expression: &str,
    request_context: Option<&str>,
) -> String {
    let Some(request_context) = request_context else {
        return expression.to_string();
    };
    if !request_implies_monday_through_friday(request_context) {
        return expression.to_string();
    }

    let mut fields: Vec<&str> = expression.split_whitespace().collect();
    if fields.len() != 6 || !is_legacy_weekday_field(fields[5]) {
        return expression.to_string();
    }

    fields[5] = "MON-FRI";
    fields.join(" ")
}

fn request_implies_monday_through_friday(text: &str) -> bool {
    let normalized = text.to_ascii_lowercase();
    WEEKDAY_REQUEST_MARKERS
        .iter()
        .any(|marker| normalized.contains(marker))
}

fn is_legacy_weekday_field(field: &str) -> bool {
    matches!(field.trim(), "0-4" | "0,1,2,3,4" | "1-5" | "1,2,3,4,5")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn validate_cron_expression_requires_six_fields() {
        assert!(validate_cron_expression("0 0 9 * * *").is_ok());

        let err = validate_cron_expression("0 9 * * *").expect_err("five fields invalid");
        assert!(matches!(err, SchedulerError::InvalidCron(5)));
    }

    #[test]
    fn next_run_after_returns_same_day_future_occurrence() {
        let after = Utc.with_ymd_and_hms(2026, 6, 30, 8, 59, 0).unwrap();
        let expected = Utc.with_ymd_and_hms(2026, 6, 30, 9, 0, 0).unwrap();

        assert_eq!(next_run_after("0 0 9 * * *", after).unwrap(), expected);
    }

    #[test]
    fn next_run_after_is_strictly_after_reference_time() {
        let after = Utc.with_ymd_and_hms(2026, 6, 30, 9, 0, 0).unwrap();
        let expected = Utc.with_ymd_and_hms(2026, 7, 1, 9, 0, 0).unwrap();

        assert_eq!(next_run_after("0 0 9 * * *", after).unwrap(), expected);
    }
}

use anyhow::{anyhow, Result};
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, Utc, Weekday};

pub fn parse_natural_date(s: &str) -> Result<DateTime<Utc>> {
    let s = s.trim().to_lowercase();
    let today = Local::now().date_naive();

    // Try ISO date first (YYYY-MM-DD)
    if let Ok(naive) = NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
        return Ok(naive.and_hms_opt(0, 0, 0).unwrap().and_utc());
    }

    // Relative durations: 7d, 2w, 3m, 1y
    if s.len() >= 2 {
        let (num_part, unit) = s.split_at(s.len() - 1);
        if let Ok(n) = num_part.parse::<i64>() {
            let date = match unit {
                "d" => Some(today - Duration::days(n)),
                "w" => Some(today - Duration::weeks(n)),
                "m" => {
                    let target_month = today.month0() as i64 - n;
                    let years_back = if target_month < 0 {
                        ((-target_month - 1) / 12 + 1) as i32
                    } else {
                        0
                    };
                    let month = ((target_month % 12 + 12) % 12 + 1) as u32;
                    NaiveDate::from_ymd_opt(today.year() - years_back, month, today.day().min(28))
                }
                "y" => NaiveDate::from_ymd_opt(
                    today.year() - n as i32,
                    today.month(),
                    today.day().min(28),
                ),
                _ => None,
            };
            if let Some(d) = date {
                return Ok(d.and_hms_opt(0, 0, 0).unwrap().and_utc());
            }
        }
    }

    // Named ranges
    match s.as_str() {
        "today" => return Ok(today.and_hms_opt(0, 0, 0).unwrap().and_utc()),
        "yesterday" => {
            return Ok((today - Duration::days(1))
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc())
        }
        "last-week" | "lastweek" => {
            let days_since_monday = today.weekday().num_days_from_monday();
            let this_monday = today - Duration::days(days_since_monday as i64);
            let last_monday = this_monday - Duration::weeks(1);
            return Ok(last_monday.and_hms_opt(0, 0, 0).unwrap().and_utc());
        }
        "this-week" | "thisweek" => {
            let days_since_monday = today.weekday().num_days_from_monday();
            let monday = today - Duration::days(days_since_monday as i64);
            return Ok(monday.and_hms_opt(0, 0, 0).unwrap().and_utc());
        }
        "last-month" | "lastmonth" => {
            let (y, m) = if today.month() == 1 {
                (today.year() - 1, 12)
            } else {
                (today.year(), today.month() - 1)
            };
            let d = NaiveDate::from_ymd_opt(y, m, 1).unwrap();
            return Ok(d.and_hms_opt(0, 0, 0).unwrap().and_utc());
        }
        "this-month" | "thismonth" => {
            let d = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap();
            return Ok(d.and_hms_opt(0, 0, 0).unwrap().and_utc());
        }
        _ => {}
    }

    // Day names: find most recent past occurrence
    if let Some(weekday) = parse_weekday_name(&s) {
        let current_wd = today.weekday().num_days_from_monday();
        let target_wd = weekday.num_days_from_monday();
        let days_back = if current_wd > target_wd {
            current_wd - target_wd
        } else if current_wd < target_wd {
            7 - (target_wd - current_wd)
        } else {
            7 // same day = last week
        };
        let d = today - Duration::days(days_back as i64);
        return Ok(d.and_hms_opt(0, 0, 0).unwrap().and_utc());
    }

    Err(anyhow!(
        "Unrecognized date '{}'. Use YYYY-MM-DD, 7d, 2w, 3m, yesterday, last-week, etc.",
        s
    ))
}

/// Parse a date for use as an inclusive end-of-day upper bound (`--until`).
/// `parse_natural_date` snaps to midnight, which would make `--since X --until X`
/// an empty window. This variant resolves to the next midnight so the whole
/// day X is selected. Consumers should treat the result as an exclusive bound.
pub fn parse_natural_date_end(s: &str) -> Result<DateTime<Utc>> {
    Ok(parse_natural_date(s)? + Duration::days(1))
}

fn parse_weekday_name(s: &str) -> Option<Weekday> {
    match s {
        "monday" | "mon" => Some(Weekday::Mon),
        "tuesday" | "tue" => Some(Weekday::Tue),
        "wednesday" | "wed" => Some(Weekday::Wed),
        "thursday" | "thu" => Some(Weekday::Thu),
        "friday" | "fri" => Some(Weekday::Fri),
        "saturday" | "sat" => Some(Weekday::Sat),
        "sunday" | "sun" => Some(Weekday::Sun),
        _ => None,
    }
}

/// Parse a diff period name into (start, end) datetime range
pub fn parse_diff_period(s: &str) -> Result<(DateTime<Utc>, DateTime<Utc>)> {
    let s = s.trim().to_lowercase();
    let today = Local::now().date_naive();

    // Try as ISO date (single day)
    if let Ok(naive) = NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
        let start = naive.and_hms_opt(0, 0, 0).unwrap().and_utc();
        let end = (naive + Duration::days(1))
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        return Ok((start, end));
    }

    match s.as_str() {
        "today" => {
            let start = today.and_hms_opt(0, 0, 0).unwrap().and_utc();
            let end = (today + Duration::days(1))
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc();
            Ok((start, end))
        }
        "yesterday" => {
            let d = today - Duration::days(1);
            Ok((
                d.and_hms_opt(0, 0, 0).unwrap().and_utc(),
                today.and_hms_opt(0, 0, 0).unwrap().and_utc(),
            ))
        }
        "this-week" | "thisweek" => {
            let dow = today.weekday().num_days_from_monday();
            let monday = today - Duration::days(dow as i64);
            let next_monday = monday + Duration::weeks(1);
            Ok((
                monday.and_hms_opt(0, 0, 0).unwrap().and_utc(),
                next_monday.and_hms_opt(0, 0, 0).unwrap().and_utc(),
            ))
        }
        "last-week" | "lastweek" => {
            let dow = today.weekday().num_days_from_monday();
            let this_monday = today - Duration::days(dow as i64);
            let last_monday = this_monday - Duration::weeks(1);
            Ok((
                last_monday.and_hms_opt(0, 0, 0).unwrap().and_utc(),
                this_monday.and_hms_opt(0, 0, 0).unwrap().and_utc(),
            ))
        }
        "this-month" | "thismonth" => {
            let first = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap();
            let next = if today.month() == 12 {
                NaiveDate::from_ymd_opt(today.year() + 1, 1, 1).unwrap()
            } else {
                NaiveDate::from_ymd_opt(today.year(), today.month() + 1, 1).unwrap()
            };
            Ok((
                first.and_hms_opt(0, 0, 0).unwrap().and_utc(),
                next.and_hms_opt(0, 0, 0).unwrap().and_utc(),
            ))
        }
        "last-month" | "lastmonth" => {
            let first_this = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap();
            let (y, m) = if today.month() == 1 {
                (today.year() - 1, 12)
            } else {
                (today.year(), today.month() - 1)
            };
            let first_last = NaiveDate::from_ymd_opt(y, m, 1).unwrap();
            Ok((
                first_last.and_hms_opt(0, 0, 0).unwrap().and_utc(),
                first_this.and_hms_opt(0, 0, 0).unwrap().and_utc(),
            ))
        }
        _ => {
            // Try as a start date for a single-day range
            let start = parse_natural_date(&s)?;
            let end_date = start + chrono::Duration::days(1);
            Ok((start, end_date))
        }
    }
}

/// Parse a CO2 budget string like "50kg", "5000g", "1t" into grams
pub fn parse_co2_budget(s: &str) -> Result<f64> {
    let s = s.trim().to_lowercase();
    if let Some(n) = s.strip_suffix("kg") {
        Ok(n.trim().parse::<f64>()? * 1000.0)
    } else if let Some(n) = s.strip_suffix('t') {
        Ok(n.trim().parse::<f64>()? * 1_000_000.0)
    } else if let Some(n) = s.strip_suffix('g') {
        Ok(n.trim().parse::<f64>()?)
    } else {
        // Default to grams
        Ok(s.parse::<f64>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn until_bound_is_next_midnight() {
        let start = parse_natural_date("2026-03-10").unwrap();
        let end = parse_natural_date_end("2026-03-10").unwrap();
        assert_eq!(end - start, Duration::days(1));
        assert_eq!(
            end.format("%Y-%m-%d %H:%M:%S").to_string(),
            "2026-03-11 00:00:00"
        );
    }

    #[test]
    fn same_day_since_until_is_a_full_day_window() {
        for s in ["today", "yesterday", "7d", "monday", "last-week"] {
            let since = parse_natural_date(s).unwrap();
            let until = parse_natural_date_end(s).unwrap();
            assert_eq!(until - since, Duration::days(1), "input {s}");
        }
    }
}

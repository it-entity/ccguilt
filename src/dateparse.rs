use anyhow::{anyhow, Result};
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, Utc, Weekday};

fn eq_ci(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.as_bytes()
            .iter()
            .zip(b.as_bytes())
            .all(|(x, y)| x.eq_ignore_ascii_case(y))
}

fn strip_ascii_whitespace(s: &str) -> &str {
    let start = s.len() - s.trim_start().len();
    let end = s.len() - s.trim_end().len();
    &s[start..s.len() - end]
}

pub fn parse_natural_date(s: &str) -> Result<DateTime<Utc>> {
    let s = strip_ascii_whitespace(s);
    let today = Local::now().date_naive();

    if let Ok(naive) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Ok(naive.and_hms_opt(0, 0, 0).unwrap().and_utc());
    }

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

    if eq_ci(s, "today") {
        return Ok(today.and_hms_opt(0, 0, 0).unwrap().and_utc());
    }
    if eq_ci(s, "yesterday") {
        return Ok((today - Duration::days(1))
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc());
    }
    if eq_ci(s, "last-week") || eq_ci(s, "lastweek") {
        let days_since_monday = today.weekday().num_days_from_monday();
        let this_monday = today - Duration::days(days_since_monday as i64);
        let last_monday = this_monday - Duration::weeks(1);
        return Ok(last_monday.and_hms_opt(0, 0, 0).unwrap().and_utc());
    }
    if eq_ci(s, "this-week") || eq_ci(s, "thisweek") {
        let days_since_monday = today.weekday().num_days_from_monday();
        let monday = today - Duration::days(days_since_monday as i64);
        return Ok(monday.and_hms_opt(0, 0, 0).unwrap().and_utc());
    }
    if eq_ci(s, "last-month") || eq_ci(s, "lastmonth") {
        let (y, m) = if today.month() == 1 {
            (today.year() - 1, 12)
        } else {
            (today.year(), today.month() - 1)
        };
        let d = NaiveDate::from_ymd_opt(y, m, 1).unwrap();
        return Ok(d.and_hms_opt(0, 0, 0).unwrap().and_utc());
    }
    if eq_ci(s, "this-month") || eq_ci(s, "thismonth") {
        let d = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap();
        return Ok(d.and_hms_opt(0, 0, 0).unwrap().and_utc());
    }

    // Day names: find most recent past occurrence
    if let Some(weekday) = parse_weekday_name(s) {
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

fn parse_weekday_name(s: &str) -> Option<Weekday> {
    if eq_ci(s, "monday") || eq_ci(s, "mon") {
        Some(Weekday::Mon)
    } else if eq_ci(s, "tuesday") || eq_ci(s, "tue") {
        Some(Weekday::Tue)
    } else if eq_ci(s, "wednesday") || eq_ci(s, "wed") {
        Some(Weekday::Wed)
    } else if eq_ci(s, "thursday") || eq_ci(s, "thu") {
        Some(Weekday::Thu)
    } else if eq_ci(s, "friday") || eq_ci(s, "fri") {
        Some(Weekday::Fri)
    } else if eq_ci(s, "saturday") || eq_ci(s, "sat") {
        Some(Weekday::Sat)
    } else if eq_ci(s, "sunday") || eq_ci(s, "sun") {
        Some(Weekday::Sun)
    } else {
        None
    }
}

pub fn parse_diff_period(s: &str) -> Result<(DateTime<Utc>, DateTime<Utc>)> {
    let s = strip_ascii_whitespace(s);
    let today = Local::now().date_naive();

    if let Ok(naive) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let start = naive.and_hms_opt(0, 0, 0).unwrap().and_utc();
        let end = (naive + Duration::days(1))
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        return Ok((start, end));
    }

    if eq_ci(s, "today") {
        let start = today.and_hms_opt(0, 0, 0).unwrap().and_utc();
        let end = (today + Duration::days(1))
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        Ok((start, end))
    } else if eq_ci(s, "yesterday") {
        let d = today - Duration::days(1);
        Ok((
            d.and_hms_opt(0, 0, 0).unwrap().and_utc(),
            today.and_hms_opt(0, 0, 0).unwrap().and_utc(),
        ))
    } else if eq_ci(s, "this-week") || eq_ci(s, "thisweek") {
        let dow = today.weekday().num_days_from_monday();
        let monday = today - Duration::days(dow as i64);
        let next_monday = monday + Duration::weeks(1);
        Ok((
            monday.and_hms_opt(0, 0, 0).unwrap().and_utc(),
            next_monday.and_hms_opt(0, 0, 0).unwrap().and_utc(),
        ))
    } else if eq_ci(s, "last-week") || eq_ci(s, "lastweek") {
        let dow = today.weekday().num_days_from_monday();
        let this_monday = today - Duration::days(dow as i64);
        let last_monday = this_monday - Duration::weeks(1);
        Ok((
            last_monday.and_hms_opt(0, 0, 0).unwrap().and_utc(),
            this_monday.and_hms_opt(0, 0, 0).unwrap().and_utc(),
        ))
    } else if eq_ci(s, "this-month") || eq_ci(s, "thismonth") {
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
    } else if eq_ci(s, "last-month") || eq_ci(s, "lastmonth") {
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
    } else {
        let start = parse_natural_date(s)?;
        let end_date = start + chrono::Duration::days(1);
        Ok((start, end_date))
    }
}

pub fn parse_co2_budget(s: &str) -> Result<f64> {
    let s = s.trim();
    if let Some(n) = s.strip_suffix("kg").or_else(|| {
        let l = s.len();
        if l > 2 {
            let bytes = s.as_bytes();
            if bytes[l - 2].eq_ignore_ascii_case(&b'K') && bytes[l - 1].eq_ignore_ascii_case(&b'G')
            {
                Some(&s[..l - 2])
            } else {
                None
            }
        } else {
            None
        }
    }) {
        Ok(n.trim().parse::<f64>()? * 1000.0)
    } else if let Some(n) = s.strip_suffix('t').or_else(|| {
        s.len().checked_sub(1).and_then(|l| {
            if s.as_bytes()[l].eq_ignore_ascii_case(&b'T') {
                Some(&s[..l])
            } else {
                None
            }
        })
    }) {
        Ok(n.trim().parse::<f64>()? * 1_000_000.0)
    } else if let Some(n) = s.strip_suffix('g').or_else(|| {
        s.len().checked_sub(1).and_then(|l| {
            if s.as_bytes()[l].eq_ignore_ascii_case(&b'G') {
                Some(&s[..l])
            } else {
                None
            }
        })
    }) {
        Ok(n.trim().parse::<f64>()?)
    } else {
        Ok(s.parse::<f64>()?)
    }
}

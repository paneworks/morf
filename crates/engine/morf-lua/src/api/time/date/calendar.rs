//! `morf.time`'s calendar and wording: days between dates, a month's length
//! and grid, leap years and weekdays, and times and durations as words.

use super::*;

/// "just now", "5 minutes ago", "in 2 days", "yesterday".
fn relative(delta: f64) -> String {
    let future = delta < 0.0;
    let seconds = delta.abs();
    let (count, unit) = if seconds < 45.0 {
        return "just now".to_owned();
    } else if seconds < 90.0 {
        (1.0, "minute")
    } else if seconds < 45.0 * 60.0 {
        ((seconds / 60.0).round(), "minute")
    } else if seconds < 90.0 * 60.0 {
        (1.0, "hour")
    } else if seconds < 22.0 * 3600.0 {
        ((seconds / 3600.0).round(), "hour")
    } else if seconds < 36.0 * 3600.0 {
        return if future { "tomorrow" } else { "yesterday" }.to_owned();
    } else if seconds < 26.0 * 86400.0 {
        ((seconds / 86400.0).round(), "day")
    } else if seconds < 320.0 * 86400.0 {
        ((seconds / (30.44 * 86400.0)).round().max(1.0), "month")
    } else {
        ((seconds / (365.25 * 86400.0)).round().max(1.0), "year")
    };
    let plural = if count == 1.0 { "" } else { "s" };
    if future {
        format!("in {count} {unit}{plural}")
    } else {
        format!("{count} {unit}{plural} ago")
    }
}

/// Installs the calendar and wording calls on `time`.
pub(super) fn install_calendar<'gc>(ctx: Context<'gc>, time: Table<'gc>) {
    // days_between(a, b) — whole calendar days from a's date to b's.
    time.set_field(
        ctx,
        "days_between",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (a, b, options): (LuaValue, LuaValue, LuaValue) = stack.consume(ctx)?;
            let a = zoned_at(ctx, a, options)?.date();
            let b = zoned_at(ctx, b, options)?.date();
            let span = a
                .until((jiff::Unit::Day, b))
                .map_err(|error| HostError(error.to_string()))?;
            stack.replace(ctx, i64::from(span.get_days()));
            Ok(CallbackReturn::Return)
        }),
    );

    time.set_field(
        ctx,
        "days_in_month",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (year, month): (i64, i64) = stack.consume(ctx)?;
            let date = civil_date(year, month, 1)?;
            stack.replace(ctx, i64::from(date.days_in_month()));
            Ok(CallbackReturn::Return)
        }),
    );

    time.set_field(
        ctx,
        "is_leap_year",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let year: i64 = stack.consume(ctx)?;
            stack.replace(ctx, civil_date(year, 1, 1)?.in_leap_year());
            Ok(CallbackReturn::Return)
        }),
    );

    // weekday(year, month, day) — 1 for Monday through 7 for Sunday.
    time.set_field(
        ctx,
        "weekday",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (year, month, day): (i64, i64, i64) = stack.consume(ctx)?;
            let date = civil_date(year, month, day)?;
            stack.replace(ctx, i64::from(date.weekday().to_monday_one_offset()));
            Ok(CallbackReturn::Return)
        }),
    );

    // month(year, month) — the weeks of a month for a calendar grid: rows
    // of seven { year, month, day, current } starting on Monday.
    time.set_field(
        ctx,
        "month",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (year, month, first_weekday): (i64, i64, Option<i64>) = stack.consume(ctx)?;
            let first = civil_date(year, month, 1)?;
            let week_start = first_weekday.unwrap_or(1).clamp(1, 7);
            let offset =
                (i64::from(first.weekday().to_monday_one_offset()) - week_start).rem_euclid(7);
            let mut day = first
                .checked_sub(Span::new().days(offset))
                .map_err(|error| HostError(error.to_string()))?;
            let rows = Table::new(&ctx);
            let weeks = (offset + i64::from(first.days_in_month()) + 6) / 7;
            for week in 0..weeks {
                let row = Table::new(&ctx);
                for column in 0..7 {
                    let cell = Table::new(&ctx);
                    cell.set_field(ctx, "year", i64::from(day.year()));
                    cell.set_field(ctx, "month", i64::from(day.month()));
                    cell.set_field(ctx, "day", i64::from(day.day()));
                    cell.set_field(ctx, "current", i64::from(day.month()) == month);
                    row.set(ctx, column + 1, cell)?;
                    day = day
                        .tomorrow()
                        .map_err(|error| HostError(error.to_string()))?;
                }
                rows.set(ctx, week + 1, row)?;
            }
            stack.replace(ctx, rows);
            Ok(CallbackReturn::Return)
        }),
    );

    // relative(time, [now]) — "5 minutes ago", "in 2 days".
    time.set_field(
        ctx,
        "relative",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (at, now): (f64, Option<f64>) = stack.consume(ctx)?;
            let now = now.unwrap_or_else(|| seconds_of(Timestamp::now()));
            stack.replace(ctx, relative(now - at));
            Ok(CallbackReturn::Return)
        }),
    );

    // duration(seconds, [style]) — "1:05:09", or "1h 5m" with "short".
    time.set_field(
        ctx,
        "duration",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (seconds, style): (f64, Option<String>) = stack.consume(ctx)?;
            let negative = seconds < 0.0;
            let total = seconds.abs().floor() as u64;
            let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
            let body = match style.as_deref() {
                Some("short") => {
                    if h > 0 {
                        if m > 0 {
                            format!("{h}h {m}m")
                        } else {
                            format!("{h}h")
                        }
                    } else if m > 0 {
                        if s > 0 && m < 10 {
                            format!("{m}m {s}s")
                        } else {
                            format!("{m}m")
                        }
                    } else {
                        format!("{s}s")
                    }
                }
                None | Some("clock") => {
                    if h > 0 {
                        format!("{h}:{m:02}:{s:02}")
                    } else {
                        format!("{m}:{s:02}")
                    }
                }
                Some(other) => {
                    return Err(HostError(format!(
                        "duration style {other:?} is not clock or short"
                    ))
                    .into());
                }
            };
            stack.replace(ctx, if negative { format!("-{body}") } else { body });
            Ok(CallbackReturn::Return)
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::relative;

    #[test]
    fn relative_times_read_like_speech() {
        assert_eq!(relative(10.0), "just now");
        assert_eq!(relative(60.0), "1 minute ago");
        assert_eq!(relative(600.0), "10 minutes ago");
        assert_eq!(relative(-7200.0), "in 2 hours");
        assert_eq!(relative(30.0 * 3600.0), "yesterday");
        assert_eq!(relative(-3.0 * 86400.0), "in 3 days");
        assert_eq!(relative(400.0 * 86400.0), "1 year ago");
    }
}

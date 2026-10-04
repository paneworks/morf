//! `morf.time`: dates, as a calendar has them.
//!
//! An instant is a number, seconds since the epoch with the fraction, so it
//! stores in JSON, sorts, and subtracts. Everything that needs a calendar —
//! a month's length, "next Tuesday", the day a task falls on — goes through
//! a time zone: the system's unless the call names one (`tz = "UTC"`,
//! `tz = "Europe/Rome"`). Arithmetic in days and months is calendar
//! arithmetic, so adding a day across a clock change is still a day.

use jiff::civil::{Date, DateTime, Time};
use jiff::tz::TimeZone;
use jiff::{Span, Timestamp, Zoned};
use luna::{Callback, CallbackReturn, Context, Table, Value as LuaValue};

use crate::scene_bindings::*;

mod calendar;

fn instant(seconds: f64) -> Result<Timestamp, HostError> {
    if !seconds.is_finite() {
        return Err(HostError("time must be a finite number of seconds".into()));
    }
    let whole = seconds.floor();
    let nanos = ((seconds - whole) * 1e9).round().min(999_999_999.0) as i32;
    Timestamp::new(whole as i64, nanos).map_err(|error| HostError(error.to_string()))
}

fn seconds_of(timestamp: Timestamp) -> f64 {
    timestamp.as_second() as f64 + f64::from(timestamp.subsec_nanosecond()) / 1e9
}

fn number(value: LuaValue<'_>) -> Option<f64> {
    match value {
        LuaValue::Integer(value) => Some(value as f64),
        LuaValue::Number(value) => Some(value),
        _ => None,
    }
}

/// The zone an options table names, or the system's.
fn zone<'gc>(ctx: Context<'gc>, options: LuaValue<'gc>) -> Result<TimeZone, HostError> {
    let named = match options {
        LuaValue::Nil => return Ok(TimeZone::system()),
        LuaValue::String(name) => name.to_str().map(str::to_owned).ok(),
        LuaValue::Table(table) => match table.get_value(ctx, "tz") {
            LuaValue::Nil => {
                if matches!(table.get_value(ctx, "utc"), LuaValue::Boolean(true)) {
                    return Ok(TimeZone::UTC);
                }
                return Ok(TimeZone::system());
            }
            LuaValue::String(name) => name.to_str().map(str::to_owned).ok(),
            _ => None,
        },
        _ => None,
    };
    let name = named.ok_or_else(|| HostError("time zone must be a name such as \"UTC\"".into()))?;
    if name.eq_ignore_ascii_case("utc") {
        return Ok(TimeZone::UTC);
    }
    if name == "local" || name == "system" {
        return Ok(TimeZone::system());
    }
    TimeZone::get(&name).map_err(|error| HostError(format!("time zone {name}: {error}")))
}

fn zoned_at<'gc>(
    ctx: Context<'gc>,
    at: LuaValue<'gc>,
    options: LuaValue<'gc>,
) -> Result<Zoned, HostError> {
    let tz = zone(ctx, options)?;
    let timestamp = match at {
        LuaValue::Nil => Timestamp::now(),
        other => instant(
            number(other).ok_or_else(|| HostError("time must be a number of seconds".into()))?,
        )?,
    };
    Ok(timestamp.to_zoned(tz))
}

fn date_table<'gc>(ctx: Context<'gc>, zoned: &Zoned) -> Table<'gc> {
    let table = Table::new(&ctx);
    table.set_field(ctx, "year", i64::from(zoned.year()));
    table.set_field(ctx, "month", i64::from(zoned.month()));
    table.set_field(ctx, "day", i64::from(zoned.day()));
    table.set_field(ctx, "hour", i64::from(zoned.hour()));
    table.set_field(ctx, "minute", i64::from(zoned.minute()));
    table.set_field(ctx, "second", i64::from(zoned.second()));
    table.set_field(ctx, "millisecond", i64::from(zoned.millisecond()));
    table.set_field(
        ctx,
        "weekday",
        i64::from(zoned.weekday().to_monday_one_offset()),
    );
    table.set_field(ctx, "yearday", i64::from(zoned.day_of_year()));
    table.set_field(ctx, "days_in_month", i64::from(zoned.days_in_month()));
    table.set_field(ctx, "leap_year", zoned.in_leap_year());
    table.set_field(ctx, "time", seconds_of(zoned.timestamp()));
    table.set_field(ctx, "offset", i64::from(zoned.offset().seconds()));
    table.set_field(ctx, "timezone", zoned.strftime("%Z").to_string());
    table.set_field(
        ctx,
        "zone",
        zoned.time_zone().iana_name().unwrap_or("").to_owned(),
    );
    table.set_field(
        ctx,
        "iso_week",
        i64::from(zoned.date().iso_week_date().week()),
    );
    table
}

fn field<'gc>(
    ctx: Context<'gc>,
    table: Table<'gc>,
    name: &str,
    default: i64,
) -> Result<i64, HostError> {
    match table.get_value(ctx, name) {
        LuaValue::Nil => Ok(default),
        LuaValue::Integer(value) => Ok(value),
        LuaValue::Number(value) if value.fract() == 0.0 => Ok(value as i64),
        _ => Err(HostError(format!("time field {name} must be an integer"))),
    }
}

fn narrow<T: TryFrom<i64>>(value: i64, name: &str) -> Result<T, HostError> {
    T::try_from(value).map_err(|_| HostError(format!("time field {name} is out of range")))
}

fn civil_date(year: i64, month: i64, day: i64) -> Result<Date, HostError> {
    Date::new(
        narrow(year, "year")?,
        narrow(month, "month")?,
        narrow(day, "day")?,
    )
    .map_err(|error| HostError(error.to_string()))
}

/// A span from `{ years, months, weeks, days, hours, minutes, seconds,
/// milliseconds }`, any of them negative.
fn span_of<'gc>(ctx: Context<'gc>, table: Table<'gc>) -> Result<Span, HostError> {
    let mut span = Span::new();
    let apply = |span: Span, name: &str, value: i64| -> Result<Span, HostError> {
        let out = match name {
            "years" => span.try_years(value),
            "months" => span.try_months(value),
            "weeks" => span.try_weeks(value),
            "days" => span.try_days(value),
            "hours" => span.try_hours(value),
            "minutes" => span.try_minutes(value),
            "seconds" => span.try_seconds(value),
            _ => span.try_milliseconds(value),
        };
        out.map_err(|error| HostError(format!("time span {name}: {error}")))
    };
    for name in [
        "years",
        "months",
        "weeks",
        "days",
        "hours",
        "minutes",
        "seconds",
        "milliseconds",
    ] {
        let value = field(ctx, table, name, 0)?;
        if value != 0 {
            span = apply(span, name, value)?;
        }
    }
    Ok(span)
}

pub(crate) fn install_date_api<'gc>(ctx: Context<'gc>, morf: Table<'gc>) {
    let time = Table::new(&ctx);

    time.set_field(
        ctx,
        "now",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            stack.replace(ctx, seconds_of(Timestamp::now()));
            Ok(CallbackReturn::Return)
        }),
    );
    time.set_field(
        ctx,
        "now_ms",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            stack.replace(ctx, Timestamp::now().as_millisecond());
            Ok(CallbackReturn::Return)
        }),
    );

    // format(pattern, [time], [options]) — strftime, in the zone.
    time.set_field(
        ctx,
        "format",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (pattern, at, options): (String, LuaValue, LuaValue) = stack.consume(ctx)?;
            if pattern.len() > 256 {
                return Err(HostError("time format exceeds 256 bytes".into()).into());
            }
            let zoned = zoned_at(ctx, at, options)?;
            let mut out = String::new();
            jiff::fmt::strtime::BrokenDownTime::from(&zoned)
                .format(&pattern, &mut out)
                .map_err(|error| HostError(format!("time format {pattern:?}: {error}")))?;
            stack.replace(ctx, out);
            Ok(CallbackReturn::Return)
        }),
    );

    // date([time], [options]) — the calendar fields of an instant.
    time.set_field(
        ctx,
        "date",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (at, options): (LuaValue, LuaValue) = stack.consume(ctx)?;
            let zoned = zoned_at(ctx, at, options)?;
            stack.replace(ctx, date_table(ctx, &zoned));
            Ok(CallbackReturn::Return)
        }),
    );

    // time({ year, month, day, hour, minute, second }, [options]) — the
    // instant those fields name; a missing time of day is midnight.
    time.set_field(
        ctx,
        "time",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (fields, options): (Table, LuaValue) = stack.consume(ctx)?;
            let tz = zone(ctx, options)?;
            let today = Timestamp::now().to_zoned(tz.clone());
            let date = civil_date(
                field(ctx, fields, "year", i64::from(today.year()))?,
                field(ctx, fields, "month", 1)?,
                field(ctx, fields, "day", 1)?,
            )?;
            let clock = Time::new(
                narrow(field(ctx, fields, "hour", 0)?, "hour")?,
                narrow(field(ctx, fields, "minute", 0)?, "minute")?,
                narrow(field(ctx, fields, "second", 0)?, "second")?,
                0,
            )
            .map_err(|error| HostError(error.to_string()))?;
            let zoned = DateTime::from_parts(date, clock)
                .to_zoned(tz)
                .map_err(|error| HostError(error.to_string()))?;
            stack.replace(ctx, seconds_of(zoned.timestamp()));
            Ok(CallbackReturn::Return)
        }),
    );

    // parse(text, [pattern], [options]) — with a pattern, strptime in the
    // zone; without, RFC 3339 / ISO 8601 or a bare date. nil, message on
    // text that does not fit.
    time.set_field(
        ctx,
        "parse",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (text, pattern, options): (String, Option<String>, LuaValue) =
                stack.consume(ctx)?;
            let tz = zone(ctx, options)?;
            let parsed: Result<Timestamp, String> = match pattern {
                Some(pattern) => jiff::fmt::strtime::parse(&pattern, &text)
                    .map_err(|error| error.to_string())
                    .and_then(|broken| {
                        if broken.offset().is_some() {
                            broken.to_timestamp().map_err(|error| error.to_string())
                        } else if let Ok(datetime) = broken.to_datetime() {
                            tz.to_timestamp(datetime).map_err(|error| error.to_string())
                        } else {
                            broken
                                .to_date()
                                .map_err(|error| error.to_string())
                                .and_then(|date| {
                                    tz.to_timestamp(date.to_datetime(Time::midnight()))
                                        .map_err(|error| error.to_string())
                                })
                        }
                    }),
                None => text
                    .parse::<Timestamp>()
                    .or_else(|_| {
                        text.parse::<DateTime>()
                            .map_err(|error| error.to_string())
                            .and_then(|datetime| {
                                tz.to_timestamp(datetime).map_err(|error| error.to_string())
                            })
                    })
                    .or_else(|_| {
                        text.parse::<Date>()
                            .map_err(|error| error.to_string())
                            .and_then(|date| {
                                tz.to_timestamp(date.to_datetime(Time::midnight()))
                                    .map_err(|error| error.to_string())
                            })
                    }),
            };
            match parsed {
                Ok(timestamp) => stack.replace(ctx, seconds_of(timestamp)),
                Err(error) => stack.replace(ctx, (LuaValue::Nil, error)),
            }
            Ok(CallbackReturn::Return)
        }),
    );

    // add(time, { days = 1, months = -2, ... }, [options])
    time.set_field(
        ctx,
        "add",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (at, span, options): (LuaValue, Table, LuaValue) = stack.consume(ctx)?;
            let zoned = zoned_at(ctx, at, options)?;
            let span = span_of(ctx, span)?;
            let moved = zoned
                .checked_add(span)
                .map_err(|error| HostError(error.to_string()))?;
            stack.replace(ctx, seconds_of(moved.timestamp()));
            Ok(CallbackReturn::Return)
        }),
    );

    // start_of(time, "minute" | "hour" | "day" | "week" | "month" | "year")
    time.set_field(
        ctx,
        "start_of",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (at, unit, options): (LuaValue, String, LuaValue) = stack.consume(ctx)?;
            let zoned = zoned_at(ctx, at, options)?;
            let date = zoned.date();
            let (date, clock) = match unit.as_str() {
                "minute" => (
                    date,
                    Time::new(zoned.hour(), zoned.minute(), 0, 0).unwrap_or(Time::midnight()),
                ),
                "hour" => (
                    date,
                    Time::new(zoned.hour(), 0, 0, 0).unwrap_or(Time::midnight()),
                ),
                "day" => (date, Time::midnight()),
                "week" => {
                    let back = i64::from(zoned.weekday().to_monday_zero_offset());
                    let monday = date
                        .checked_sub(Span::new().days(back))
                        .map_err(|error| HostError(error.to_string()))?;
                    (monday, Time::midnight())
                }
                "month" => (date.first_of_month(), Time::midnight()),
                "year" => (date.first_of_year(), Time::midnight()),
                other => {
                    return Err(HostError(format!(
                        "start_of unit {other:?} is not minute, hour, day, week, month or year"
                    ))
                    .into());
                }
            };
            let start = DateTime::from_parts(date, clock)
                .to_zoned(zoned.time_zone().clone())
                .map_err(|error| HostError(error.to_string()))?;
            stack.replace(ctx, seconds_of(start.timestamp()));
            Ok(CallbackReturn::Return)
        }),
    );

    calendar::install_calendar(ctx, time);

    time.set_field(
        ctx,
        "timezone",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let tz = TimeZone::system();
            stack.replace(ctx, tz.iana_name().unwrap_or("UTC").to_owned());
            Ok(CallbackReturn::Return)
        }),
    );

    morf.set_field(ctx, "time", time);
}

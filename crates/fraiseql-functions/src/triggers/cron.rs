//! Cron-scheduled triggers.
//!
//! Handles `cron:<expression>` triggers that fire on a schedule using POSIX cron expressions.
//!
//! - [`CronSchedule`] matches a 5-field POSIX expression against a date and time.
//! - [`Schedule`] reads one on a timezone's wall clock (UTC by default), with cron's
//!   daylight-saving convention for jobs at a fixed time of day.
//! - [`CronExecutionState`] keeps a window from firing twice. The server seeds it from
//!   `_fraiseql_cron_state` at boot, so a restart mid-window does not fire again.
//!
//! ## Example Expressions
//!
//! - `0 * * * *` - Every hour
//! - `0 2 * * *` - Daily at 2 AM
//! - `*/5 * * * *` - Every 5 minutes
//! - `0 0 1 * *` - First day of every month

use fraiseql_time::{
    Timestamp,
    jiff::{
        SignedDuration,
        civil::DateTime,
        tz::{AmbiguousOffset, TimeZone},
    },
};
use serde::{Deserialize, Serialize};

use crate::types::EventPayload;

/// A parsed cron field that can be a number, wildcard, or step value.
#[derive(Debug, Clone)]
enum CronField {
    /// Wildcard (*) matches any value
    Any,
    /// Specific value
    Value(u32),
    /// Step value (*/5 matches every 5th)
    Step { base: Option<u32>, step: u32 },
    /// List of values (1,3,5)
    List(Vec<u32>),
    /// Range (1-5)
    Range { start: u32, end: u32 },
}

impl CronField {
    /// `*` or `*/n`: the field names no particular value. cron's daylight-saving rules
    /// treat a job with such a minute or hour field as running "all the time".
    const fn is_wildcard(&self) -> bool {
        matches!(self, Self::Any | Self::Step { base: None, .. })
    }

    /// Check if this field matches the given value.
    fn matches(&self, value: u32) -> bool {
        match self {
            CronField::Any => true,
            CronField::Value(v) => *v == value,
            CronField::Step { base, step } => {
                let start = base.unwrap_or(0);
                if value < start {
                    return false;
                }
                (value - start).is_multiple_of(*step)
            },
            CronField::List(values) => values.contains(&value),
            CronField::Range { start, end } => value >= *start && value <= *end,
        }
    }

    /// Parse a cron field from a string.
    fn parse(field: &str) -> Result<Self, String> {
        if field == "*" {
            return Ok(CronField::Any);
        }

        if let Some(step_idx) = field.find('/') {
            let (base_part, step_part) = field.split_at(step_idx);
            let step_str = &step_part[1..]; // Skip the '/'

            let step = step_str
                .parse::<u32>()
                .map_err(|_| format!("Invalid step value: {}", step_str))?;

            let base = if base_part == "*" {
                None
            } else {
                Some(
                    base_part
                        .parse::<u32>()
                        .map_err(|_| format!("Invalid base value: {}", base_part))?,
                )
            };

            return Ok(CronField::Step { base, step });
        }

        if let Some(dash_idx) = field.find('-') {
            let (start_str, end_str) = field.split_at(dash_idx);
            let end_str = &end_str[1..]; // Skip the '-'

            let start = start_str
                .parse::<u32>()
                .map_err(|_| format!("Invalid range start: {}", start_str))?;
            let end =
                end_str.parse::<u32>().map_err(|_| format!("Invalid range end: {}", end_str))?;

            return Ok(CronField::Range { start, end });
        }

        if field.contains(',') {
            let values = field
                .split(',')
                .map(|v| v.parse::<u32>().map_err(|_| format!("Invalid list value: {}", v)))
                .collect::<Result<Vec<u32>, String>>()?;

            return Ok(CronField::List(values));
        }

        let value = field
            .parse::<u32>()
            .map_err(|_| format!("Invalid cron field value: {}", field))?;

        Ok(CronField::Value(value))
    }

    /// Refuse a field that can never match inside `min..=max`, naming the field.
    ///
    /// Each refused shape used to parse and then never match: minute `61`, a step of
    /// `0`, a range whose start is past its end. A schedule built from one never fires,
    /// so a function or source declared on it silently never runs (#1399).
    fn within(self, name: &str, min: u32, max: u32) -> Result<Self, String> {
        let out_of_range = |value: u32| {
            if (min..=max).contains(&value) {
                Ok(())
            } else {
                Err(format!("{name} value {value} is outside {min}-{max}"))
            }
        };
        match &self {
            CronField::Any => {},
            CronField::Value(value) => out_of_range(*value)?,
            CronField::List(values) => values.iter().try_for_each(|value| out_of_range(*value))?,
            CronField::Step { base, step } => {
                if *step == 0 {
                    return Err(format!("{name} step must be at least 1"));
                }
                if let Some(base) = base {
                    out_of_range(*base)?;
                }
            },
            CronField::Range { start, end } => {
                out_of_range(*start)?;
                out_of_range(*end)?;
                if start > end {
                    return Err(format!("{name} range {start}-{end} runs backwards"));
                }
            },
        }
        Ok(self)
    }
}

/// A cron expression with validation.
#[derive(Debug, Clone)]
pub struct CronSchedule {
    /// The raw cron expression (e.g., "0 2 * * *").
    pub expression: String,
    /// Parsed minute field (0-59)
    minute:         CronField,
    /// Parsed hour field (0-23)
    hour:           CronField,
    /// Parsed day-of-month field (1-31)
    day:            CronField,
    /// Parsed month field (1-12)
    month:          CronField,
    /// Parsed day-of-week field (0-7, 0 and 7 = Sunday)
    weekday:        CronField,
}

impl CronSchedule {
    /// Parse and validate a cron expression.
    ///
    /// # Errors
    ///
    /// Returns an error if the cron expression is invalid.
    pub fn parse(expression: &str) -> Result<Self, String> {
        let parts: Vec<&str> = expression.split_whitespace().collect();

        if parts.len() != 5 {
            return Err(format!("Cron expression must have 5 fields, got {}", parts.len()));
        }

        let minute = CronField::parse(parts[0])?.within("minute", 0, 59)?;
        let hour = CronField::parse(parts[1])?.within("hour", 0, 23)?;
        let day = CronField::parse(parts[2])?.within("day-of-month", 1, 31)?;
        let month = CronField::parse(parts[3])?.within("month", 1, 12)?;
        // 7 is POSIX's alternate Sunday (see `matches`).
        let weekday = CronField::parse(parts[4])?.within("day-of-week", 0, 7)?;

        Ok(CronSchedule {
            expression: expression.to_string(),
            minute,
            hour,
            day,
            month,
            weekday,
        })
    }

    /// Whether the job runs at a fixed time of day: neither its minute nor its hour is a
    /// wildcard. Only such jobs get cron's daylight-saving treatment (see [`Schedule`]).
    #[must_use]
    pub const fn is_fixed_time(&self) -> bool {
        !self.minute.is_wildcard() && !self.hour.is_wildcard()
    }

    /// Check if a wall-clock date and time (in whatever zone it is read) matches this
    /// cron schedule.
    #[must_use]
    pub fn matches(&self, datetime: DateTime) -> bool {
        let field = |value: i8| u32::try_from(value).unwrap_or(u32::MAX);
        let minute = field(datetime.minute());
        let hour = field(datetime.hour());
        let day = field(datetime.day());
        let month = field(datetime.month());
        // POSIX cron weekdays are 0=Sunday..6=Saturday (#841 shipped
        // `number_from_sunday()`, which is 1..=7, firing everything a day
        // early and making `0` unmatchable). `7` is the POSIX alternate
        // Sunday encoding, so a Sunday also matches a field containing 7.
        let weekday = field(datetime.weekday().to_sunday_zero_offset());
        let weekday_matches =
            self.weekday.matches(weekday) || (weekday == 0 && self.weekday.matches(7));

        self.minute.matches(minute)
            && self.hour.matches(hour)
            && self.day.matches(day)
            && self.month.matches(month)
            && weekday_matches
    }
}

/// A cron expression evaluated in a timezone.
///
/// The expression is matched against the wall clock of `tz`. Around a daylight-saving
/// change, a job at a fixed time of day ([`CronSchedule::is_fixed_time`]) follows cron's
/// convention:
///
/// - a local time that occurs twice (the clocks go back) fires on its first occurrence only;
/// - a local time the clocks skip (they go forward) fires at the first minute after the gap.
///
/// A job whose minute or hour is a wildcard runs on the wall clock as it is: through both
/// passes of a repeated hour, and not at all in a skipped one.
#[derive(Debug, Clone)]
pub struct Schedule {
    /// The cron expression.
    pub expr: CronSchedule,
    /// The timezone whose wall clock the expression is read on.
    pub tz:   TimeZone,
}

/// `at` on the UTC wall clock.
fn utc_wall_clock(at: Timestamp) -> DateTime {
    at.as_jiff().to_zoned(TimeZone::UTC).datetime()
}

impl Schedule {
    /// Parse `expression`, to be evaluated in the IANA `timezone` (UTC when `None`).
    ///
    /// # Errors
    ///
    /// Returns an error when the expression is invalid or the timezone is not an IANA
    /// name.
    pub fn parse(expression: &str, timezone: Option<&str>) -> Result<Self, String> {
        let tz = match timezone {
            None => TimeZone::UTC,
            // Named exactly, as the IANA database spells it: the lookup alone also takes
            // another case. `Factory` is the database's placeholder for a host whose zone
            // was never set, not a place, and is refused with the unknown names.
            Some(name) => TimeZone::get(name)
                .ok()
                .filter(|tz| tz.iana_name() == Some(name) && name != "Factory")
                .ok_or_else(|| {
                    format!(
                        "unknown timezone {name:?}: expected an IANA name such as \"Europe/Paris\""
                    )
                })?,
        };
        Ok(Self {
            expr: CronSchedule::parse(expression)?,
            tz,
        })
    }

    /// The IANA name of the zone the expression is read in, as it was given.
    #[must_use]
    pub fn zone_name(&self) -> &str {
        self.tz.iana_name().unwrap_or("UTC")
    }

    /// Decide whether the tick at `now` fires, given what `state` last fired.
    #[must_use]
    pub fn decide(&self, state: &CronExecutionState, now: &Timestamp) -> CronDecision {
        if self.is_due(now) {
            state.decide_window(now)
        } else {
            CronDecision::NotDue
        }
    }

    fn is_due(&self, now: &Timestamp) -> bool {
        let local = self.wall_clock(*now);
        if !self.expr.is_fixed_time() {
            return self.expr.matches(local);
        }
        if self.is_second_pass(*now, local) {
            return false;
        }
        self.expr.matches(local) || self.skipped_minutes(*now, local).any(|m| self.expr.matches(m))
    }

    /// `at` on the zone's wall clock.
    fn wall_clock(&self, at: Timestamp) -> DateTime {
        at.as_jiff().to_zoned(self.tz.clone()).datetime()
    }

    /// Whether `now` is the second occurrence of the local time `local`.
    fn is_second_pass(&self, now: Timestamp, local: DateTime) -> bool {
        let ambiguous = self.tz.to_ambiguous_timestamp(local);
        match ambiguous.offset() {
            AmbiguousOffset::Fold { .. } => {
                ambiguous.earlier().is_ok_and(|first| first < now.as_jiff())
            },
            AmbiguousOffset::Unambiguous { .. } | AmbiguousOffset::Gap { .. } => false,
        }
    }

    /// The local minutes the clocks skipped since the previous tick, a minute ago.
    fn skipped_minutes(&self, now: Timestamp, local: DateTime) -> impl Iterator<Item = DateTime> {
        let minute = SignedDuration::from_mins(1);
        let previous = now.checked_sub(minute).map(|p| self.wall_clock(p));
        let next = move |m: &DateTime| m.checked_add(minute).ok();
        std::iter::successors(previous.as_ref().and_then(next), next)
            .take_while(move |m| *m < local)
    }
}

/// Why a tick did not fire — or that it should.
///
/// [`CronExecutionState::decide`] returns this so scheduling loops can log the
/// reason for every skipped tick instead of silently `continue`-ing (#796: the
/// inverted window guard suppressed every firing for months with no signal).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CronDecision {
    /// The schedule matches and this window has not fired yet — execute.
    Fire,
    /// The schedule does not match this tick's minute (the normal idle tick).
    NotDue,
    /// The schedule matches but this window already fired (`last_executed >=
    /// window_start`) — the dedup guard, expected at most once per window
    /// (e.g. after a restart restores persisted state mid-window).
    AlreadyFired {
        /// Start of the window containing the tick (its minute, truncated).
        window_start:  Timestamp,
        /// When the window fired.
        last_executed: Timestamp,
    },
}

/// Execution state for a cron trigger (tracks last execution to prevent duplicates).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CronExecutionState {
    /// Last time this trigger was executed.
    pub last_executed: Option<Timestamp>,
}

impl CronExecutionState {
    /// Create a new execution state with no prior executions.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Restore state from a persisted last-firing instant (the
    /// `_fraiseql_cron_state` cross-restart guard): a restart inside an
    /// already-fired window must not re-fire it.
    #[must_use]
    pub const fn resuming_from(last_executed: Timestamp) -> Self {
        Self {
            last_executed: Some(last_executed),
        }
    }

    /// Check if the trigger should execute at the given time.
    ///
    /// Returns true iff [`decide`](Self::decide) says [`CronDecision::Fire`]:
    /// the schedule matches `exec_time` and no execution has occurred in the
    /// window containing it.
    #[must_use]
    pub fn should_execute(&self, schedule: &CronSchedule, exec_time: &Timestamp) -> bool {
        self.decide(schedule, exec_time) == CronDecision::Fire
    }

    /// Decide whether the trigger fires at `exec_time`, with the reason when
    /// it does not (so schedulers can log every skipped tick — see
    /// [`CronDecision`]).
    ///
    /// The invariant: a window fires exactly once, and `last_executed` lies
    /// inside a fired window — so the guard is `last_executed < window_start`,
    /// never the reverse (#796 shipped the negation and every schedule fired
    /// once, then never again).
    #[must_use]
    pub fn decide(&self, schedule: &CronSchedule, exec_time: &Timestamp) -> CronDecision {
        // Schedule must match the execution time
        if !schedule.matches(utc_wall_clock(*exec_time)) {
            return CronDecision::NotDue;
        }
        self.decide_window(exec_time)
    }

    /// Decide a tick the schedule is due at: it fires unless its window already did.
    fn decide_window(&self, exec_time: &Timestamp) -> CronDecision {
        // If no prior execution, always execute
        let Some(last_executed) = self.last_executed else {
            return CronDecision::Fire;
        };

        // The start of the window CONTAINING exec_time (not the previous one).
        let window_start = Self::find_schedule_window(exec_time);

        if last_executed < window_start {
            CronDecision::Fire
        } else {
            CronDecision::AlreadyFired {
                window_start,
                last_executed,
            }
        }
    }

    /// Find the start of the schedule window **containing** the given time.
    ///
    /// Cron's resolution is one minute, so for a matching instant the window
    /// start is that instant truncated to its minute: for `"0 2 * * *"` the
    /// window for 2024-03-15 02:00:07.312 starts at 2024-03-15 02:00:00.
    ///
    /// Truncation never moves the instant across a minute boundary, so the window
    /// start is due exactly when the tick is.
    fn find_schedule_window(time: &Timestamp) -> Timestamp {
        // On the UTC wall clock, so an instant before 1970 starts its own minute too.
        // Truncating seconds and their fraction to zero cannot fail.
        utc_wall_clock(*time)
            .with()
            .second(0)
            .subsec_nanosecond(0)
            .build()
            .and_then(|start| start.to_zoned(TimeZone::UTC))
            .map_or(*time, |start| Timestamp::from(start.timestamp()))
    }

    /// Record an execution at the given time.
    #[allow(clippy::missing_const_for_fn)] // Reason: takes &mut self, cannot be const
    pub fn record_execution(&mut self, time: Timestamp) {
        self.last_executed = Some(time);
    }

    /// Find all missed execution times between two timestamps.
    ///
    /// Finds all times when the schedule would have executed after `since` and before `until`.
    #[must_use]
    pub fn find_missed_executions(
        &self,
        schedule: &CronSchedule,
        since: &Timestamp,
        until: &Timestamp,
    ) -> Vec<Timestamp> {
        let mut missed = Vec::new();
        // Start scanning from the minute AFTER since
        let minute = SignedDuration::from_mins(1);
        let mut current = *since + minute;

        // Scan minute by minute (more fine-grained to catch all matches)
        while current < *until {
            if schedule.matches(utc_wall_clock(current)) {
                missed.push(current);
            }

            // Always advance to next minute
            current += minute;
        }

        missed
    }
}

/// `at`'s whole second in UTC, `2024-03-15T02:00:59Z`: a fraction is dropped, not rounded.
fn whole_second_utc(at: Timestamp) -> String {
    let t = utc_wall_clock(at);
    let year = t.year();
    let year = if year < 0 {
        format!("-{:04}", -year)
    } else {
        format!("{year:04}")
    };
    format!(
        "{year}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        t.month(),
        t.day(),
        t.hour(),
        t.minute(),
        t.second()
    )
}

/// A trigger that fires on a cron schedule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronTrigger {
    /// Name of the function to invoke.
    pub function_name: String,
    /// Cron expression (e.g., `"0 2 * * *"`).
    pub schedule:      String,
    /// Timezone for schedule evaluation (e.g., `"UTC"`, `"America/New_York"`).
    pub timezone:      String,
}

impl CronTrigger {
    /// Build an `EventPayload` from a cron execution.
    #[must_use]
    pub fn build_payload(&self, exec_time: &Timestamp) -> EventPayload {
        let trigger_type = format!("cron:{}", self.function_name);

        let data = serde_json::json!({
            "schedule": self.schedule,
            "timezone": self.timezone,
            "executed_at": whole_second_utc(*exec_time),
        });

        EventPayload {
            trigger_type,
            entity: "cron".to_string(),
            event_kind: "scheduled".to_string(),
            data,
            timestamp: fraiseql_time::Timestamp::now(),
        }
    }
}

#[cfg(test)]
#[path = "cron_time_tests.rs"]
mod time_tests;

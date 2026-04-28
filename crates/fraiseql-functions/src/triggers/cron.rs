//! Cron-scheduled triggers.
//!
//! Handles `cron:<function-name>` triggers that fire on a schedule using cron expressions.

use crate::types::EventPayload;
use chrono::{Datelike, Timelike};
use serde::{Deserialize, Serialize};

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
            }
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
            let end = end_str
                .parse::<u32>()
                .map_err(|_| format!("Invalid range end: {}", end_str))?;

            return Ok(CronField::Range { start, end });
        }

        if field.contains(',') {
            let values = field
                .split(',')
                .map(|v| {
                    v.parse::<u32>()
                        .map_err(|_| format!("Invalid list value: {}", v))
                })
                .collect::<Result<Vec<u32>, String>>()?;

            return Ok(CronField::List(values));
        }

        let value = field
            .parse::<u32>()
            .map_err(|_| format!("Invalid cron field value: {}", field))?;

        Ok(CronField::Value(value))
    }
}

/// A cron expression with validation.
#[derive(Debug, Clone)]
pub struct CronSchedule {
    /// The raw cron expression (e.g., "0 2 * * *").
    pub expression: String,
    /// Parsed minute field (0-59)
    minute: CronField,
    /// Parsed hour field (0-23)
    hour: CronField,
    /// Parsed day-of-month field (1-31)
    day: CronField,
    /// Parsed month field (1-12)
    month: CronField,
    /// Parsed day-of-week field (0-6, 0=Sunday)
    weekday: CronField,
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
            return Err(format!(
                "Cron expression must have 5 fields, got {}",
                parts.len()
            ));
        }

        let minute = CronField::parse(parts[0])?;
        let hour = CronField::parse(parts[1])?;
        let day = CronField::parse(parts[2])?;
        let month = CronField::parse(parts[3])?;
        let weekday = CronField::parse(parts[4])?;

        Ok(CronSchedule {
            expression: expression.to_string(),
            minute,
            hour,
            day,
            month,
            weekday,
        })
    }

    /// Check if a given datetime matches this cron schedule.
    pub fn matches(&self, datetime: &chrono::DateTime<chrono::Utc>) -> bool {
        let minute = datetime.minute();
        let hour = datetime.hour();
        let day = datetime.day();
        let month = datetime.month();
        let weekday = datetime.weekday().number_from_sunday();

        self.minute.matches(minute)
            && self.hour.matches(hour)
            && self.day.matches(day)
            && self.month.matches(month)
            && self.weekday.matches(weekday)
    }
}

/// Execution state for a cron trigger (tracks last execution to prevent duplicates).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CronExecutionState {
    /// Last time this trigger was executed.
    pub last_executed: Option<chrono::DateTime<chrono::Utc>>,
}

impl CronExecutionState {
    /// Create a new execution state with no prior executions.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if the trigger should execute at the given time.
    ///
    /// Returns true if:
    /// - Schedule matches the time
    /// - No execution has occurred in this schedule window
    pub fn should_execute(
        &self,
        schedule: &CronSchedule,
        exec_time: &chrono::DateTime<chrono::Utc>,
    ) -> bool {
        // Schedule must match the execution time
        if !schedule.matches(exec_time) {
            return false;
        }

        // If no prior execution, always execute
        let Some(last_exec) = self.last_executed else {
            return true;
        };

        // Find the start of the matching schedule window at exec_time
        let window_start = Self::find_schedule_window(schedule, exec_time);

        // Don't execute if we already executed in this window
        last_exec >= window_start
    }

    /// Find the start of the current schedule window for a given time.
    ///
    /// For example, if schedule is `"0 2 * * *"` (2 AM daily), the window for
    /// 2024-03-15 02:30:00 starts at 2024-03-15 02:00:00.
    fn find_schedule_window(
        schedule: &CronSchedule,
        time: &chrono::DateTime<chrono::Utc>,
    ) -> chrono::DateTime<chrono::Utc> {
        // Find the most recent time that matches the schedule at or before the given time
        let mut current = *time;

        // Go back minute by minute to find the matching window
        for _ in 0..60 {
            current -= chrono::Duration::minutes(1);
            if schedule.matches(&current) {
                // Found the start of the window
                return current;
            }
        }

        // Fallback: return time as-is if we don't find it (shouldn't happen)
        *time
    }

    /// Record an execution at the given time.
    #[allow(clippy::missing_const_for_fn)] // Reason: takes &mut self, cannot be const
    pub fn record_execution(&mut self, time: chrono::DateTime<chrono::Utc>) {
        self.last_executed = Some(time);
    }

    /// Find all missed execution times between two timestamps.
    ///
    /// Finds all times when the schedule would have executed after `since` and before `until`.
    pub fn find_missed_executions(
        &self,
        schedule: &CronSchedule,
        since: &chrono::DateTime<chrono::Utc>,
        until: &chrono::DateTime<chrono::Utc>,
    ) -> Vec<chrono::DateTime<chrono::Utc>> {
        let mut missed = Vec::new();
        // Start scanning from the minute AFTER since
        let mut current = *since + chrono::Duration::minutes(1);

        // Scan minute by minute (more fine-grained to catch all matches)
        while current < *until {
            if schedule.matches(&current) {
                missed.push(current);
            }

            // Always advance to next minute
            current += chrono::Duration::minutes(1);
        }

        missed
    }
}


/// A trigger that fires on a cron schedule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronTrigger {
    /// Name of the function to invoke.
    pub function_name: String,
    /// Cron expression (e.g., `"0 2 * * *"`).
    pub schedule: String,
    /// Timezone for schedule evaluation (e.g., `"UTC"`, `"America/New_York"`).
    pub timezone: String,
}

impl CronTrigger {
    /// Build an `EventPayload` from a cron execution.
    pub fn build_payload(
        &self,
        exec_time: &chrono::DateTime<chrono::Utc>,
    ) -> EventPayload {
        let trigger_type = format!("cron:{}", self.function_name);

        let data = serde_json::json!({
            "schedule": self.schedule,
            "timezone": self.timezone,
            "executed_at": exec_time.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        });

        EventPayload {
            trigger_type,
            entity: "cron".to_string(),
            event_kind: "scheduled".to_string(),
            data,
            timestamp: chrono::Utc::now(),
        }
    }
}

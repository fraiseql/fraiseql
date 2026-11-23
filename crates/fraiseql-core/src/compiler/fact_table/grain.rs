//! A fact table's declared grain: the period one of its rows stands for.
//!
//! A table rolled up to one row per month of `day` cannot answer for part of a month. Its
//! row dated 1 January holds all of January, so a range from 10 January either drops it or
//! counts it whole, and a week of it does not exist. A table declaring its grain refuses
//! such a request, naming the bounds and buckets it can answer. A table without one is
//! read as it always was: a row stands for its own `over` value.

use chrono::{Datelike, Months, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};

use crate::{
    backend::where_clause::WhereOperator,
    compiler::aggregate_types::TemporalBucket,
    error::{FraiseQLError, Result},
};

/// The period each row of a fact table stands for, on one of its `date` columns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactGrain {
    /// The `date` denormalized filter each row is dated by: its period's first day.
    pub column:     String,
    /// The period one row stands for.
    pub bucket:     GrainBucket,
    /// A finer fact table over the same rows, which answers the edges of a range this
    /// table's rows would split: its aggregate reads the whole periods from this table and
    /// the days around them from that one. `None`: such a range is refused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edges_from: Option<String>,
}

/// A period a fact table's row can stand for: the calendar buckets of a `date` column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrainBucket {
    /// One row per day: every bound and bucket is answerable.
    Day,
    /// One row per ISO week, dated on its Monday.
    Week,
    /// One row per half-month, dated on the 1st or the 16th.
    HalfMonth,
    /// One row per month, dated on the 1st.
    Month,
    /// One row per quarter, dated on 1 January, April, July or October.
    Quarter,
    /// One row per semester, dated on 1 January or 1 July.
    Semester,
    /// One row per year, dated on 1 January.
    Year,
}

impl GrainBucket {
    /// The name the bucket has in a `groupBy` key and in `grain.bucket`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Week => "week",
            Self::HalfMonth => "half_month",
            Self::Month => "month",
            Self::Quarter => "quarter",
            Self::Semester => "semester",
            Self::Year => "year",
        }
    }

    /// The grouping bucket that is this period.
    #[must_use]
    pub const fn temporal(self) -> TemporalBucket {
        match self {
            Self::Day => TemporalBucket::Day,
            Self::Week => TemporalBucket::Week,
            Self::HalfMonth => TemporalBucket::HalfMonth,
            Self::Month => TemporalBucket::Month,
            Self::Quarter => TemporalBucket::Quarter,
            Self::Semester => TemporalBucket::Semester,
            Self::Year => TemporalBucket::Year,
        }
    }

    /// The buckets a row of this period falls wholly inside, finest first: a group of
    /// whole rows. A week straddles months and years, so it nests only in itself.
    #[must_use]
    pub const fn served(self) -> &'static [GrainBucket] {
        match self {
            Self::Day => &[
                Self::Day,
                Self::Week,
                Self::HalfMonth,
                Self::Month,
                Self::Quarter,
                Self::Semester,
                Self::Year,
            ],
            Self::Week => &[Self::Week],
            Self::HalfMonth => &[
                Self::HalfMonth,
                Self::Month,
                Self::Quarter,
                Self::Semester,
                Self::Year,
            ],
            Self::Month => &[Self::Month, Self::Quarter, Self::Semester, Self::Year],
            Self::Quarter => &[Self::Quarter, Self::Semester, Self::Year],
            Self::Semester => &[Self::Semester, Self::Year],
            Self::Year => &[Self::Year],
        }
    }

    /// Whether `bucket` groups whole rows of this period.
    #[must_use]
    pub fn serves(self, bucket: TemporalBucket) -> bool {
        self.served().iter().any(|b| b.temporal() == bucket)
    }

    /// The first day of the period holding `day`, as PostgreSQL's bucket start is.
    #[must_use]
    pub fn start(self, day: NaiveDate) -> NaiveDate {
        let first_of = |month: u32| NaiveDate::from_ymd_opt(day.year(), month, 1).unwrap_or(day);
        match self {
            Self::Day => day,
            Self::Week => day.week(Weekday::Mon).first_day(),
            Self::HalfMonth => day.with_day(if day.day() < 16 { 1 } else { 16 }).unwrap_or(day),
            Self::Month => first_of(day.month()),
            Self::Quarter => first_of((day.month0() / 3) * 3 + 1),
            Self::Semester => first_of(if day.month() <= 6 { 1 } else { 7 }),
            Self::Year => first_of(1),
        }
    }

    /// The first day of the first period starting on or after `day`; `None` past the
    /// calendar.
    #[must_use]
    pub fn ceil(self, day: NaiveDate) -> Option<NaiveDate> {
        if self.start(day) == day {
            Some(day)
        } else {
            self.next_start(day)
        }
    }

    /// The first day of the period after the one holding `day`; `None` past the calendar.
    fn next_start(self, day: NaiveDate) -> Option<NaiveDate> {
        let start = self.start(day);
        match self {
            Self::Day => start.succ_opt(),
            Self::Week => start.checked_add_days(chrono::Days::new(7)),
            Self::HalfMonth if start.day() == 1 => start.with_day(16),
            Self::HalfMonth | Self::Month => start.with_day(1)?.checked_add_months(Months::new(1)),
            Self::Quarter => start.checked_add_months(Months::new(3)),
            Self::Semester => start.checked_add_months(Months::new(6)),
            Self::Year => start.checked_add_months(Months::new(12)),
        }
    }
}

impl FactGrain {
    /// Refuses a `where` condition on the grain column that does not fall on period
    /// boundaries: a range that would split a row.
    ///
    /// `_gte` and `_lt` take the day a period starts on; `_gt` and `_lte` the day before
    /// it, the day a period ends on. Any other operator on the column reads part of a
    /// period, except `_isnull`, which reads no date. A day grain refuses nothing.
    ///
    /// # Errors
    ///
    /// [`FraiseQLError::Validation`] naming the condition, the grain and the nearest
    /// bounds the table can answer.
    pub fn check_bound(
        &self,
        table: &str,
        key: &str,
        operator: &WhereOperator,
        value: &serde_json::Value,
    ) -> Result<()> {
        let bucket = self.bucket;
        if bucket == GrainBucket::Day {
            return Ok(());
        }
        let ends = match operator {
            WhereOperator::Gte | WhereOperator::Lt => false,
            WhereOperator::Gt | WhereOperator::Lte => true,
            WhereOperator::IsNull => return Ok(()),
            _ => {
                return Err(self.refusal(
                    table,
                    "where",
                    &format!(
                        "`{key}` reads part of a {period}. Bound `{column}` on {period} \
                         boundaries with `_gte`, `_gt`, `_lt` or `_lte` instead",
                        period = bucket.name().replace('_', "-"),
                        column = self.column,
                    ),
                ));
            },
        };
        let day = value
            .as_str()
            .and_then(|text| NaiveDate::parse_from_str(text, "%Y-%m-%d").ok())
            .ok_or_else(|| {
                self.refusal(
                    table,
                    "where",
                    &format!("`{key}` takes a date (`YYYY-MM-DD`), got {value}"),
                )
            })?;
        // A bound that ends a period is aligned when the day after it starts one.
        let first = if ends { day.succ_opt() } else { Some(day) };
        if first.is_some_and(|first| bucket.start(first) == first) {
            return Ok(());
        }
        let period = bucket.name().replace('_', "-");
        let (before, after) = first
            .map_or((None, None), |first| (Some(bucket.start(first)), bucket.next_start(first)));
        let shown = |start: Option<NaiveDate>| {
            let day = if ends {
                start.and_then(|d| d.pred_opt())
            } else {
                start
            };
            day.map(|d| format!("`{key}: {d}`"))
        };
        let nearest: Vec<String> = [shown(before), shown(after)].into_iter().flatten().collect();
        let verb = if ends { "end" } else { "start" };
        Err(self.refusal(
            table,
            "where",
            &format!(
                "`{key}: {day}` does not {verb} a {period}, and a {period}'s row cannot be \
                 split. Bound the range where a {period} {verb}s{}",
                if nearest.is_empty() {
                    String::new()
                } else {
                    format!(", such as {}", nearest.join(" or "))
                }
            ),
        ))
    }

    /// Refuses grouping the grain column by a bucket a row does not fall wholly inside.
    ///
    /// # Errors
    ///
    /// [`FraiseQLError::Validation`] naming the key and the buckets the table can group by.
    pub fn check_bucket(&self, table: &str, key: &str, bucket: TemporalBucket) -> Result<()> {
        if self.bucket.serves(bucket) {
            return Ok(());
        }
        let served: Vec<String> = self
            .bucket
            .served()
            .iter()
            .map(|b| format!("`{}_{}`", self.column, b.name()))
            .collect();
        Err(self.refusal(
            table,
            "groupBy",
            &format!(
                "`{key}` groups by a period finer than a row, or one a row straddles. Group \
                 by {}",
                served.join(", ")
            ),
        ))
    }

    fn refusal(&self, table: &str, argument: &str, why: &str) -> FraiseQLError {
        FraiseQLError::Validation {
            message: format!(
                "fact table `{table}` holds one row per {} of `{}`: {why}",
                self.bucket.name().replace('_', "-"),
                self.column
            ),
            path:    Some(argument.to_string()),
        }
    }
}

#[cfg(test)]
#[path = "grain_tests.rs"]
mod tests;

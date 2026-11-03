//! The `groupBy` keys a fact table's aggregate root takes (#1800).
//!
//! A key names one grouping: a dimension path, a denormalized filter column, a temporal
//! bucket of a time column (`<column>_<bucket>`), or the depth or a level of an ltree column
//! (`<column>_depth`, `<column>_level`). The generated keys are spelled from a column's name,
//! so a real column or dimension spelled like one would give the key two meanings, and the
//! published `groupBy` type one field for both; such a table is refused at load.

use std::fmt;

use super::{FactTableMetadata, SqlType};
use crate::compiler::aggregate_types::TemporalBucket;

/// A `groupBy` key and the grouping it names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupKey {
    /// The key, as a request spells it and a row answers under it.
    pub name:   String,
    /// What the key groups by.
    pub source: GroupKeySource,
}

/// What a [`GroupKey`] groups by.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum GroupKeySource {
    /// A path of the JSONB dimension column.
    Dimension(String),
    /// A denormalized filter column, grouped whole.
    Filter(String),
    /// A temporal bucket of a time column (a `date` or `timestamp` filter, or a calendar
    /// dimension's source column).
    Bucket {
        /// The time column.
        column: String,
        /// The bucket.
        bucket: TemporalBucket,
    },
    /// The number of labels of an ltree column's path.
    Depth(String),
    /// An ltree column's path cut to a level.
    Level(String),
}

impl GroupKeySource {
    /// Whether the key is spelled from a column's name rather than declared.
    const fn is_generated(&self) -> bool {
        matches!(self, Self::Bucket { .. } | Self::Depth(_) | Self::Level(_))
    }
}

impl fmt::Display for GroupKeySource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dimension(path) => write!(f, "the dimension `{path}`"),
            Self::Filter(column) => write!(f, "the denormalized filter `{column}`"),
            Self::Bucket { column, bucket } => {
                write!(f, "the `{}` bucket of `{column}`", bucket.field_suffix())
            },
            Self::Depth(column) => write!(f, "the depth of `{column}`"),
            Self::Level(column) => write!(f, "the level of `{column}`"),
        }
    }
}

impl FactTableMetadata {
    /// Every `groupBy` key the table takes, each once, in a deterministic order: filters,
    /// dimension paths not also a filter, then the generated keys.
    ///
    /// A dimension spelled like a filter is one key: the runtime reads it from the column. A
    /// calendar dimension's bucket of a filter column is that column's own bucket, served from
    /// the calendar. Two different groupings under one name are both kept, so
    /// [`group_key_collisions`](Self::group_key_collisions) can name them.
    #[must_use]
    pub fn group_keys(&self) -> Vec<GroupKey> {
        let mut keys: Vec<GroupKey> = self
            .denormalized_filters
            .iter()
            .map(|f| GroupKey {
                name:   f.name.clone(),
                source: GroupKeySource::Filter(f.name.clone()),
            })
            .collect();
        for path in &self.dimensions.paths {
            if !self.denormalized_filters.iter().any(|f| f.name == path.name) {
                keys.push(GroupKey {
                    name:   path.name.clone(),
                    source: GroupKeySource::Dimension(path.name.clone()),
                });
            }
        }
        let mut generated = Vec::new();
        for filter in &self.denormalized_filters {
            match filter.sql_type {
                SqlType::Date | SqlType::Timestamp => {
                    for bucket in TemporalBucket::all() {
                        generated.push(bucket_key(&filter.name, *bucket));
                    }
                },
                SqlType::Ltree => {
                    generated.push(GroupKey {
                        name:   format!("{}_depth", filter.name),
                        source: GroupKeySource::Depth(filter.name.clone()),
                    });
                    generated.push(GroupKey {
                        name:   format!("{}_level", filter.name),
                        source: GroupKeySource::Level(filter.name.clone()),
                    });
                },
                _ => {},
            }
        }
        for calendar in &self.calendar_dimensions {
            for granularity in &calendar.granularities {
                for bucket in &granularity.buckets {
                    generated.push(bucket_key(&calendar.source_column, bucket.bucket_type));
                }
            }
        }
        for key in generated {
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
        keys
    }

    /// One message per generated `groupBy` key that is also another grouping's key.
    #[must_use]
    pub fn group_key_collisions(&self) -> Vec<String> {
        let keys = self.group_keys();
        let mut violations = Vec::new();
        for (i, key) in keys.iter().enumerate() {
            if !key.source.is_generated() {
                continue;
            }
            for other in keys.iter().take(i).filter(|other| other.name == key.name) {
                violations.push(format!(
                    "fact table `{}`: groupBy key `{}` is both {} and {}: rename the column \
                     and recompile",
                    self.table_name, key.name, key.source, other.source
                ));
            }
        }
        violations
    }
}

fn bucket_key(column: &str, bucket: TemporalBucket) -> GroupKey {
    GroupKey {
        name:   format!("{column}_{}", bucket.field_suffix()),
        source: GroupKeySource::Bucket {
            column: column.to_string(),
            bucket,
        },
    }
}

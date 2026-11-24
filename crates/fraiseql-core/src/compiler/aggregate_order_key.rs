//! Which aggregates an aggregate root's `orderBy` sorts by.

use super::{aggregate_types::AggregateFunction, aggregation::AggregateSelection};
use crate::error::{FraiseQLError, Result};

impl AggregateFunction {
    /// Whether the groups sort meaningfully by this aggregate's value. A `json_agg`,
    /// `jsonb_agg` or `array_agg` is a JSON document or an array, which compares as no order a
    /// client asked for, so it is not a sort key, as a list query's JSON and list fields are
    /// not. A `string_agg` is text, and sorts as text.
    #[must_use]
    pub const fn sorts(self) -> bool {
        !matches!(self, Self::ArrayAgg | Self::JsonAgg | Self::JsonbAgg)
    }
}

/// Refuse sorting the groups by `selection`, an aggregate `orderBy` names, when its value does
/// not sort.
///
/// # Errors
///
/// `FraiseQLError::Validation` for a `json_agg`, `jsonb_agg` or `array_agg`.
pub(super) fn refuse_unsortable(selection: &AggregateSelection) -> Result<()> {
    match selection {
        AggregateSelection::MeasureAggregate {
            function, alias, ..
        } if !function.sorts() => Err(FraiseQLError::Validation {
            message: format!(
                "orderBy `{alias}` is a JSON document or an array, which does not sort: order \
                 by a group key or a scalar aggregate"
            ),
            path:    Some("orderBy".to_string()),
        }),
        _ => Ok(()),
    }
}

//! The `SortDirection` a list query's `orderBy` publishes holds the six values the engine sorts
//! by. A declared one takes the derived one's place, so it must hold the same six.

use super::CompiledSchema;
use crate::schema::derived_inputs::{SORT_DIRECTION_ENUM, order_by_type_names};

impl CompiledSchema {
    /// The refusal of a declared `SortDirection` that a list query's `orderBy` would publish
    /// with values other than the six, naming the first query that publishes it.
    pub(super) fn sort_direction_violation(&self) -> Option<String> {
        let declared = self.find_enum(SORT_DIRECTION_ENUM)?;
        if declared
            .values
            .iter()
            .map(|v| v.name.as_str())
            .eq(crate::types::SORT_DIRECTIONS)
        {
            return None;
        }
        let query = self.queries.iter().find(|query| {
            query.auto_params.has_order_by && order_by_type_names(self, query).is_some()
        })?;
        Some(format!(
            "`{SORT_DIRECTION_ENUM}` is declared, and the `orderBy` of query `{}` publishes the \
             enum of that name with the values {}: declare those, or no `{SORT_DIRECTION_ENUM}`, \
             and recompile",
            query.name,
            crate::backend::types::nulls_order::sort_directions_listed()
        ))
    }
}

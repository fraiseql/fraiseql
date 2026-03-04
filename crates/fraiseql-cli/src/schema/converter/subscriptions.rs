use anyhow::{Context, Result};
use fraiseql_core::schema::{SubscriptionDefinition, SubscriptionFilter};

use super::SchemaConverter;
use crate::schema::intermediate::IntermediateSubscription;

impl SchemaConverter {
    pub(super) fn convert_subscription(
        intermediate: IntermediateSubscription,
    ) -> Result<SubscriptionDefinition> {
        let arguments = intermediate
            .arguments
            .into_iter()
            .map(Self::convert_argument)
            .collect::<Result<Vec<_>>>()
            .context(format!("Failed to convert subscription '{}'", intermediate.name))?;

        // Convert filter conditions to SubscriptionFilter
        let filter = intermediate.filter.map(|f| {
            let argument_paths = f.conditions.into_iter().map(|c| (c.argument, c.path)).collect();
            SubscriptionFilter {
                argument_paths,
                static_filters: Vec::new(),
            }
        });

        // Convert deprecation
        let deprecation = intermediate
            .deprecated
            .map(|d| fraiseql_core::schema::DeprecationInfo { reason: d.reason });

        Ok(SubscriptionDefinition {
            name: intermediate.name,
            return_type: intermediate.return_type,
            arguments,
            description: intermediate.description,
            topic: intermediate.topic,
            filter,
            fields: intermediate.fields,
            filter_fields: Vec::new(),
            deprecation,
        })
    }
}

use anyhow::{Context, Result};
use fraiseql_core::schema::{SubscriptionDefinition, SubscriptionFilter};

use super::{DeclaredTypeNames, SchemaConverter};
use crate::schema::intermediate::IntermediateSubscription;

impl SchemaConverter {
    pub(super) fn convert_subscription(
        intermediate: IntermediateSubscription,
        declared: &DeclaredTypeNames,
    ) -> Result<SubscriptionDefinition> {
        let name = intermediate.name;
        let arguments = intermediate
            .arguments
            .into_iter()
            .map(|a| Self::convert_argument(a, declared))
            .collect::<Result<Vec<_>>>()
            .context(format!("Failed to convert subscription '{name}'"))?;

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

        let subscription = SubscriptionDefinition {
            name,
            return_type: intermediate.return_type,
            arguments,
            description: intermediate.description,
            topic: intermediate.topic,
            filter,
            fields: intermediate.fields,
            filter_fields: Vec::new(),
            deprecation,
        };

        // Resolve every filter reference against the subscription's own declared
        // arguments, where the two facts sit side by side (#1262). Accepted, the
        // reference is not merely inert: the runtime skips a condition whose variable is
        // absent — that is how an unsupplied optional argument behaves — so the filter
        // fails **open** and the subscription delivers every event on its topic. Same
        // shape as `vector_distance`, where a dangling field reference is already a
        // compile error.
        if let Some(violation) = subscription.filter_violation() {
            anyhow::bail!("{violation}");
        }

        Ok(subscription)
    }
}

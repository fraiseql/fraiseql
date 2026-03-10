//! @requires field pre-fetching and entity augmentation.
//!
//! # @requires Stub — Subgraph Mode
//!
//! `pre_fetch_requires_fields` is intentionally a no-op stub.
//!
//! In the Apollo Federation specification, `@requires` instructs the
//! gateway/router to resolve listed fields from their owning subgraphs
//! *before* forwarding the entity to this subgraph for mutation.  When
//! FraiseQL acts as the **gateway/orchestrator** it delegates that
//! resolution to the Apollo Router or a custom query planner; the router
//! populates the entity representation with the required fields before
//! calling the target subgraph.
//!
//! When FraiseQL acts as a **subgraph** the entity representation
//! arriving in the `_entities` resolver already contains every
//! `@requires` field — they were fetched by the gateway.  There is
//! therefore no cross-subgraph HTTP call to make inside `SagaExecutor`.
//!
//! If a future mode requires FraiseQL to act as a standalone gateway
//! that itself resolves `@requires` fields, a proper `EntityFetcher`
//! component with an HTTP client must be wired into `SagaExecutor`.
//! Until then, this stub returns an empty JSON object so that
//! `augment_entity_with_requires` is a no-op.

use super::*;

impl SagaExecutor {
    /// Pre-fetch any `@requires` fields needed by a saga step.
    ///
    /// # Current Behaviour (Stub)
    ///
    /// Returns an empty JSON object `{}`.  In subgraph mode the gateway
    /// has already resolved `@requires` fields before the entity reaches
    /// this executor, so no additional fetching is required.
    ///
    /// # Future Extension
    ///
    /// Replace with an `EntityFetcher` that issues queries to owning
    /// subgraphs when FraiseQL operates as a standalone saga gateway.
    ///
    /// # Errors
    ///
    /// Currently infallible.  A real implementation may return errors if
    /// the owning subgraph is unavailable.
    pub(super) async fn pre_fetch_requires_fields(
        &self,
        saga_id: Uuid,
        step_number: u32,
    ) -> SagaStoreResult<serde_json::Value> {
        info!(
            saga_id = %saga_id,
            step_number = step_number,
            "@requires pre-fetch invoked (stub — no cross-subgraph fetch in subgraph mode)"
        );

        // No-op: gateway has already resolved @requires fields into the
        // entity representation before it arrives here.
        Ok(serde_json::json!({}))
    }

    /// Merge `@requires` fields into the entity data.
    ///
    /// Performs a shallow merge of `requires_fields` into `entity_data`.
    /// Keys in `requires_fields` overwrite matching keys in `entity_data`,
    /// which lets gateway-supplied fields take precedence over any
    /// client-provided values.
    ///
    /// If `entity_data` is not a JSON object the value is returned
    /// unchanged (no augmentation possible).
    pub(super) fn augment_entity_with_requires(
        &self,
        entity_data: serde_json::Value,
        requires_fields: serde_json::Value,
    ) -> serde_json::Value {
        match (entity_data, requires_fields) {
            (serde_json::Value::Object(mut entity), serde_json::Value::Object(requires)) => {
                for (key, value) in requires {
                    entity.insert(key, value);
                }
                serde_json::Value::Object(entity)
            },
            (entity, _) => entity,
        }
    }
}

//! The `orderBy` a gRPC read hands the engine is one the engine's parser takes: a
//! `{field, direction}` key, sorting by that field in that direction.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use fraiseql_core::db::types::{OrderByClause, OrderDirection};

use super::handler::order_by_argument;

#[test]
fn the_grpc_order_by_is_a_key_the_engine_parses() {
    for (direction, parsed) in [("ASC", OrderDirection::Asc), ("DESC", OrderDirection::Desc)] {
        let clauses = OrderByClause::from_graphql_json(&order_by_argument("name", direction))
            .unwrap_or_else(|e| panic!("{direction}: {e}"));
        assert_eq!(clauses.len(), 1, "{direction}");
        assert_eq!((clauses[0].field.as_str(), clauses[0].direction), ("name", parsed));
    }
}

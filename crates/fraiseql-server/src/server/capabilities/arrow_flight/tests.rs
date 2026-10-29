use std::sync::Arc;

use fraiseql_core::{runtime::Executor, schema::CompiledSchema};
use fraiseql_test_utils::failing_adapter::FailingAdapter;

use super::wire_policy;
use crate::routes::graphql::AppState;

/// A served Flight service builds its principals with the schema's tenant claim, as
/// every other transport does.
#[test]
fn a_served_flight_service_builds_principals_with_the_schemas_tenant_claim() {
    let schema = CompiledSchema::default();
    let tenant_claim = schema.tenant_claim().to_string();
    let app = AppState::new(Arc::new(Executor::new(schema, Arc::new(FailingAdapter::new()))));
    let mut service = fraiseql_arrow::FraiseQLFlightService::new();
    wire_policy(&mut service, app);
    assert_eq!(service.tenant_claim(), Some(tenant_claim.as_str()));
}

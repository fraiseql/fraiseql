//! Common test utilities and helpers for integration testing

pub mod database_fixture;
pub mod graphql_executor;
#[allow(dead_code)] // Reason: each integration test binary uses only a subset of helpers
pub mod server_harness;
// Each integration test binary compiles this module independently and uses
// only a subset of helpers, so unused-function warnings are expected.
#[allow(dead_code)] // Reason: each integration test binary uses only a subset of helpers
pub mod test_app;

#[allow(unused_imports)]
// Reason: re-exported for all integration test binaries; each uses a subset
pub use database_fixture::{
    DatabaseFixture, GraphQLResult, PostFixture, TestDataBuilder, UserFixture,
};
#[allow(unused_imports)]
// Reason: re-exported for all integration test binaries; each uses a subset
pub use fraiseql_test_utils::{SagaStepDef, SagaStepResult, StepStatusEnum, TestSagaExecutor};
#[allow(unused_imports)]
// Reason: re-exported for all integration test binaries; each uses a subset
pub use graphql_executor::FakeGraphQLExecutor;

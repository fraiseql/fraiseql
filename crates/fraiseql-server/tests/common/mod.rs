//! Common test utilities and helpers for integration testing

pub mod database_fixture;
pub mod graphql_executor;
// Each integration test binary compiles this module independently and uses
// only a subset of helpers, so unused-function warnings are expected.
#[allow(dead_code)]
pub mod test_app;

#[allow(unused_imports)]
pub use database_fixture::{
    DatabaseFixture, GraphQLResult, PostFixture, TestDataBuilder, UserFixture,
};
#[allow(unused_imports)]
pub use graphql_executor::TestGraphQLExecutor;
#[allow(unused_imports)]
pub use fraiseql_test_utils::{SagaStepDef, SagaStepResult, StepStatusEnum, TestSagaExecutor};

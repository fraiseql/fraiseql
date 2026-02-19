//! Federation test suite — split by concern from oversized monolithic files.
//!
//! Original files (total ~10,842 lines, 255 tests):
//! - federation_docker_compose_integration.rs (3,030 lines)
//! - federation_mutations_integration.rs (1,835 lines)
//! - federation_database_integration.rs (1,711 lines)
//! - federation_saga_e2e_scenarios.rs (1,581 lines)
//! - federation_saga_e2e.rs (1,464 lines)
//! - federation_requires_runtime.rs (1,221 lines)

mod federation {
    pub mod common;

    // Split from federation_requires_runtime.rs (27 tests)
    mod requires_enforcement;
    mod requires_data_types;
    mod requires_edge_cases;

    // Split from federation_mutations_integration.rs (48 tests)
    mod mutation_local;
    mod mutation_extended;
    mod mutation_cross_graph;
    mod mutation_response;
    mod mutation_error;
    mod mutation_detection;

    // Split from federation_database_integration.rs (35 tests)
    mod entity_resolution;
    mod entity_where_clause;
    mod entity_cross_db;

    // Split from federation_saga_e2e_scenarios.rs (58 tests)
    mod saga_basic_execution;
    mod saga_failure;
    mod saga_compensation;
    mod saga_recovery;

    // Split from federation_saga_e2e.rs (25 tests)
    mod saga_e2e_harness;
    mod saga_e2e_lifecycle;
    mod saga_e2e_failure;
    mod saga_e2e_state;
    mod saga_e2e_recovery;

    // Split from federation_docker_compose_integration.rs (62 tests)
    mod docker_health;
    mod docker_queries;
    mod docker_federation;
    mod docker_mutations;
    mod docker_performance;
}

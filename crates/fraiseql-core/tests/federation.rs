//! Federation test suite — split by concern into focused modules.

mod federation {
    pub mod common;

    // @requires directive enforcement (27 tests)
    mod requires_enforcement;
    mod requires_data_types;
    mod requires_edge_cases;

    // Mutation operations (48 tests)
    mod mutation_local;
    mod mutation_extended;
    mod mutation_cross_graph;
    mod mutation_response;
    mod mutation_error;
    mod mutation_detection;

    // Entity resolution — database integration
    mod entity_resolution;
    mod entity_connection;
    mod entity_projection;
    mod entity_perf;
    mod entity_where_clause;
    mod entity_cross_db;

    // Saga execution and basic scenarios
    mod saga_basic_execution;
    mod saga_failure;
    mod saga_compensation;

    // Saga recovery and complex failures
    mod saga_recovery_manager;
    mod saga_crash_recovery;
    mod saga_complex_failures;

    // Saga E2E scenarios
    mod saga_e2e_harness;
    mod saga_e2e_lifecycle;
    mod saga_e2e_failure;
    mod saga_e2e_state;
    mod saga_e2e_recovery;

    // Docker Compose integration — health and queries
    mod docker_health;
    mod docker_queries;

    // Docker Compose integration — federation
    mod docker_two_subgraph;
    mod docker_entity_resolution;
    mod docker_composite_keys;
    mod docker_three_subgraph;
    mod docker_apollo_router;

    // Docker Compose integration — mutations
    mod docker_mutations;

    // Docker Compose integration — performance
    mod docker_perf_subgraph;
    mod docker_perf_gateway;
}

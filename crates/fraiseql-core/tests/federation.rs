//! Federation test suite — split by concern into focused modules.

mod federation {
    pub mod common;

    // @requires directive enforcement (27 tests)
    mod requires_data_types;
    mod requires_edge_cases;
    mod requires_enforcement;

    // Mutation operations (48 tests)
    mod mutation_cross_graph;
    mod mutation_detection;
    mod mutation_error;
    mod mutation_extended;
    mod mutation_local;
    mod mutation_response;

    // Entity resolution — database integration
    mod entity_connection;
    mod entity_cross_db;
    mod entity_perf;
    mod entity_projection;
    mod entity_resolution;
    mod entity_where_clause;

    // Saga execution and basic scenarios
    mod saga_basic_execution;
    mod saga_compensation;
    mod saga_failure;

    // Saga recovery and complex failures
    mod saga_complex_failures;
    mod saga_crash_recovery;
    mod saga_recovery_manager;

    // Saga E2E scenarios
    mod saga_e2e_failure;
    mod saga_e2e_harness;
    mod saga_e2e_lifecycle;
    mod saga_e2e_recovery;
    mod saga_e2e_state;

    // Docker Compose integration — health and queries
    mod docker_health;
    mod docker_queries;

    // Docker Compose integration — federation
    mod docker_apollo_router;
    mod docker_composite_keys;
    mod docker_entity_resolution;
    mod docker_three_subgraph;
    mod docker_two_subgraph;

    // Docker Compose integration — mutations
    mod docker_mutations;

    // Docker Compose integration — performance
    mod docker_perf_gateway;
    mod docker_perf_subgraph;
}

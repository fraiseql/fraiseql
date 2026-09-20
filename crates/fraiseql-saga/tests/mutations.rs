//! Saga mutation tests: local dispatch, error shapes, and response projection.

mod mutations {
    pub mod common;

    mod mutation_detection;
    mod mutation_error;
    mod mutation_local;
    mod mutation_response;
}

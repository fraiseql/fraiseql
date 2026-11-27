#![no_main]

//! Fuzz the WHERE SQL the executor runs, through `fraiseql_core::db`.
//!
//! Over arbitrary trees, the parameterised generator must never panic, must bind a
//! rendered tree's parameters as exactly `$1`..`$n`, and must render the same statement
//! for the same tree with other operands of the same JSON shapes, whenever it renders.
//! See `fraiseql-db/fuzz/fuzz_targets/where_generator.rs` for why a refusal is not a
//! difference.

use fraiseql_core::db::{
    PostgresDialect, WhereClause,
    fuzzing::{Operands, placeholders_are_one_to, revalue},
    where_generator::GenericWhereGenerator,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: (WhereClause, Operands, Operands)| {
    let (tree, a, b) = input;
    let generator = GenericWhereGenerator::new(PostgresDialect);
    let Ok((sql, params)) = generator.generate(&tree) else {
        return;
    };
    assert!(
        placeholders_are_one_to(&sql, params.len()),
        "placeholders are not $1..${}: {sql}",
        params.len()
    );
    for operands in [&a, &b] {
        if let Ok((other, other_params)) = generator.generate(&revalue(&tree, operands)) {
            assert_eq!(sql, other, "the operands changed the statement");
            assert_eq!(params.len(), other_params.len(), "the operands changed the binding");
        }
    }
});

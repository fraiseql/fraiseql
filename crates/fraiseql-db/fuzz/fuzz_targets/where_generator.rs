#![no_main]

//! Fuzz WHERE-clause SQL generation over arbitrary trees.
//!
//! Generation must never panic. A tree it renders must bind its parameters as exactly
//! `$1`..`$n`, and its SQL must not depend on its operand values: the same tree carrying
//! two other sets of operands of the same JSON shapes renders the same statement with
//! the same number of parameters, whenever it renders. A value may be refused (an
//! unreadable operand), so a refusal is not a difference; a statement that changes with
//! a value means the value reached the SQL text. The trees come from
//! `fraiseql_db::fuzzing`, which says which nodes it builds; the parse boundary is
//! `where_from_json`'s.

use fraiseql_db::{
    GenericWhereGenerator, PostgresDialect, WhereClause,
    fuzzing::{Operands, placeholders_are_one_to, revalue},
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

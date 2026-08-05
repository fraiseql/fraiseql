#![no_main]

//! Property target for #719: an inline GraphQL argument never silently vanishes.
//!
//! An argument is parsed into a JSON *string* on `GraphQLArgument::value_json`
//! and re-read later by the matcher, the directive evaluator, the query
//! classifier and the multi-root pipeline. #719 was two independent failures of
//! that round trip, and both were silent:
//!
//! * the writer escaped only `"`, so a Windows path, a newline or a control
//!   character produced *invalid JSON* that every reader dropped via `.ok()?`;
//! * a variable was signalled in-band as `"$name"`, so the literal `"$100"` was
//!   indistinguishable from a reference to a variable called `100`.
//!
//! A dropped argument is not a cosmetic loss. A dropped `where:` does not narrow
//! a result set — it **widens** it, which is why this one gets a fuzz target
//! rather than a unit test.
//!
//! The property: for every argument in a document the parser accepted,
//! `decode(value_json)` must succeed. Anything that fails to decode is an
//! argument the runtime will silently discard.

use fraiseql_core::graphql::{parse_query, types::FieldSelection, value_json};
use libfuzzer_sys::fuzz_target;

/// Every argument must survive the write→read round trip.
fn check_selections(selections: &[FieldSelection]) {
    for field in selections {
        for arg in &field.arguments {
            let decoded = value_json::decode(&arg.value_json);
            assert!(
                decoded.is_ok(),
                "argument {:?} did not survive the value_json round trip and would be \
                 silently dropped (a dropped `where:` widens the result set): {:?} — {:?}",
                arg.name,
                arg.value_json,
                decoded.err(),
            );
        }
        check_selections(&field.nested_fields);
    }
}

fuzz_target!(|data: &str| {
    let Ok(parsed) = parse_query(data) else {
        return;
    };

    check_selections(&parsed.selections);
    for fragment in &parsed.fragments {
        check_selections(&fragment.selections);
    }

    // Variable defaults travel through the same encoder.
    for var in &parsed.variables {
        if let Some(default) = &var.default_value {
            let decoded = value_json::decode(default);
            assert!(
                decoded.is_ok(),
                "variable default for {:?} did not survive the round trip: {default:?}",
                var.name,
            );
        }
    }
});

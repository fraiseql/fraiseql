//! A document defines each fragment name once (§ 5.5.1.1 Fragment Name Uniqueness).
//!
//! Every reader of a document's fragments looks a spread up by name. Two definitions
//! under one name give each reader a choice, and readers that choose differently score
//! one selection and execute another. A document that defines a name twice is refused
//! where it is parsed, and a [`ParsedQuery`] that carries two cannot be deserialized.

#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use super::{complexity::parse_graphql_document, parser::parse_query, types::ParsedQuery};

/// The first `F` is small and the second is wide: a reader that scores the first and
/// runs the second sees a different document from the one it runs.
const DECOY_THEN_WIDE: &str = "{ posts { ...F } } \
     fragment F on Post { id } \
     fragment F on Post { t0: title t1: title t2: title }";

/// The same shape one level down: the second `F` nests a relation the first does not.
const DECOY_THEN_DEEP: &str = "{ posts { ...F } } \
     fragment F on Post { title } \
     fragment F on Post { author { name } }";

const DISTINCT: &str = "{ posts { ...F ...G } } \
     fragment F on Post { id } \
     fragment G on Post { title }";

#[test]
fn a_document_defining_a_fragment_name_twice_is_refused_at_parse() {
    for source in [DECOY_THEN_WIDE, DECOY_THEN_DEEP] {
        let error =
            parse_graphql_document(source).expect_err("a repeated fragment name is refused");
        assert!(
            error.to_string().contains("There can be only one fragment named \"F\""),
            "{error}"
        );
    }
}

#[test]
fn the_executors_parse_refuses_a_repeated_fragment_name() {
    for source in [DECOY_THEN_WIDE, DECOY_THEN_DEEP] {
        let error = parse_query(source).expect_err("a repeated fragment name is refused");
        assert!(
            error.to_string().contains("There can be only one fragment named \"F\""),
            "{error}"
        );
    }
}

#[test]
fn distinct_fragment_names_parse() {
    let parsed = parse_query(DISTINCT).expect("distinct names parse");
    let names: Vec<&str> = parsed.fragments.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["F", "G"], "fragments keep document order");
}

#[test]
fn a_parsed_query_carrying_a_fragment_name_twice_does_not_deserialize() {
    let mut value = serde_json::to_value(parse_query(DISTINCT).unwrap()).unwrap();
    let fragments = value["fragments"].as_array_mut().unwrap();
    fragments[1]["name"] = serde_json::json!("F");
    let error = serde_json::from_value::<ParsedQuery>(value)
        .expect_err("a ParsedQuery with a repeated fragment name is refused");
    assert!(
        error.to_string().contains("There can be only one fragment named \"F\""),
        "{error}"
    );
}

#[test]
fn a_parsed_query_with_distinct_fragment_names_round_trips() {
    let parsed = parse_query(DISTINCT).unwrap();
    let back: ParsedQuery =
        serde_json::from_value(serde_json::to_value(&parsed).unwrap()).expect("round-trips");
    assert_eq!(back.fragments.len(), 2, "both fragments survive");
}

/// A document of many distinct fragments parses: the uniqueness check is a lookup per
/// fragment, so the document's size bounds its cost.
#[test]
fn a_document_of_twenty_thousand_distinct_fragments_parses() {
    use std::fmt::Write as _;

    let mut source = String::from("{ posts { id } }");
    for i in 0..20_000 {
        write!(source, " fragment F{i} on Post {{ id }}").unwrap();
    }
    let parsed = parse_graphql_document(&source).expect("distinct names parse");
    assert_eq!(parsed.fragments.len(), 20_000);
}

/// The uniqueness check's cost does not grow with the number of fragments held. The
/// same 32,000 fragments go into one `Fragments` and, split, into sixteen of 2,000:
/// a lookup per fragment costs the same either way, while a check that scanned the
/// held fragments would do sixteen times the comparisons in the single one. Both sides
/// push the same definitions, so a slow or loaded machine scales them alike; each side
/// is the fastest of three runs.
#[test]
fn fragment_uniqueness_cost_does_not_grow_with_the_fragments_held() {
    use std::time::{Duration, Instant};

    use super::types::{FragmentDefinition, Fragments};

    const TOTAL: usize = 32_000;
    const SPLIT: usize = 16;

    fn fastest_of_three(definitions: &[FragmentDefinition], per_set: usize) -> Duration {
        (0..3)
            .map(|_| {
                let batch = definitions.to_vec();
                let started = Instant::now();
                let mut sets = Vec::new();
                let mut fragments = Fragments::default();
                for (i, definition) in batch.into_iter().enumerate() {
                    if i > 0 && i % per_set == 0 {
                        sets.push(std::mem::take(&mut fragments));
                    }
                    fragments.try_push(definition).unwrap();
                }
                sets.push(fragments);
                let elapsed = started.elapsed();
                std::hint::black_box(&sets);
                elapsed
            })
            .min()
            .unwrap()
    }

    let definitions: Vec<FragmentDefinition> = (0..TOTAL)
        .map(|i| FragmentDefinition {
            name:             format!("F{i}"),
            type_condition:   "Post".to_string(),
            selections:       Vec::new(),
            fragment_spreads: Vec::new(),
        })
        .collect();
    let split = fastest_of_three(&definitions, TOTAL / SPLIT).max(Duration::from_micros(1));
    let single = fastest_of_three(&definitions, TOTAL);
    let ratio = single.as_secs_f64() / split.as_secs_f64();
    assert!(
        ratio < 6.0,
        "one set of {TOTAL} cost {ratio:.1}x sixteen sets of {} ({split:?} → {single:?})",
        TOTAL / SPLIT
    );
}

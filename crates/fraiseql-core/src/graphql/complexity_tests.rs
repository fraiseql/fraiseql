//! Unit tests for the direct-read cost estimator and the projection it scores.
//!
//! The subject is [`DirectReadProjection`] and the identity it exists to preserve: a
//! `?select=` embed composed into one statement must be charged what the fan-out it
//! replaces is charged, or `[security.cost_budget] per_request_max` loosens by the
//! difference the moment the shape changes.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::collections::HashMap;

use super::*;

/// No `@cost` overrides — the case every test here but one is about.
fn no_weights() -> HashMap<String, usize> {
    HashMap::new()
}

fn score(projection: &DirectReadProjection) -> usize {
    estimate_direct_read_cost("users", &no_weights(), projection)
}

/// A level's page, as both the estimator's multiplier and the number of parent rows
/// a fan-out would issue sub-reads for. They are the same number, which is what makes
/// the identity below hold.
fn page(limit: Option<u32>) -> usize {
    limit.map_or(1, |l| {
        usize::try_from(l).unwrap_or(usize::MAX).clamp(1, PAGINATION_MULTIPLIER_CEILING)
    })
}

/// What **today's** fan-out charges for the same shape, computed independently of the
/// estimator's nested arithmetic: each level is read flat, and one sub-read is issued
/// per parent row per level — a full page of them, the worst case the ceiling exists
/// to bound.
///
/// Deliberately recursive over the same tree rather than a closed formula, so the
/// identity is asserted at every depth and not just the one a formula was derived for.
fn fan_out_total(projection: &DirectReadProjection) -> usize {
    let own = score(&DirectReadProjection::flat(projection.leaf_fields, projection.limit));
    let rows = page(projection.limit);
    projection
        .nested
        .iter()
        .fold(own, |acc, child| acc + rows * fan_out_total(child))
}

fn embed(
    leaf_fields: usize,
    limit: Option<u32>,
    nested: Vec<DirectReadProjection>,
) -> DirectReadProjection {
    DirectReadProjection {
        leaf_fields,
        limit,
        nested,
    }
}

/// Every flat shape scores exactly what the bare `selected_field_count` scored before
/// the projection type existed. The figures are the ones `5e15b0135` measured on the
/// REST budget suite's fixture: a one-field parent read over a page of 100 is 101, and
/// a two-field `orders` sub-read over the same page is 201.
#[test]
fn a_flat_projection_scores_what_the_bare_field_count_scored() {
    assert_eq!(score(&DirectReadProjection::flat(1, Some(100))), 101);
    assert_eq!(score(&DirectReadProjection::flat(2, Some(100))), 201);
    assert_eq!(score(&DirectReadProjection::flat(3, Some(100))), 301);
    // The page clamps to [1, 100] exactly as the document path's does.
    assert_eq!(score(&DirectReadProjection::flat(2, Some(10_000))), 201);
    assert_eq!(score(&DirectReadProjection::flat(2, Some(0))), 3);
    // No page: multiplier 1, the inherited permissiveness both paths share.
    assert_eq!(score(&DirectReadProjection::flat(2, None)), 3);
}

/// A read projecting nothing scores 1, with or without a page — unchanged, and the
/// case the `.count` pass would fall into if it were ever given a cost gate.
#[test]
fn a_projection_with_nothing_under_it_scores_one() {
    assert_eq!(score(&DirectReadProjection::flat(0, Some(100))), 1);
    assert_eq!(score(&DirectReadProjection::flat(0, None)), 1);
    assert_eq!(score(&embed(0, Some(100), vec![])), 1);
}

/// **The identity this piece is for.** A composed projection is charged exactly what
/// the fan-out it replaces is charged at a full page — at every depth, for every
/// shape below.
///
/// Without this the ceiling would move when the execution shape moves, which is the
/// defect class `a09bc0dae`, `5e15b0135` and this commit are all one of: an operator
/// writes one number and it has to keep meaning the same request.
#[test]
fn a_composed_embed_scores_what_its_fan_out_charges() {
    let shapes = [
        // parent 1 field / page 100, one embed of 2 fields / page 100 — the REST
        // budget suite's `users?select=id,orders(id,total)`.
        embed(1, Some(100), vec![embed(2, Some(100), vec![])]),
        // the same request on a page of 2, which is what that fixture actually holds
        embed(1, Some(2), vec![embed(2, Some(2), vec![])]),
        // a parent projecting nothing of its own
        embed(0, Some(50), vec![embed(3, Some(10), vec![])]),
        // two embeds side by side, different pages
        embed(2, Some(20), vec![embed(1, Some(5), vec![]), embed(4, Some(100), vec![])]),
        // three levels deep: `a(b(c))`
        embed(1, Some(10), vec![embed(1, Some(10), vec![embed(2, Some(10), vec![])])]),
        // an unpaginated level in the middle
        embed(2, Some(10), vec![embed(1, None, vec![embed(1, Some(3), vec![])])]),
    ];

    for shape in &shapes {
        assert_eq!(
            score(shape),
            fan_out_total(shape),
            "composed score must equal the fan-out's full-page total for {shape:?}"
        );
    }
}

/// The two-level figure, stated outright rather than only as an identity, so a change
/// that moved **both** sides of the test above still fails something.
///
/// `users?select=id,orders(id,total)` over pages of 100: the parent read alone is 101,
/// each `orders` sub-read is 201, and a full page issues 100 of them — 20 201.
#[test]
fn the_two_level_figure_is_stated_not_only_derived() {
    let composed = embed(1, Some(100), vec![embed(2, Some(100), vec![])]);
    assert_eq!(score(&composed), 20_201);
    assert_eq!(101 + 100 * 201, 20_201);
}

/// The estimator's arithmetic is the document path's, asserted against
/// [`estimate_query_cost`] itself rather than against a restatement of it — so a
/// change to `DocumentAnalyzer::field_complexity` that this type failed to follow
/// breaks here.
#[test]
fn the_composed_score_is_the_document_paths_score() {
    for (parent_fields, parent_page, child_fields, child_page) in [
        (1_usize, 100_u32, 2_usize, 100_u32),
        (3, 10, 1, 25),
        (2, 7, 4, 1),
    ] {
        let parent_sel = (0..parent_fields).map(|i| format!("f{i}")).collect::<Vec<_>>().join(" ");
        let child_sel = (0..child_fields).map(|i| format!("g{i}")).collect::<Vec<_>>().join(" ");
        let query = format!(
            "{{ users(limit: {parent_page}) {{ {parent_sel} \
             orders(limit: {child_page}) {{ {child_sel} }} }} }}"
        );

        let document = parse_graphql_document(&query).unwrap();
        let document_score = estimate_query_cost(&document, &no_weights(), None);

        let composed = embed(
            parent_fields,
            Some(parent_page),
            vec![embed(child_fields, Some(child_page), vec![])],
        );

        assert_eq!(
            score(&composed),
            document_score,
            "the same shape must cost the same whichever transport it arrived on ({query})"
        );
    }
}

/// A composed embed costs strictly more than the parent read alone. The positive
/// control: an estimator that dropped the nested levels on the floor would still pass
/// every flat test above, and this is what it fails.
#[test]
fn a_composed_embed_costs_more_than_the_parent_alone() {
    let parent_only = DirectReadProjection::flat(1, Some(100));
    let composed = embed(1, Some(100), vec![embed(2, Some(100), vec![])]);
    assert!(
        score(&composed) > score(&parent_only),
        "an embed composed into the parent statement must be charged for"
    );
}

/// An `@cost` weight on the query name contributes exactly that weight and the
/// projection is not walked — as it is for a root field in `DocumentAnalyzer::root_cost`,
/// and regardless of what is composed beneath it.
#[test]
fn a_cost_weight_short_circuits_the_projection() {
    let mut weights = HashMap::new();
    weights.insert("users".to_string(), 7_usize);
    let composed = embed(1, Some(100), vec![embed(2, Some(100), vec![])]);
    assert_eq!(estimate_direct_read_cost("users", &weights, &composed), 7);
    // and a query the map does not name is still walked
    assert_eq!(estimate_direct_read_cost("orders", &weights, &composed), 20_201);
}

/// A pagination variable the request omits scores its declared default (§ 6.4.1,
/// #1504), as the executor will run it — not the fail-closed ceiling, which refused
/// `query($n: Int = 3) { items(limit: $n) { id } }` as if it were unbounded.
#[test]
fn an_omitted_pagination_variable_scores_its_default() {
    let validator = RequestValidator::new();
    let literal = validator.analyze("{ items(limit: 3) { id name } }").unwrap();
    let defaulted = validator
        .analyze_with_variables("query Q($n: Int = 3) { items(limit: $n) { id name } }", None)
        .unwrap();
    assert_eq!(defaulted.complexity, literal.complexity);

    let supplied = validator
        .analyze_with_variables(
            "query Q($n: Int = 3) { items(limit: $n) { id name } }",
            Some(&serde_json::json!({"n": 50})),
        )
        .unwrap();
    let literal_50 = validator.analyze("{ items(limit: 50) { id name } }").unwrap();
    assert_eq!(supplied.complexity, literal_50.complexity, "a supplied value wins");
}

/// Two operations declaring the same variable with different defaults: the larger one
/// scores, so the shared name cannot lower another operation's cost.
#[test]
fn a_variable_defaulted_differently_by_two_operations_scores_the_larger() {
    let validator = RequestValidator::new();
    let both = validator
        .analyze_with_variables(
            "query A($n: Int = 50) { items(limit: $n) { id } } \
             query B($n: Int = 2) { items(limit: $n) { id } }",
            None,
        )
        .unwrap();
    let literal = validator
        .analyze("query A { items(limit: 50) { id } } query B { items(limit: 50) { id } }")
        .unwrap();
    assert_eq!(both.complexity, literal.complexity);
}

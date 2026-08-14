//! Unit tests for the `@defer` response split.
//!
//! These drive the splitter directly with hand-built selection trees, because the
//! properties that matter are structural: which key leaves `data`, what path it is
//! addressed by, and what happens inside a list. The end-to-end wiring (a real
//! document, a real database, the SSE envelope) is `graphql_sse_e2e_pg`.
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::collections::HashMap;

use serde_json::{Value, json};

use super::{contains_defer, split};
use crate::graphql::types::{Directive, FieldSelection, GraphQLArgument};

fn field(name: &str) -> FieldSelection {
    FieldSelection {
        name:          name.to_owned(),
        alias:         None,
        arguments:     vec![],
        nested_fields: vec![],
        directives:    vec![],
    }
}

fn with_children(name: &str, children: Vec<FieldSelection>) -> FieldSelection {
    FieldSelection {
        nested_fields: children,
        ..field(name)
    }
}

/// `@defer` with optional `if` / `label` arguments, encoded the way the parser does.
fn defer(args: Vec<(&str, &str, &str)>) -> Directive {
    Directive {
        name:      "defer".to_owned(),
        arguments: args
            .into_iter()
            .map(|(name, value_type, value_json)| GraphQLArgument {
                name:       name.to_owned(),
                value_type: value_type.to_owned(),
                value_json: value_json.to_owned(),
            })
            .collect(),
    }
}

fn deferred(mut selection: FieldSelection, directive: Directive) -> FieldSelection {
    selection.directives.push(directive);
    selection
}

fn no_vars() -> HashMap<String, Value> {
    HashMap::new()
}

#[test]
fn a_document_without_defer_is_untouched() {
    let selections = vec![with_children("user", vec![field("id"), field("email")])];
    let mut data = json!({"user": {"id": 1, "email": "a@b.c"}});

    assert!(!contains_defer(&selections, &no_vars()));
    let payloads = split(&selections, &mut data, &no_vars());

    assert!(payloads.is_empty());
    assert_eq!(data, json!({"user": {"id": 1, "email": "a@b.c"}}));
}

#[test]
fn a_deferred_field_leaves_data_and_is_addressed_by_its_parent_path() {
    let selections = vec![with_children(
        "user",
        vec![field("id"), deferred(field("email"), defer(vec![]))],
    )];
    let mut data = json!({"user": {"id": 1, "email": "a@b.c"}});

    assert!(contains_defer(&selections, &no_vars()));
    let payloads = split(&selections, &mut data, &no_vars());

    assert_eq!(
        data,
        json!({"user": {"id": 1}}),
        "the deferred key must leave the immediate payload"
    );
    assert_eq!(payloads.len(), 1);
    assert_eq!(payloads[0].path, vec![json!("user")]);
    assert_eq!(payloads[0].data, json!({"email": "a@b.c"}).as_object().cloned().unwrap());
}

/// Two fields contributed by one deferred fragment arrive in ONE payload, not two:
/// the spread is the unit the client deferred, and `inherit_directives` is what makes
/// its members individually recognisable.
#[test]
fn fields_sharing_a_path_and_label_are_grouped_into_one_payload() {
    let d = defer(vec![("label", "string", "\"details\"")]);
    let selections = vec![with_children(
        "user",
        vec![
            field("id"),
            deferred(field("email"), d.clone()),
            deferred(field("bio"), d),
        ],
    )];
    let mut data = json!({"user": {"id": 1, "email": "a@b.c", "bio": "hi"}});

    let payloads = split(&selections, &mut data, &no_vars());

    assert_eq!(payloads.len(), 1, "one fragment, one payload: {payloads:?}");
    assert_eq!(payloads[0].label.as_deref(), Some("details"));
    assert_eq!(
        payloads[0].data,
        json!({"email": "a@b.c", "bio": "hi"}).as_object().cloned().unwrap()
    );
}

/// Different labels under the same parent are different fragments and stay apart.
#[test]
fn different_labels_under_one_parent_stay_separate() {
    let selections = vec![with_children(
        "user",
        vec![
            deferred(field("email"), defer(vec![("label", "string", "\"contact\"")])),
            deferred(field("bio"), defer(vec![("label", "string", "\"profile\"")])),
        ],
    )];
    let mut data = json!({"user": {"email": "a@b.c", "bio": "hi"}});

    let payloads = split(&selections, &mut data, &no_vars());

    assert_eq!(payloads.len(), 2);
    assert_eq!(payloads[0].label.as_deref(), Some("contact"));
    assert_eq!(payloads[1].label.as_deref(), Some("profile"));
}

/// The reason a list needs per-element paths: without them every element's deferred
/// fields collapse onto the list's own path and all but the last are lost.
#[test]
fn each_list_element_gets_its_own_indexed_path() {
    let selections = vec![with_children(
        "users",
        vec![field("id"), deferred(field("email"), defer(vec![]))],
    )];
    let mut data = json!({"users": [
        {"id": 1, "email": "one@x"},
        {"id": 2, "email": "two@x"},
    ]});

    let payloads = split(&selections, &mut data, &no_vars());

    assert_eq!(data, json!({"users": [{"id": 1}, {"id": 2}]}));
    assert_eq!(payloads.len(), 2, "one payload per element: {payloads:?}");
    assert_eq!(payloads[0].path, vec![json!("users"), json!(0)]);
    assert_eq!(payloads[1].path, vec![json!("users"), json!(1)]);
    assert_eq!(payloads[1].data, json!({"email": "two@x"}).as_object().cloned().unwrap());
}

#[test]
fn a_deferred_root_field_is_addressed_by_the_empty_path() {
    let selections = vec![deferred(
        with_children("user", vec![field("id")]),
        defer(vec![]),
    )];
    let mut data = json!({"user": {"id": 1}});

    let payloads = split(&selections, &mut data, &no_vars());

    assert_eq!(data, json!({}));
    assert_eq!(payloads.len(), 1);
    assert!(payloads[0].path.is_empty(), "root-level defer has no path: {payloads:?}");
    assert_eq!(payloads[0].data, json!({"user": {"id": 1}}).as_object().cloned().unwrap());
}

#[test]
fn defer_if_false_is_not_a_defer() {
    let selections = vec![with_children(
        "user",
        vec![deferred(
            field("email"),
            defer(vec![("if", "boolean", "false")]),
        )],
    )];
    let mut data = json!({"user": {"email": "a@b.c"}});

    assert!(
        !contains_defer(&selections, &no_vars()),
        "an if:false @defer must not arm the split"
    );
    let payloads = split(&selections, &mut data, &no_vars());

    assert!(payloads.is_empty());
    assert_eq!(data, json!({"user": {"email": "a@b.c"}}));
}

#[test]
fn defer_if_resolves_a_variable() {
    let selections = vec![with_children(
        "user",
        vec![deferred(
            field("email"),
            defer(vec![("if", "variable", r#"{"$var":"slow"}"#)]),
        )],
    )];
    let mut vars = HashMap::new();
    vars.insert("slow".to_owned(), json!(false));

    assert!(!contains_defer(&selections, &vars));

    vars.insert("slow".to_owned(), json!(true));
    assert!(contains_defer(&selections, &vars));

    let mut data = json!({"user": {"email": "a@b.c"}});
    let payloads = split(&selections, &mut data, &vars);
    assert_eq!(payloads.len(), 1);
}

/// A field the query asked for that the row did not produce has nothing to deliver
/// later — the split must not invent a payload carrying `null`.
#[test]
fn a_deferred_field_absent_from_the_response_produces_no_payload() {
    let selections = vec![with_children(
        "user",
        vec![field("id"), deferred(field("email"), defer(vec![]))],
    )];
    let mut data = json!({"user": {"id": 1}});

    let payloads = split(&selections, &mut data, &no_vars());

    assert!(payloads.is_empty(), "no key removed, so no payload: {payloads:?}");
    assert_eq!(data, json!({"user": {"id": 1}}));
}

/// Aliases are what the response is keyed by, so they are what the split must use.
#[test]
fn the_split_keys_on_the_alias_not_the_field_name() {
    let mut aliased = field("email");
    aliased.alias = Some("contactEmail".to_owned());
    let selections = vec![with_children(
        "user",
        vec![deferred(aliased, defer(vec![]))],
    )];
    let mut data = json!({"user": {"contactEmail": "a@b.c", "email": "wrong"}});

    let payloads = split(&selections, &mut data, &no_vars());

    assert_eq!(
        data,
        json!({"user": {"email": "wrong"}}),
        "the aliased key is the one that leaves; the same-named unaliased key stays"
    );
    assert_eq!(payloads[0].data, json!({"contactEmail": "a@b.c"}).as_object().cloned().unwrap());
}

/// A deferred fragment nested inside another deferred fragment: `inherit_directives`
/// leaves both directives on the leaf, outermost first, and the outermost decides —
/// so the inner `@defer` cannot pull the field out of the payload the outer one owns.
#[test]
fn the_outermost_defer_decides_the_group() {
    let selections = vec![with_children(
        "user",
        vec![FieldSelection {
            directives: vec![
                defer(vec![("label", "string", "\"outer\"")]),
                defer(vec![("label", "string", "\"inner\"")]),
            ],
            ..field("email")
        }],
    )];
    let mut data = json!({"user": {"email": "a@b.c"}});

    let payloads = split(&selections, &mut data, &no_vars());

    assert_eq!(payloads.len(), 1);
    assert_eq!(payloads[0].label.as_deref(), Some("outer"));
}

/// A deferred field inside a non-deferred nested object: the walk must descend
/// through the parent rather than stopping at the root.
#[test]
fn the_split_descends_through_undeferred_nesting() {
    let selections = vec![with_children(
        "user",
        vec![with_children(
            "profile",
            vec![field("name"), deferred(field("bio"), defer(vec![]))],
        )],
    )];
    let mut data = json!({"user": {"profile": {"name": "n", "bio": "b"}}});

    let payloads = split(&selections, &mut data, &no_vars());

    assert_eq!(data, json!({"user": {"profile": {"name": "n"}}}));
    assert_eq!(payloads[0].path, vec![json!("user"), json!("profile")]);
}

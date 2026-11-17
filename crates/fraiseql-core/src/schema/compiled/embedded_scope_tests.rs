//! A type that carries its own scope key is not embedded in another type's served document,
//! unless the embedding field declares it scoped like its parent (#1737).
//!
//! A read of a scoped type is filtered by a key taken from the caller's token. Embedded in
//! another type's document, its rows are published to every reader of that document, and the
//! engine can only filter them on a key the embedded documents may not carry: the nested
//! list then comes back empty and nothing says why. So the schema is refused when it loads,
//! in the compiler and at server boot. The one exception is declared, field by field, and is
//! accepted only where it is true by construction: every document that reaches the field is
//! scoped by the same columns, from the same sources.
//!
//! Each schema here is loaded from JSON, as a compiled schema is: the check runs in the load.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
#![allow(clippy::needless_pass_by_value)] // Reason: helpers take the JSON a call site builds inline

use serde_json::{Value, json};

use super::CompiledSchema;

/// `tenant_id`, from the token's `org_id` claim: the scope most schemas carry.
fn tenant() -> Value {
    json!({ "tenant_id": { "source": "jwt", "claim": "org_id" } })
}

/// A field `name` of `field_type`.
fn field(name: &str, field_type: Value) -> Value {
    json!({ "name": name, "field_type": field_type })
}

/// The same field, declared scoped like its parent.
fn declared(name: &str, field_type: Value) -> Value {
    json!({ "name": name, "field_type": field_type, "scoped_like_parent": true })
}

fn object(name: &str) -> Value {
    json!({ "Object": name })
}

fn list_of(item: Value) -> Value {
    json!({ "List": { "item": item } })
}

/// A type read from the relation `v_<name>`, with an `id` and a published `tenantId`.
fn stored(name: &str, fields: Vec<Value>) -> Value {
    let mut all = vec![field("id", json!("ID")), field("tenantId", json!("ID"))];
    all.extend(fields);
    json!({ "name": name, "sql_source": format!("v_{}", name.to_lowercase()), "fields": all })
}

/// A value object: no relation of its own, it lives inside another type's document.
fn value_object(name: &str, fields: Vec<Value>) -> Value {
    json!({ "name": name, "sql_source": "", "embedded": true, "fields": fields })
}

/// A list query over `type_name`'s relation, scoped by `inject` (`null` for unscoped).
fn list_query(name: &str, type_name: &str, inject: Value) -> Value {
    let mut query = json!({
        "name": name,
        "return_type": type_name,
        "returns_list": true,
        "sql_source": format!("v_{}", type_name.to_lowercase()),
        "arguments": []
    });
    if !inject.is_null() {
        query["inject_params"] = inject;
    }
    query
}

fn schema(types: Vec<Value>, queries: Vec<Value>) -> Value {
    json!({ "types": types, "queries": queries, "mutations": [], "subscriptions": [] })
}

/// Why `schema` does not load.
fn refusal(schema: &Value) -> String {
    match CompiledSchema::from_json(&schema.to_string(), false) {
        Ok(_) => panic!("this schema must not load:\n{schema:#}"),
        Err(error) => error.to_string(),
    }
}

fn loads(schema: &Value) {
    if let Err(error) = CompiledSchema::from_json(&schema.to_string(), false) {
        panic!("this schema must load: {error}\n{schema:#}");
    }
}

/// `OrderLine`, scoped by the tenant through its own list query.
fn scoped_line() -> (Value, Value) {
    (stored("OrderLine", vec![]), list_query("orderLines", "OrderLine", tenant()))
}

// ── the refusal ──────────────────────────────────────────────────────────────

/// The issue's shape, one level: `Order` embeds the scoped `OrderLine`, undeclared.
#[test]
fn a_scoped_type_embedded_undeclared_is_refused_naming_the_field_and_the_key() {
    let (line, lines) = scoped_line();
    let order = stored("Order", vec![field("lines", list_of(object("OrderLine")))]);
    let message =
        refusal(&schema(vec![order, line], vec![list_query("orders", "Order", tenant()), lines]));

    for needle in [
        "Order.lines",
        "OrderLine",
        "orderLines",
        "tenant_id",
        "scoped_like_parent",
    ] {
        assert!(message.contains(needle), "names `{needle}`: {message}");
    }
}

/// Never inferred: a parent scoped exactly alike still has to say so.
#[test]
fn a_parent_scoped_alike_does_not_stand_for_the_declaration() {
    let (line, lines) = scoped_line();
    let order = stored("Order", vec![field("line", object("OrderLine"))]);

    let message =
        refusal(&schema(vec![order, line], vec![list_query("orders", "Order", tenant()), lines]));

    assert!(message.contains("Order.line"), "{message}");
}

// ── the declaration ──────────────────────────────────────────────────────────

#[test]
fn a_declared_embedding_loads_when_the_parent_is_scoped_alike() {
    let (line, lines) = scoped_line();
    let order = stored("Order", vec![declared("lines", list_of(object("OrderLine")))]);

    loads(&schema(vec![order, line], vec![list_query("orders", "Order", tenant()), lines]));
}

/// The declaration is a claim the schema can contradict: each way it is false is refused.
#[test]
fn a_declaration_the_parent_does_not_bear_out_is_refused() {
    let other_column = json!({ "org_id": { "source": "jwt", "claim": "org_id" } });
    let other_claim = json!({ "tenant_id": { "source": "jwt", "claim": "sub" } });
    for (why, parent_scope) in [
        ("the parent's read is not scoped", Value::Null),
        ("the parent is scoped by another column", other_column),
        ("the parent takes the column from another claim", other_claim),
    ] {
        let (line, lines) = scoped_line();
        let order = stored("Order", vec![declared("lines", list_of(object("OrderLine")))]);

        let message = refusal(&schema(
            vec![order, line],
            vec![list_query("orders", "Order", parent_scope), lines],
        ));

        assert!(
            message.contains("Order.lines") && message.contains("tenant_id"),
            "{why}: {message}"
        );
        assert!(message.contains("orders"), "{why}: names the read that falls short: {message}");
    }
}

/// Every read of the parent serves its document, so every one of them carries the scope.
#[test]
fn a_declaration_is_refused_when_one_read_of_the_parent_is_unscoped() {
    let (line, lines) = scoped_line();
    let order = stored("Order", vec![declared("lines", list_of(object("OrderLine")))]);
    let mut by_id = list_query("order", "Order", Value::Null);
    by_id["returns_list"] = json!(false);

    let message = refusal(&schema(
        vec![order, line],
        vec![list_query("orders", "Order", tenant()), by_id, lines],
    ));

    assert!(message.contains("Order.lines") && message.contains("`order`"), "{message}");
}

/// A declaration with nothing to declare would stay behind when the scope is removed.
#[test]
fn a_declaration_on_a_field_that_embeds_no_scoped_type_is_refused() {
    let note = stored("Note", vec![]);
    let order = stored("Order", vec![declared("note", object("Note"))]);

    let message = refusal(&schema(
        vec![order, note],
        vec![
            list_query("orders", "Order", tenant()),
            list_query("notes", "Note", Value::Null),
        ],
    ));

    assert!(
        message.contains("Order.note") && message.contains("scoped_like_parent"),
        "{message}"
    );
}

// ── what a document is ───────────────────────────────────────────────────────

/// The issue's own shape: `Order` → `Section` → `OrderLine`. `Section` has no relation; the
/// lines are in `Order`'s document, and the declaration goes where the scoped type appears.
#[test]
fn a_scoped_type_under_a_value_object_is_in_the_document_that_holds_it() {
    let build = |lines_field: Value, order_scope: Value| {
        let (line, lines) = scoped_line();
        let section = value_object("Section", vec![field("id", json!("ID")), lines_field]);
        let order = stored("Order", vec![field("sections", list_of(object("Section")))]);
        schema(
            vec![order, section, line],
            vec![list_query("orders", "Order", order_scope), lines],
        )
    };
    let lines = || list_of(object("OrderLine"));

    let undeclared = refusal(&build(field("lines", lines()), tenant()));
    assert!(undeclared.contains("Section.lines"), "{undeclared}");
    loads(&build(declared("lines", lines()), tenant()));
    let unscoped_root = refusal(&build(declared("lines", lines()), Value::Null));
    assert!(
        unscoped_root.contains("Section.lines") && unscoped_root.contains("Order"),
        "names the document that reaches it: {unscoped_root}"
    );
}

/// A value object shared by two documents: the declaration holds only if both bear it out.
#[test]
fn a_declaration_under_a_shared_value_object_needs_every_document_scoped_alike() {
    let build = |report_scope: Value| {
        let (line, lines) = scoped_line();
        let section = value_object(
            "Section",
            vec![
                field("id", json!("ID")),
                declared("lines", list_of(object("OrderLine"))),
            ],
        );
        let order = stored("Order", vec![field("sections", list_of(object("Section")))]);
        let report = stored("Report", vec![field("section", object("Section"))]);
        schema(
            vec![order, report, section, line],
            vec![
                list_query("orders", "Order", tenant()),
                list_query("reports", "Report", report_scope),
                lines,
            ],
        )
    };

    loads(&build(tenant()));
    let message = refusal(&build(Value::Null));
    assert!(message.contains("Section.lines") && message.contains("Report"), "{message}");
}

/// A scoped type is reached through a list, an interface or a union alike.
#[test]
fn a_scoped_type_is_found_through_a_list_an_interface_and_a_union() {
    for (shape, field_type) in [
        ("a list of lists", list_of(list_of(object("OrderLine")))),
        ("an interface it implements", json!({ "Interface": "Billable" })),
        ("a union it is a member of", json!({ "Union": "LineOrNote" })),
    ] {
        let (mut line, lines) = scoped_line();
        line["implements"] = json!(["Billable"]);
        let note = stored("Note", vec![]);
        let order = stored("Order", vec![field("item", field_type)]);
        let mut document = schema(
            vec![order, line, note],
            vec![
                list_query("orders", "Order", tenant()),
                lines,
                list_query("notes", "Note", Value::Null),
            ],
        );
        document["interfaces"] =
            json!([{ "name": "Billable", "fields": [{ "name": "id", "field_type": "ID" }] }]);
        document["unions"] =
            json!([{ "name": "LineOrNote", "member_types": ["OrderLine", "Note"] }]);

        let message = refusal(&document);

        assert!(
            message.contains("Order.item") && message.contains("OrderLine"),
            "{shape}: {message}"
        );
    }
}

// ── what the rule leaves alone ───────────────────────────────────────────────

#[test]
fn an_unscoped_type_and_a_value_object_embed_freely() {
    let note = stored("Note", vec![]);
    let money = value_object("Money", vec![field("amount", json!("Float"))]);
    let order = stored(
        "Order",
        vec![
            field("note", object("Note")),
            field("total", object("Money")),
        ],
    );

    loads(&schema(
        vec![order, note, money],
        vec![
            list_query("orders", "Order", tenant()),
            list_query("notes", "Note", Value::Null),
        ],
    ));
}

/// A mutation's payload is not a stored document: its function builds it for the caller, and
/// the engine filters the scoped levels of what it returns. It carries the entity undeclared.
#[test]
fn a_mutation_payload_carries_a_scoped_entity_undeclared() {
    let (line, lines) = scoped_line();
    let payload = value_object("CreateLineSuccess", vec![field("line", object("OrderLine"))]);
    let mut document = schema(vec![payload, line], vec![lines]);
    document["mutations"] = json!([{
        "name": "createLine",
        "return_type": "CreateLineSuccess",
        "sql_source": "fn_create_line",
        "arguments": []
    }]);

    loads(&document);
}

// ── every way a document is served ───────────────────────────────────────────

/// A type no query returns is still served: resolved as a federation entity, scoped by its
/// own `inject_params`. Its document obeys the rule, and its scope bears a declaration out.
#[test]
fn a_federation_entity_no_query_returns_is_a_document_too() {
    let build = |lines_field: Value, entity_scope: Value| {
        let (line, lines) = scoped_line();
        let mut order = stored("Order", vec![lines_field]);
        if !entity_scope.is_null() {
            order["inject_params"] = entity_scope;
        }
        let mut document = schema(vec![order, line], vec![lines]);
        document["federation"] = json!({
            "enabled": true,
            "version": "v2",
            "entities": [{ "name": "Order", "key_fields": ["id"] }]
        });
        document
    };
    let lines = || list_of(object("OrderLine"));

    let undeclared = refusal(&build(field("lines", lines()), tenant()));
    assert!(undeclared.contains("Order.lines"), "{undeclared}");
    loads(&build(declared("lines", lines()), tenant()));
    let unscoped = refusal(&build(declared("lines", lines()), Value::Null));
    assert!(unscoped.contains("Order.lines"), "{unscoped}");
}

/// A document holds what its types embed, to any depth. `Shipment` is resolved as an entity
/// and scoped by its own `inject_params`, which bears its declaration out; `Order` embeds
/// it, declared, so `Order`'s readers are served the lines too and `Order` has to bear both
/// declarations out as well.
#[test]
fn a_declaration_is_held_to_every_document_above_it() {
    let build = |order_scope: Value| {
        let (line, lines) = scoped_line();
        let mut shipment =
            stored("Shipment", vec![declared("lines", list_of(object("OrderLine")))]);
        shipment["inject_params"] = tenant();
        let order = stored("Order", vec![declared("shipment", object("Shipment"))]);
        let mut document = schema(
            vec![order, shipment, line],
            vec![list_query("orders", "Order", order_scope), lines],
        );
        document["federation"] = json!({
            "enabled": true,
            "version": "v2",
            "entities": [{ "name": "Shipment", "key_fields": ["id"] }]
        });
        document
    };

    loads(&build(tenant()));
    let message = refusal(&build(Value::Null));
    assert!(message.contains("Shipment.lines") && message.contains("`orders`"), "{message}");
}

/// And so is a type a subscription streams.
#[test]
fn a_type_a_subscription_streams_is_a_document_too() {
    let (line, lines) = scoped_line();
    let order = stored("Order", vec![field("lines", list_of(object("OrderLine")))]);
    let mut document = schema(vec![order, line], vec![lines]);
    document["subscriptions"] =
        json!([{ "name": "orderChanged", "return_type": "Order", "arguments": [] }]);

    let message = refusal(&document);

    assert!(message.contains("Order.lines"), "{message}");
}

// ── the routes `compile --database` samples ──────────────────────────────────

fn loaded(schema: &Value) -> CompiledSchema {
    CompiledSchema::from_json(&schema.to_string(), false).unwrap()
}

/// `Order` reaches `Address` by two fields, and `Address` declares its scoped `lines`:
/// one type, two places in every `Order` document.
fn two_routes_to_one_type(lines_field: Value) -> Value {
    let (line, lines) = scoped_line();
    let address = value_object("Address", vec![lines_field]);
    let order = stored(
        "Order",
        vec![
            field("billing", object("Address")),
            field("shipping", object("Address")),
        ],
    );
    schema(vec![order, address, line], vec![list_query("orders", "Order", tenant()), lines])
}

#[test]
fn a_type_reached_by_two_routes_is_sampled_along_both() {
    let schema = loaded(&two_routes_to_one_type(declared("lines", list_of(object("OrderLine")))));
    let roots = schema.scoped_embedding_routes();

    let [order] = roots.as_slice() else {
        panic!("one served document: {roots:#?}")
    };
    assert_eq!(order.root, "Order");
    assert_eq!(order.reads.iter().map(|q| q.name.as_str()).collect::<Vec<_>>(), ["orders"]);
    let paths: Vec<_> = order.routes.iter().map(|r| r.path.clone()).collect();
    assert_eq!(paths, [vec!["billing", "lines"], vec!["shipping", "lines"]]);
    for route in &order.routes {
        assert_eq!((route.owner, route.field, route.embedded), ("Address", "lines", "OrderLine"));
        assert_eq!(route.columns, ["tenant_id"]);
    }
    assert_eq!(order.unchecked, 0);
}

/// A type scoped by its own `inject_params`, which no query returns, is sampled by the
/// columns it declares: no read of its own carries them.
#[test]
fn a_type_scoped_by_its_own_declaration_is_sampled_by_its_columns() {
    let mut shipment = stored("Shipment", vec![]);
    shipment["inject_params"] = tenant();
    let order = stored("Order", vec![declared("shipment", object("Shipment"))]);
    let schema =
        loaded(&schema(vec![order, shipment], vec![list_query("orders", "Order", tenant())]));
    let roots = schema.scoped_embedding_routes();

    let [order] = roots.as_slice() else {
        panic!("one served document: {roots:#?}")
    };
    let [route] = order.routes.as_slice() else {
        panic!("one route: {order:#?}")
    };
    assert_eq!((route.owner, route.field, route.embedded), ("Order", "shipment", "Shipment"));
    assert_eq!(route.columns, ["tenant_id"]);
}

/// Walking every route is for the data check only: the rule refused at load names the
/// field once, as it did when one route was walked.
#[test]
fn the_load_time_rule_names_a_field_reached_by_two_routes_once() {
    let message = refusal(&two_routes_to_one_type(field("lines", list_of(object("OrderLine")))));

    assert_eq!(
        message,
        "Validation error: a scoped type is embedded in a served document:\n  - \
         `Address.lines` embeds `OrderLine`, whose read `orderLines` \
         is scoped by `tenant_id`: every reader of the document that holds it would be served \
         those rows. Read them through a query of their own, or declare `scoped_like_parent` \
         on the field when the document is scoped by the same key"
    );
}

/// A type that holds its holder back is walked once per route, not around the loop: the
/// route records the field that closes the loop, and stops there.
#[test]
fn a_route_does_not_enter_a_type_twice() {
    let (_, lines) = scoped_line();
    let line = stored("OrderLine", vec![declared("order", object("Order"))]);
    let order = stored("Order", vec![declared("lines", list_of(object("OrderLine")))]);
    let schema =
        loaded(&schema(vec![order, line], vec![list_query("orders", "Order", tenant()), lines]));

    let order = schema
        .scoped_embedding_routes()
        .into_iter()
        .find(|r| r.root == "Order")
        .unwrap();
    let paths: Vec<_> = order.routes.iter().map(|r| r.path.clone()).collect();
    assert_eq!(paths, [vec!["lines"], vec!["lines", "order"]]);
}

#[test]
fn routes_past_the_cap_are_counted_not_recorded() {
    let (line, lines) = scoped_line();
    let extra = 6;
    let fields = (0..super::embedded_scope::SCOPED_ROUTE_CAP + extra)
        .map(|n| declared(&format!("lines{n}"), list_of(object("OrderLine"))))
        .collect();
    let order = stored("Order", fields);
    let schema =
        loaded(&schema(vec![order, line], vec![list_query("orders", "Order", tenant()), lines]));

    let order = schema
        .scoped_embedding_routes()
        .into_iter()
        .find(|r| r.root == "Order")
        .unwrap();
    assert_eq!(order.routes.len(), super::embedded_scope::SCOPED_ROUTE_CAP);
    assert_eq!(order.unchecked, extra);
}

//! What the served `_service { sdl }` tags, read through the path the server serves it by.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use crate::schema::{
    ArgumentDefinition, CompiledSchema, EnumDefinition, EnumValueDefinition, FederationConfig,
    FederationEntity, FieldDefinition, FieldType, InputFieldDefinition, InputObjectDefinition,
    MutationDefinition, QueryDefinition, TypeDefinition,
};

fn object(name: &str) -> FieldType {
    FieldType::Object(name.to_string())
}

fn gated_query(name: &str, returns: &str, role: &str) -> QueryDefinition {
    QueryDefinition {
        requires_role: Some(role.to_string()),
        ..QueryDefinition::new(name, returns)
    }
}

/// A subgraph with something each tagging rule decides:
///
/// - `products` (public) and `audit` (`auditor`) reach the entity `Product`, which reaches the
///   `@shareable` `Money`;
/// - `costReport` (`finance`) alone reaches `CostReport`, which reaches `Money` and the enum
///   `CostBasis`;
/// - `archiveProduct` (`admin`) alone takes the input `ArchiveInput`;
/// - `payroll` returns `Payroll`, a type gated `hr` itself, and nothing returns `Bonus`, also gated
///   `hr`;
/// - `suppliers` (`buyer`) alone reaches the entity `Supplier`;
/// - `ledger` (`finance`) and `ledgerExport` (`auditor`) both reach `Ledger`, and nothing else
///   does.
fn subgraph() -> CompiledSchema {
    let mut s = CompiledSchema::new();
    s.types.push(
        TypeDefinition::new("Money", "v_money")
            .with_field(FieldDefinition::new("amount", FieldType::Int)),
    );
    s.types.push(
        TypeDefinition::new("Product", "v_product")
            .with_field(FieldDefinition::new("id", FieldType::Id))
            .with_field(FieldDefinition::new("price", object("Money"))),
    );
    s.types.push(
        TypeDefinition::new("CostReport", "v_cost_report")
            .with_field(FieldDefinition::new("total", object("Money")))
            .with_field(FieldDefinition::new("basis", FieldType::Enum("CostBasis".to_string()))),
    );
    s.enums
        .push(EnumDefinition::new("CostBasis").with_value(EnumValueDefinition::new("FIFO")));
    s.input_types.push(
        InputObjectDefinition::new("ArchiveInput")
            .with_field(InputFieldDefinition::new("reason", "String")),
    );
    s.types.push(TypeDefinition {
        requires_role: Some("hr".to_string()),
        ..TypeDefinition::new("Payroll", "v_payroll")
            .with_field(FieldDefinition::new("gross", FieldType::Int))
    });
    s.types.push(TypeDefinition {
        requires_role: Some("hr".to_string()),
        ..TypeDefinition::new("Bonus", "v_bonus")
            .with_field(FieldDefinition::new("amount", FieldType::Int))
    });
    s.types.push(
        TypeDefinition::new("Supplier", "v_supplier")
            .with_field(FieldDefinition::new("id", FieldType::Id)),
    );
    s.types.push(
        TypeDefinition::new("Ledger", "v_ledger")
            .with_field(FieldDefinition::new("balance", FieldType::Int)),
    );

    s.queries.push(QueryDefinition::new("products", "Product").returning_list());
    s.queries.push(gated_query("audit", "Product", "auditor"));
    s.queries.push(gated_query("costReport", "CostReport", "finance"));
    s.queries.push(gated_query("payroll", "Payroll", "hr"));
    s.queries.push(gated_query("suppliers", "Supplier", "buyer"));
    s.queries.push(gated_query("ledger", "Ledger", "finance"));
    s.queries.push(gated_query("ledgerExport", "Ledger", "auditor"));
    s.mutations.push(MutationDefinition {
        requires_role: Some("admin".to_string()),
        arguments: vec![ArgumentDefinition::new(
            "input",
            FieldType::Input("ArchiveInput".to_string()),
        )],
        ..MutationDefinition::new("archiveProduct", "Product")
    });

    s.federation = Some(FederationConfig {
        enabled: true,
        version: Some("v2".to_string()),
        entities: ["Product", "Supplier"]
            .map(|name| FederationEntity {
                name: name.to_string(),
                key_fields: vec!["id".to_string()],
                ..Default::default()
            })
            .to_vec(),
        shareable_types: vec!["Money".to_string()],
        ..Default::default()
    });
    s.build_indexes();
    s
}

fn sdl() -> String {
    subgraph().federation_service_sdl().unwrap()
}

/// The line declaring root field `name`.
fn field_line<'a>(sdl: &'a str, name: &str) -> &'a str {
    sdl.lines()
        .find(|l| {
            let l = l.trim_start();
            l.starts_with(&format!("{name}:")) || l.starts_with(&format!("{name}("))
        })
        .unwrap_or_else(|| panic!("no field `{name}` in:\n{sdl}"))
}

/// The line declaring type `name`, of any kind.
fn type_line<'a>(sdl: &'a str, kind: &str, name: &str) -> &'a str {
    let head = format!("{kind} {name} ");
    sdl.lines()
        .find(|l| l.starts_with(&head))
        .unwrap_or_else(|| panic!("no `{kind} {name}` in:\n{sdl}"))
}

fn tag(role: &str) -> String {
    format!(r#"@tag(name: "requires-role/{role}")"#)
}

#[test]
fn a_gated_root_field_carries_its_roles_tag() {
    let sdl = sdl();
    for (field, role) in [
        ("audit", "auditor"),
        ("costReport", "finance"),
        ("archiveProduct", "admin"),
    ] {
        let line = field_line(&sdl, field);
        assert!(line.trim_end().ends_with(&tag(role)), "{line}");
    }
}

#[test]
fn an_ungated_root_field_carries_no_tag() {
    let sdl = sdl();
    let line = field_line(&sdl, "products");
    assert!(!line.contains("@tag"), "{line}");
}

#[test]
fn a_type_only_a_roles_operations_reach_carries_its_tag() {
    let sdl = sdl();
    for (name, role) in [
        ("CostReport", "finance"),
        ("Payroll", "hr"),
        ("Bonus", "hr"),
    ] {
        let line = type_line(&sdl, "type", name);
        assert!(line.contains(&tag(role)), "{line}");
        assert_eq!(line.matches("@tag").count(), 1, "{line}");
    }
}

#[test]
fn a_type_an_ungated_root_reaches_carries_no_tag() {
    let sdl = sdl();
    let line = type_line(&sdl, "type", "Money");
    assert!(!line.contains("@tag"), "{line}");
}

#[test]
fn an_input_or_enum_only_a_roles_operations_reach_is_never_tagged() {
    // Another subgraph may declare the same name, and its declaration would be tagged too.
    let sdl = sdl();
    for (kind, name) in [("enum", "CostBasis"), ("input", "ArchiveInput")] {
        let line = type_line(&sdl, kind, name);
        assert!(!line.contains("@tag"), "{line}");
    }
}

#[test]
fn an_entity_is_never_tagged() {
    let sdl = sdl();
    for name in ["Product", "Supplier"] {
        let line = type_line(&sdl, "type", name);
        assert!(line.contains("@key"), "not rendered as an entity: {line}");
        assert!(!line.contains("@tag"), "{line}");
    }
}

#[test]
fn a_type_two_roles_reach_carries_neither_tag() {
    // Excluding either role leaves it reached through the other: a tag would take it out
    // from under a field the contract keeps.
    let sdl = sdl();
    let line = type_line(&sdl, "type", "Ledger");
    assert!(!line.contains("@tag"), "{line}");
}

#[test]
fn the_tag_directive_is_declared_and_imported() {
    let sdl = sdl();
    let link = sdl.lines().next().unwrap();
    assert!(link.contains(r#""@tag""#), "{link}");
    assert!(sdl.contains("directive @tag(name: String!) repeatable on"), "{sdl}");
}

#[test]
fn a_role_no_contract_tag_can_name_refuses_the_sdl() {
    let role = "r".repeat(crate::federation::MAX_TAG_LEN);
    let on_a_query = |schema: &mut CompiledSchema| {
        schema.queries.push(gated_query("overlong", "Money", &role));
    };
    let on_a_type_alone = |schema: &mut CompiledSchema| {
        schema.types.push(TypeDefinition {
            requires_role: Some(role.clone()),
            ..TypeDefinition::new("Overlong", "v_overlong")
        });
    };
    for gate in [
        &on_a_query as &dyn Fn(&mut CompiledSchema),
        &on_a_type_alone,
    ] {
        let mut schema = subgraph();
        gate(&mut schema);
        schema.build_indexes();
        let err = schema.federation_service_sdl().unwrap_err();
        assert!(err.to_string().contains(&role), "{err}");
    }
}

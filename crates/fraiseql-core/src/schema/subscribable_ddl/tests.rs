//! Unit tests for the #366 capture-trigger DDL generator (no database required).

#![allow(clippy::unwrap_used)] // Reason: test module

use super::*;
use crate::schema::{CompiledSchema, SubscribableEntity};

fn schema_with(entities: Vec<SubscribableEntity>) -> CompiledSchema {
    CompiledSchema {
        subscribable: entities,
        ..Default::default()
    }
}

fn one(entity_type: &str, tables: &[&str]) -> SubscribableEntity {
    SubscribableEntity {
        entity_type: entity_type.to_string(),
        tables:      tables.iter().map(|t| (*t).to_string()).collect(),
    }
}

#[test]
fn empty_schema_yields_empty_ddl() {
    assert!(generate_capture_trigger_ddl(&CompiledSchema::default()).is_empty());
    assert!(generate_capture_trigger_ddl(&schema_with(vec![])).is_empty());
}

#[test]
fn emits_three_statement_level_triggers_per_table() {
    let ddl = generate_capture_trigger_ddl(&schema_with(vec![one("Post", &["tb_post"])]));
    // One trigger each for INSERT / UPDATE / DELETE.
    assert_eq!(ddl.matches("CREATE TRIGGER").count(), 3, "{ddl}");
    assert!(ddl.contains("AFTER INSERT ON \"tb_post\""));
    assert!(ddl.contains("AFTER UPDATE ON \"tb_post\""));
    assert!(ddl.contains("AFTER DELETE ON \"tb_post\""));
    // Statement-level + transition tables (bulk-efficient).
    assert!(ddl.contains("FOR EACH STATEMENT"));
    assert!(ddl.contains("REFERENCING NEW TABLE AS new_table"));
    assert!(ddl.contains("REFERENCING OLD TABLE AS old_table NEW TABLE AS new_table"));
    // Idempotent install.
    assert_eq!(ddl.matches("DROP TRIGGER IF EXISTS").count(), 3);
}

#[test]
fn passes_entity_type_and_convention_columns_as_args() {
    let ddl = generate_capture_trigger_ddl(&schema_with(vec![one("Post", &["tb_post"])]));
    assert!(
        ddl.contains(
            "EXECUTE FUNCTION core.fn_entity_change_log_capture('Post', 'id', 'tenant_id')"
        ),
        "object_type is the GraphQL type name; pk/tenant use the conventions: {ddl}"
    );
}

#[test]
fn schema_qualified_table_quotes_both_parts() {
    let ddl = generate_capture_trigger_ddl(&schema_with(vec![one("Post", &["public.tb_post"])]));
    assert!(ddl.contains("ON \"public\".\"tb_post\""), "quotes schema and table: {ddl}");
    // The trigger NAME must not contain the dot (illegal in an identifier).
    assert!(ddl.contains("\"tr_cdc_capture_ins_tb_post\""), "{ddl}");
    assert!(!ddl.contains("tr_cdc_capture_ins_public.tb_post"));
}

#[test]
fn multiple_tables_and_entities_each_get_triggers() {
    let ddl = generate_capture_trigger_ddl(&schema_with(vec![
        one("Post", &["tb_post", "tb_post_archive"]),
        one("Comment", &["tb_comment"]),
    ]));
    // 3 tables × 3 ops.
    assert_eq!(ddl.matches("CREATE TRIGGER").count(), 9, "{ddl}");
    assert!(ddl.contains("('Post', 'id', 'tenant_id')"));
    assert!(ddl.contains("('Comment', 'id', 'tenant_id')"));
}

#[test]
fn invalid_table_name_is_skipped_with_a_warning_not_unsafe_sql() {
    let ddl = generate_capture_trigger_ddl(&schema_with(vec![one(
        "Post",
        &["tb_post; DROP TABLE users"],
    )]));
    assert!(ddl.contains("-- WARNING: skipped"), "injection-y name is skipped: {ddl}");
    assert_eq!(ddl.matches("CREATE TRIGGER").count(), 0, "no triggers for an unsafe name");
    // The rejected name may echo in the WARNING, but only inside a `--` comment —
    // never on an executable line.
    for line in ddl.lines().filter(|l| l.contains("DROP TABLE users")) {
        assert!(
            line.trim_start().starts_with("--"),
            "the rejected name only ever appears in a comment, not executable SQL: {line:?}"
        );
    }
}

#[test]
fn entity_type_with_a_quote_is_escaped() {
    let ddl = generate_capture_trigger_ddl(&schema_with(vec![one("Wei'rd", &["tb_x"])]));
    assert!(ddl.contains("('Wei''rd', 'id', 'tenant_id')"), "single quote doubled: {ddl}");
}

#[test]
fn long_table_name_trigger_stays_within_the_63_byte_cap() {
    let long = "tb_".to_string() + &"a".repeat(80);
    let ddl = generate_capture_trigger_ddl(&schema_with(vec![one("Post", &[&long])]));
    for line in ddl.lines().filter(|l| l.contains("CREATE TRIGGER")) {
        // Extract the quoted trigger name and assert its byte length.
        let name = line.split('"').nth(1).unwrap();
        assert!(name.len() <= 63, "trigger name `{name}` is {} bytes (> 63)", name.len());
    }
    // The full table is still quoted untruncated in the ON target.
    assert!(ddl.contains(&format!("ON \"{long}\"")), "ON target is not truncated");
}

#[test]
fn distinct_long_tables_get_distinct_trigger_names() {
    let a = "tb_".to_string() + &"x".repeat(80) + "_alpha";
    let b = "tb_".to_string() + &"x".repeat(80) + "_beta";
    let ddl = generate_capture_trigger_ddl(&schema_with(vec![one("A", &[&a]), one("B", &[&b])]));
    let names: Vec<&str> = ddl
        .lines()
        .filter(|l| l.contains("CREATE TRIGGER"))
        .filter_map(|l| l.split('"').nth(1))
        .collect();
    let ins_names: Vec<&&str> = names.iter().filter(|n| n.contains("_ins_")).collect();
    assert_eq!(ins_names.len(), 2);
    assert_ne!(ins_names[0], ins_names[1], "hash suffix disambiguates truncated names");
}

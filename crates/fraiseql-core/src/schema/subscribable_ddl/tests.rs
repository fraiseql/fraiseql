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
        pre_image:   false,
    }
}

fn one_pre_image(entity_type: &str, tables: &[&str]) -> SubscribableEntity {
    SubscribableEntity {
        pre_image: true,
        ..one(entity_type, tables)
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
    // A non-opted-in entity passes exactly three args — byte-identical to before
    // the pre-image flag existed (the function COALESCEs an absent TG_ARGV[3]).
    assert!(!ddl.contains("'tenant_id', 'true'"), "no pre-image 4th arg when off: {ddl}");
}

#[test]
fn pre_image_opt_in_appends_the_flag_as_the_fourth_trigger_arg() {
    let ddl = generate_capture_trigger_ddl(&schema_with(vec![one_pre_image("Post", &["tb_post"])]));
    // changelog_pre_image parity: an opted-in entity passes 'true' as TG_ARGV[3],
    // so the capture function records OLD into object_data_before for its tables.
    assert!(
        ddl.contains(
            "EXECUTE FUNCTION core.fn_entity_change_log_capture('Post', 'id', 'tenant_id', 'true')"
        ),
        "pre-image opt-in appends 'true' as the 4th capture-function arg: {ddl}"
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
    let long = "tb_".to_string() + "a".repeat(80).as_str();
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
    let a = "tb_".to_string() + "x".repeat(80).as_str() + "_alpha";
    let b = "tb_".to_string() + "x".repeat(80).as_str() + "_beta";
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

// ── a table that keeps its entity document in a column ──

/// `Ticket`, captured from `tb_ticket`, which `[subscriptions.capture.tb_ticket]` declares
/// to keep its document in `data`.
fn document_table_schema() -> CompiledSchema {
    use crate::schema::{CaptureTableConfig, SubscriptionsConfig};

    let mut schema = schema_with(vec![one("Ticket", &["tb_ticket"]), one("Post", &["tb_post"])]);
    let mut subscriptions = SubscriptionsConfig::default();
    subscriptions.capture.insert(
        "tb_ticket".to_string(),
        CaptureTableConfig {
            document_column: "data".to_string(),
        },
    );
    schema.subscriptions_config = Some(subscriptions);
    schema
}

/// A list read of `Ticket` scoped by `tenant_id` and `region` from the token, `native`
/// naming those of them its relation holds as columns.
fn scoped_read(native: &[&str]) -> crate::schema::QueryDefinition {
    use crate::schema::{InjectedParamSource, NativeColumn, QueryDefinition};

    let mut tickets = QueryDefinition::new("tickets", "Ticket")
        .returning_list()
        .with_sql_source("v_ticket");
    for column in ["tenant_id", "region"] {
        tickets
            .inject_params
            .insert(column.to_string(), InjectedParamSource::Jwt(column.to_string()));
    }
    for column in native {
        tickets
            .native_columns
            .insert((*column).to_string(), NativeColumn::nullable("text"));
    }
    tickets
}

#[test]
fn a_declared_document_column_is_the_fifth_trigger_arg() {
    let ddl = generate_capture_trigger_ddl(&document_table_schema());
    assert_eq!(
        ddl.matches(
            "core.fn_entity_change_log_capture('Ticket', 'id', 'tenant_id', 'false', 'data');"
        )
        .count(),
        3,
        "{ddl}"
    );
    // A table with no entry is generated as it was before the entry existed.
    assert_eq!(
        ddl.matches("core.fn_entity_change_log_capture('Post', 'id', 'tenant_id');")
            .count(),
        3,
        "{ddl}"
    );
}

#[test]
fn a_document_column_keeps_the_pre_image_flag_in_its_place() {
    let mut schema = document_table_schema();
    schema.subscribable[0].pre_image = true;
    let ddl = generate_capture_trigger_ddl(&schema);
    assert!(
        ddl.contains(
            "core.fn_entity_change_log_capture('Ticket', 'id', 'tenant_id', 'true', 'data');"
        ),
        "{ddl}"
    );
}

#[test]
fn the_native_scope_columns_of_the_types_read_are_the_sixth_trigger_arg() {
    let mut schema = document_table_schema();
    schema.queries.push(scoped_read(&["tenant_id", "region"]));
    let ddl = generate_capture_trigger_ddl(&schema);
    assert!(
        ddl.contains(
            "core.fn_entity_change_log_capture('Ticket', 'id', 'tenant_id', 'false', 'data', \
             'tenant_id,region');"
        ),
        "{ddl}"
    );
    // `Post` is read by no scoped query, and its table keeps no document.
    assert!(ddl.contains("core.fn_entity_change_log_capture('Post', 'id', 'tenant_id');"));
}

#[test]
fn a_scope_column_the_read_takes_from_the_document_is_not_listed() {
    let mut schema = document_table_schema();
    schema.queries.push(scoped_read(&["region"]));
    let ddl = generate_capture_trigger_ddl(&schema);
    assert!(
        ddl.contains(
            "core.fn_entity_change_log_capture('Ticket', 'id', 'tenant_id', 'false', 'data', \
             'region');"
        ),
        "{ddl}"
    );

    let mut schema = document_table_schema();
    schema.queries.push(scoped_read(&[]));
    let ddl = generate_capture_trigger_ddl(&schema);
    assert!(
        ddl.contains(
            "core.fn_entity_change_log_capture('Ticket', 'id', 'tenant_id', 'false', 'data');"
        ),
        "{ddl}"
    );
}

#[test]
fn a_capture_entry_for_a_table_no_type_declares_is_refused_by_name() {
    let mut schema = document_table_schema();
    let entry = schema
        .subscriptions_config
        .as_mut()
        .unwrap()
        .capture
        .remove("tb_ticket")
        .unwrap();
    schema
        .subscriptions_config
        .as_mut()
        .unwrap()
        .capture
        .insert("tb_tiket".to_string(), entry);
    let violations = schema.capture_violations();
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert!(violations[0].contains("`tb_tiket`"), "{violations:?}");
    assert!(violations[0].contains("`tb_post`, `tb_ticket`"), "{violations:?}");
}

#[test]
fn a_document_column_that_is_no_identifier_is_refused_by_name() {
    let mut schema = document_table_schema();
    schema
        .subscriptions_config
        .as_mut()
        .unwrap()
        .capture
        .get_mut("tb_ticket")
        .unwrap()
        .document_column = "data->'doc'".to_string();
    let violations = schema.capture_violations();
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert!(
        violations[0].contains("`subscriptions.capture.tb_ticket.document_column`"),
        "{violations:?}"
    );
    assert!(violations[0].contains("`data->'doc'`"), "{violations:?}");
}

#[test]
fn a_schema_whose_capture_entries_name_declared_tables_has_no_violation() {
    assert!(document_table_schema().capture_violations().is_empty());
    assert!(CompiledSchema::default().capture_violations().is_empty());
}

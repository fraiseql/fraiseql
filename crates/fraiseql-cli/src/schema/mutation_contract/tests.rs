//! Unit tests for the pure mutation-contract logic (no database).

use fraiseql_core::schema::{
    ArgumentDefinition, CompiledSchema, FieldType, InjectedParamSource, InputFieldDefinition,
    InputObjectDefinition, MutationDefinition, MutationOperation,
};
use indexmap::IndexMap;

use super::*;
use crate::schema::pg_catalog::{OutColumn, PgFunction};

// ─── Fixtures ───────────────────────────────────────────────────────────────

fn single_input_arg(type_name: &str) -> ArgumentDefinition {
    ArgumentDefinition::new("input", FieldType::Input(type_name.to_string()))
}

fn input_type(name: &str, field_count: usize) -> InputObjectDefinition {
    let fields = (0..field_count)
        .map(|i| InputFieldDefinition::new(format!("f{i}"), "String"))
        .collect();
    InputObjectDefinition::new(name).with_fields(fields)
}

fn inject(keys: &[&str]) -> IndexMap<String, InjectedParamSource> {
    keys.iter()
        .map(|k| ((*k).to_string(), InjectedParamSource::Jwt((*k).to_string())))
        .collect()
}

fn col(name: &str, type_name: &str) -> OutColumn {
    OutColumn {
        name:      name.to_string(),
        type_name: type_name.to_string(),
        is_enum:   false,
    }
}

/// The canonical 13-column `mutation_response` result row.
fn full_response_columns() -> Vec<OutColumn> {
    vec![
        col("succeeded", "boolean"),
        col("state_changed", "boolean"),
        col("error_class", "text"),
        col("status_detail", "text"),
        col("http_status", "smallint"),
        col("message", "text"),
        col("entity_id", "uuid"),
        col("entity_type", "text"),
        col("entity", "jsonb"),
        col("updated_fields", "text[]"),
        col("cascade", "jsonb"),
        col("error_detail", "jsonb"),
        col("metadata", "jsonb"),
    ]
}

fn pg_function(in_types: &[&str], in_names: &[Option<&str>], out: Vec<OutColumn>) -> PgFunction {
    PgFunction {
        schema:      "public".to_string(),
        name:        "fn_test".to_string(),
        in_types:    in_types.iter().map(|s| (*s).to_string()).collect(),
        in_names:    in_names.iter().map(|n| n.map(str::to_string)).collect(),
        out_columns: out,
        returns_set: true,
    }
}

// ─── expected_call: shape derivation ────────────────────────────────────────

#[test]
fn update_single_input_is_jsonb_payload_even_without_input_type_in_schema() {
    let mut m = MutationDefinition::new("updateUser", "UpdateUserResult");
    m.sql_source = Some("fn_update_user".to_string());
    m.operation = MutationOperation::Update {
        table: "tb_user".to_string(),
    };
    m.arguments = vec![single_input_arg("UpdateUserInput")];
    // No UpdateUserInput in the schema on purpose — the update path doesn't look it up.
    let schema = CompiledSchema::default();

    let ec = expected_call(&m, &schema).expect("update mutation is DB-backed");
    assert_eq!(ec.shape, CallShape::JsonbPayload);
    assert_eq!(ec.base_arity, 1);
    assert!(ec.first_is_jsonb_payload);
    assert_eq!(ec.total_arity(), 1);
}

#[test]
fn insert_single_input_flattens_input_type_fields() {
    let mut m = MutationDefinition::new("createUser", "CreateUserResult");
    m.sql_source = Some("fn_create_user".to_string());
    m.operation = MutationOperation::Insert {
        table: "tb_user".to_string(),
    };
    m.arguments = vec![single_input_arg("CreateUserInput")];
    let schema = CompiledSchema {
        input_types: vec![input_type("CreateUserInput", 3)],
        ..Default::default()
    };

    let ec = expected_call(&m, &schema).expect("insert mutation is DB-backed");
    assert_eq!(ec.shape, CallShape::FlattenedFields);
    assert_eq!(ec.base_arity, 3);
    assert!(!ec.first_is_jsonb_payload);
}

#[test]
fn insert_single_input_falls_back_to_flat_when_type_missing() {
    let mut m = MutationDefinition::new("createUser", "CreateUserResult");
    m.sql_source = Some("fn_create_user".to_string());
    m.operation = MutationOperation::Insert {
        table: String::new(),
    };
    m.arguments = vec![single_input_arg("CreateUserInput")];
    // CreateUserInput absent → flatten can't happen → flat args (1 positional).
    let schema = CompiledSchema::default();

    let ec = expected_call(&m, &schema).expect("DB-backed");
    assert_eq!(ec.shape, CallShape::FlatArgs);
    assert_eq!(ec.base_arity, 1);
}

#[test]
fn flat_args_count_each_argument() {
    let mut m = MutationDefinition::new("archive", "ArchiveResult");
    m.sql_source = Some("fn_archive".to_string());
    m.arguments = vec![
        ArgumentDefinition::new("id", FieldType::Uuid),
        ArgumentDefinition::new("reason", FieldType::String),
    ];
    let schema = CompiledSchema::default();

    let ec = expected_call(&m, &schema).expect("DB-backed");
    assert_eq!(ec.shape, CallShape::FlatArgs);
    assert_eq!(ec.base_arity, 2);
}

#[test]
fn single_input_with_non_input_type_is_flat() {
    let mut m = MutationDefinition::new("touch", "TouchResult");
    m.sql_source = Some("fn_touch".to_string());
    // arg is named "input" but typed as a scalar, not an Input object.
    m.arguments = vec![ArgumentDefinition::new("input", FieldType::String)];
    let schema = CompiledSchema::default();

    let ec = expected_call(&m, &schema).expect("DB-backed");
    assert_eq!(ec.shape, CallShape::FlatArgs);
    assert_eq!(ec.base_arity, 1);
}

#[test]
fn inject_params_extend_total_arity_in_order() {
    let mut m = MutationDefinition::new("updateUser", "UpdateUserResult");
    m.sql_source = Some("fn_update_user".to_string());
    m.operation = MutationOperation::Update {
        table: "tb_user".to_string(),
    };
    m.arguments = vec![single_input_arg("UpdateUserInput")];
    m.inject_params = inject(&["tenant_id", "sub"]);
    let schema = CompiledSchema::default();

    let ec = expected_call(&m, &schema).expect("DB-backed");
    assert_eq!(ec.base_arity, 1);
    assert_eq!(ec.inject_names, vec!["tenant_id".to_string(), "sub".to_string()]);
    assert_eq!(ec.total_arity(), 3);
}

#[test]
fn non_db_backed_mutation_is_skipped() {
    let mut m = MutationDefinition::new("notifyExternal", "NotifyResult");
    // No sql_source, Custom operation (no table) → not DB-backed.
    m.operation = MutationOperation::Custom;
    let schema = CompiledSchema::default();
    assert!(expected_call(&m, &schema).is_none());
}

#[test]
fn falls_back_to_operation_table_when_sql_source_absent() {
    let mut m = MutationDefinition::new("createUser", "CreateUserResult");
    m.operation = MutationOperation::Insert {
        table: "fn_create_user".to_string(),
    };
    let schema = CompiledSchema::default();
    let ec = expected_call(&m, &schema).expect("table fallback is DB-backed");
    assert_eq!(ec.sql_source, "fn_create_user");
}

// ─── check_mutation: call binding ───────────────────────────────────────────

fn jsonb_update_call() -> ExpectedCall {
    ExpectedCall {
        sql_source:             "fn_update_user".to_string(),
        shape:                  CallShape::JsonbPayload,
        base_arity:             1,
        inject_names:           vec!["tenant_id".to_string()],
        first_is_jsonb_payload: true,
    }
}

#[test]
fn missing_function_when_no_candidates() {
    let v = check_mutation(&jsonb_update_call(), &[]);
    assert_eq!(v, vec![ContractViolation::MissingFunction]);
}

#[test]
fn arity_mismatch_reports_found_arities() {
    let candidates = vec![pg_function(
        &["jsonb"],
        &[Some("input")],
        full_response_columns(),
    )];
    // expects 2 (1 payload + 1 inject) but the candidate takes 1.
    let v = check_mutation(&jsonb_update_call(), &candidates);
    assert_eq!(
        v,
        vec![ContractViolation::ArityMismatch {
            expected: 2,
            found:    vec![1],
        }]
    );
}

#[test]
fn ambiguous_when_two_overloads_match_arity() {
    let f = || {
        pg_function(
            &["jsonb", "uuid"],
            &[Some("input"), Some("tenant_id")],
            full_response_columns(),
        )
    };
    let v = check_mutation(&jsonb_update_call(), &[f(), f()]);
    assert_eq!(v, vec![ContractViolation::AmbiguousFunction { arity: 2, count: 2 }]);
}

#[test]
fn payload_not_jsonb_flagged() {
    let candidates = vec![pg_function(
        &["text", "uuid"],
        &[Some("input"), Some("tenant_id")],
        full_response_columns(),
    )];
    let v = check_mutation(&jsonb_update_call(), &candidates);
    assert!(v.contains(&ContractViolation::PayloadNotJsonb {
        actual: "text".to_string(),
    }));
}

#[test]
fn happy_path_has_no_violations() {
    let candidates = vec![pg_function(
        &["jsonb", "uuid"],
        &[Some("input"), Some("tenant_id")],
        full_response_columns(),
    )];
    let v = check_mutation(&jsonb_update_call(), &candidates);
    assert!(v.is_empty(), "expected no violations, got {v:?}");
}

#[test]
fn inject_name_mismatch_is_a_warning() {
    let candidates = vec![pg_function(
        &["jsonb", "uuid"],
        &[Some("input"), Some("org_id")], // server binds inject `tenant_id` here
        full_response_columns(),
    )];
    let v = check_mutation(&jsonb_update_call(), &candidates);
    let mismatch = v
        .iter()
        .find(|x| matches!(x, ContractViolation::InjectNameMismatch { .. }))
        .expect("inject name mismatch reported");
    assert_eq!(mismatch.severity(), Severity::Warn);
}

#[test]
fn unnamed_inject_param_position_is_not_flagged() {
    // Function with no parameter names → can't verify inject names → no warning.
    let candidates = vec![pg_function(
        &["jsonb", "uuid"],
        &[None, None],
        full_response_columns(),
    )];
    let v = check_mutation(&jsonb_update_call(), &candidates);
    assert!(
        !v.iter().any(|x| matches!(x, ContractViolation::InjectNameMismatch { .. })),
        "unnamed params should not trigger an inject-name mismatch: {v:?}"
    );
}

// ─── check_mutation: response shape ─────────────────────────────────────────

fn flat_call() -> ExpectedCall {
    ExpectedCall {
        sql_source:             "fn_create".to_string(),
        shape:                  CallShape::FlatArgs,
        base_arity:             1,
        inject_names:           vec![],
        first_is_jsonb_payload: false,
    }
}

#[test]
fn missing_succeeded_column_is_an_error() {
    let mut cols = full_response_columns();
    cols.retain(|c| c.name != "succeeded");
    let candidates = vec![pg_function(&["jsonb"], &[Some("input")], cols)];
    let v = check_mutation(&flat_call(), &candidates);
    let missing = v
        .iter()
        .find(|x| {
            matches!(
                x,
                ContractViolation::MissingRequiredColumn {
                    column: "succeeded",
                }
            )
        })
        .expect("missing succeeded reported");
    assert_eq!(missing.severity(), Severity::Error);
}

#[test]
fn wrong_succeeded_type_is_an_error() {
    let mut cols = full_response_columns();
    cols[0] = col("succeeded", "text");
    let candidates = vec![pg_function(&["jsonb"], &[Some("input")], cols)];
    let v = check_mutation(&flat_call(), &candidates);
    assert!(v.contains(&ContractViolation::RequiredColumnWrongType {
        column: "succeeded",
        actual: "text".to_string(),
    }));
}

#[test]
fn optional_column_wrong_type_is_a_warning() {
    let mut cols = full_response_columns();
    // entity declared as text instead of jsonb.
    if let Some(c) = cols.iter_mut().find(|c| c.name == "entity") {
        c.type_name = "text".to_string();
    }
    let candidates = vec![pg_function(&["jsonb"], &[Some("input")], cols)];
    let v = check_mutation(&flat_call(), &candidates);
    let warn = v
        .iter()
        .find(|x| {
            matches!(
                x,
                ContractViolation::OptionalColumnWrongType {
                    column: "entity",
                    ..
                }
            )
        })
        .expect("entity type warning reported");
    assert_eq!(warn.severity(), Severity::Warn);
}

#[test]
fn error_class_enum_is_accepted() {
    let mut cols = full_response_columns();
    if let Some(c) = cols.iter_mut().find(|c| c.name == "error_class") {
        c.type_name = "app.mutation_error_class".to_string();
        c.is_enum = true;
    }
    let candidates = vec![pg_function(&["jsonb"], &[Some("input")], cols)];
    let v = check_mutation(&flat_call(), &candidates);
    assert!(
        !v.iter().any(|x| matches!(
            x,
            ContractViolation::OptionalColumnWrongType {
                column: "error_class",
                ..
            }
        )),
        "enum error_class must be accepted: {v:?}"
    );
}

#[test]
fn scalar_return_is_unverifiable() {
    let candidates = vec![pg_function(&["jsonb"], &[Some("input")], vec![])];
    let v = check_mutation(&flat_call(), &candidates);
    let unverifiable = v
        .iter()
        .find(|x| matches!(x, ContractViolation::ResponseShapeUnverifiable))
        .expect("unverifiable reported");
    assert_eq!(unverifiable.severity(), Severity::Warn);
}

// ─── Report aggregation ─────────────────────────────────────────────────────

#[test]
fn report_counts_split_by_severity() {
    let mut report = ContractReport {
        checked:   2,
        skipped:   1,
        mutations: vec![],
    };
    report.mutations.push(MutationReport {
        mutation:   "updateUser".to_string(),
        sql_source: "fn_update_user".to_string(),
        violations: vec![
            ContractViolation::MissingRequiredColumn {
                column: "succeeded",
            }, // error
            ContractViolation::ResponseShapeUnverifiable, // warn
        ],
    });
    assert_eq!(report.error_count(), 1);
    assert_eq!(report.warn_count(), 1);
    let json = report.to_json();
    assert_eq!(json["errors"], 1);
    assert_eq!(json["warnings"], 1);
    assert_eq!(json["checked"], 2);
    assert_eq!(json["skipped"], 1);
}

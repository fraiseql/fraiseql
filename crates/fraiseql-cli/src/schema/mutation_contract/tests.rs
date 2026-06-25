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
fn insert_single_input_takes_jsonb_path_when_type_missing() {
    let mut m = MutationDefinition::new("createUser", "CreateUserResult");
    m.sql_source = Some("fn_create_user".to_string());
    m.operation = MutationOperation::Insert {
        table: String::new(),
    };
    m.arguments = vec![single_input_arg("CreateUserInput")];
    // CreateUserInput absent → the runtime can't flatten an unknown type, so it
    // forwards the whole input as one jsonb arg (`!known_input_type`, mirroring
    // mutation/mod.rs:500). The gate must agree: JsonbPayload(1), arg-1 is jsonb.
    let schema = CompiledSchema::default();

    let ec = expected_call(&m, &schema).expect("DB-backed");
    assert_eq!(ec.shape, CallShape::JsonbPayload);
    assert_eq!(ec.base_arity, 1);
    assert!(ec.first_is_jsonb_payload);
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

// ─── #484: single-JSONB arity mirror (input_style == Jsonb) ──────────────────
//
// The contract gate must mirror the runtime predicate
// (`mutation/mod.rs:499-500`): a single structured `input` arg is passed as ONE
// jsonb payload when the op is Update, OR `input_style == Jsonb`, OR the input
// type is unknown. Before the fix the gate only routed `Update` to JsonbPayload,
// so every single-JSONB Insert/Delete/Custom false-failed with ArityMismatch.

/// End-to-end: derive the expected call from the schema, then check it against
/// the candidate functions — exactly what `doctor`/`validate --against-db` do.
fn check(
    m: &MutationDefinition,
    schema: &CompiledSchema,
    candidates: &[PgFunction],
) -> Vec<ContractViolation> {
    check_mutation(&expected_call(m, schema).expect("DB-backed mutation"), candidates)
}

/// `createOrder` with a single 6-field `input`, given operation + input style.
fn order_mutation(
    op: MutationOperation,
    style: fraiseql_core::schema::InputStyle,
) -> MutationDefinition {
    let mut m = MutationDefinition::new("createOrder", "CreateOrderResult");
    m.sql_source = Some("fn_create_order".to_string());
    m.operation = op;
    m.input_style = style;
    m.arguments = vec![single_input_arg("CreateOrderInput")];
    m
}

fn schema_with_order_input(fields: usize) -> CompiledSchema {
    CompiledSchema {
        input_types: vec![input_type("CreateOrderInput", fields)],
        ..Default::default()
    }
}

#[test]
fn jsonb_insert_single_input_no_false_arity_mismatch() {
    use fraiseql_core::schema::InputStyle;
    // input_style = jsonb + a single-jsonb function → the runtime sends one jsonb
    // arg, so the gate must NOT expect the 6 flattened fields. (Today: ArityMismatch{6}.)
    let m = order_mutation(
        MutationOperation::Insert {
            table: "tb_order".to_string(),
        },
        InputStyle::Jsonb,
    );
    let schema = schema_with_order_input(6);
    let candidates = vec![pg_function(
        &["jsonb"],
        &[Some("p_input")],
        full_response_columns(),
    )];
    assert_eq!(check(&m, &schema, &candidates), vec![], "single-jsonb insert must be clean");
}

#[test]
fn jsonb_insert_with_inject_param_no_violations() {
    use fraiseql_core::schema::InputStyle;
    let mut m = order_mutation(
        MutationOperation::Insert {
            table: "tb_order".to_string(),
        },
        InputStyle::Jsonb,
    );
    m.inject_params = inject(&["tenant_id"]);
    let schema = schema_with_order_input(6);
    let candidates = vec![pg_function(
        &["jsonb", "uuid"],
        &[Some("p_input"), Some("tenant_id")],
        full_response_columns(),
    )];
    assert_eq!(check(&m, &schema, &candidates), vec![], "jsonb + inject must be clean");
}

#[test]
fn jsonb_insert_against_flattened_function_is_arity_mismatch() {
    use fraiseql_core::schema::InputStyle;
    // input_style = jsonb but the function actually flattens 6 args → genuinely
    // wrong: the runtime sends 1 jsonb, the function wants 6. Still caught.
    let m = order_mutation(
        MutationOperation::Insert {
            table: "tb_order".to_string(),
        },
        InputStyle::Jsonb,
    );
    let schema = schema_with_order_input(6);
    let candidates = vec![pg_function(
        &["int4", "int4", "int4", "int4", "int4", "int4"],
        &[None, None, None, None, None, None],
        full_response_columns(),
    )];
    assert_eq!(
        check(&m, &schema, &candidates),
        vec![ContractViolation::ArityMismatch {
            expected: 1,
            found:    vec![6],
        }],
    );
}

#[test]
fn flatten_insert_is_unchanged() {
    use fraiseql_core::schema::InputStyle;
    // Flatten style with a known 6-field input → still flattens to 6 args.
    let m = order_mutation(
        MutationOperation::Insert {
            table: "tb_order".to_string(),
        },
        InputStyle::Flatten,
    );
    let schema = schema_with_order_input(6);
    let candidates = vec![pg_function(
        &["text", "text", "text", "text", "text", "text"],
        &[None, None, None, None, None, None],
        full_response_columns(),
    )];
    assert_eq!(check(&m, &schema, &candidates), vec![], "flatten path unchanged");
}

#[test]
fn update_with_flatten_style_still_takes_jsonb_path() {
    use fraiseql_core::schema::InputStyle;
    // Update always uses the single-JSONB path regardless of input_style.
    let m = order_mutation(
        MutationOperation::Update {
            table: "tb_order".to_string(),
        },
        InputStyle::Flatten,
    );
    let schema = schema_with_order_input(6);
    let candidates = vec![pg_function(
        &["jsonb"],
        &[Some("p_input")],
        full_response_columns(),
    )];
    assert_eq!(check(&m, &schema, &candidates), vec![], "update→jsonb preserved");
}

#[test]
fn jsonb_payload_wrong_type_is_flagged() {
    use fraiseql_core::schema::InputStyle;
    let m = order_mutation(
        MutationOperation::Insert {
            table: "tb_order".to_string(),
        },
        InputStyle::Jsonb,
    );
    let schema = schema_with_order_input(6);
    // Right arity (1) but arg-1 is text, not jsonb → the new path still asserts jsonb.
    let candidates = vec![pg_function(
        &["text"],
        &[Some("p_input")],
        full_response_columns(),
    )];
    assert!(
        check(&m, &schema, &candidates).contains(&ContractViolation::PayloadNotJsonb {
            actual: "text".to_string(),
        }),
        "single-jsonb path must assert arg-1 is jsonb",
    );
}

#[test]
fn custom_op_jsonb_style_is_not_update_only() {
    use fraiseql_core::schema::{InputStyle, MutationOperation as Op};
    // The gap is NOT update-only: a Custom (non-DML) op with jsonb style also
    // takes the single-JSONB path. (Today: ArityMismatch{3}.)
    let mut m = MutationDefinition::new("archiveOrder", "ArchiveOrderResult");
    m.sql_source = Some("fn_archive_order".to_string());
    m.operation = Op::Custom;
    m.input_style = InputStyle::Jsonb;
    m.arguments = vec![single_input_arg("ArchiveOrderInput")];
    let schema = CompiledSchema {
        input_types: vec![input_type("ArchiveOrderInput", 3)],
        ..Default::default()
    };
    let candidates = vec![pg_function(
        &["jsonb"],
        &[Some("p_input")],
        full_response_columns(),
    )];
    assert_eq!(check(&m, &schema, &candidates), vec![], "custom + jsonb must be clean");
}

#[test]
fn two_scalar_args_with_jsonb_style_stay_flat() {
    use fraiseql_core::schema::InputStyle;
    // No single structured `input` → input_arg_is_structured is false, so the
    // jsonb arm must NOT fire even with input_style = jsonb. Stays FlatArgs(2).
    let mut m = MutationDefinition::new("createOrder", "CreateOrderResult");
    m.sql_source = Some("fn_create_order".to_string());
    m.operation = MutationOperation::Insert {
        table: "tb_order".to_string(),
    };
    m.input_style = InputStyle::Jsonb;
    m.arguments = vec![
        ArgumentDefinition::new("a", FieldType::Int),
        ArgumentDefinition::new("b", FieldType::Int),
    ];
    let schema = CompiledSchema::default();
    let candidates = vec![pg_function(
        &["int4", "int4"],
        &[None, None],
        full_response_columns(),
    )];
    assert_eq!(check(&m, &schema, &candidates), vec![], "two scalar args stay FlatArgs(2)");
}

#[test]
fn preserve_naming_jsonb_single_input_holds() {
    use fraiseql_core::schema::{InputStyle, NamingConvention};
    let m = order_mutation(
        MutationOperation::Insert {
            table: "tb_order".to_string(),
        },
        InputStyle::Jsonb,
    );
    let schema = CompiledSchema {
        input_types: vec![input_type("CreateOrderInput", 6)],
        naming_convention: NamingConvention::Preserve,
        ..Default::default()
    };
    let candidates = vec![pg_function(
        &["jsonb"],
        &[Some("p_input")],
        full_response_columns(),
    )];
    assert_eq!(check(&m, &schema, &candidates), vec![], "arg-1-is-jsonb holds under Preserve");
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

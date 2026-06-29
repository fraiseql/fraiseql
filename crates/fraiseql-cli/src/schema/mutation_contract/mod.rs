//! Static server↔database mutation-contract validation (#397).
//!
//! For each DB-backed mutation in a compiled schema, this checks that the
//! PostgreSQL function the server *will* call matches what the server *will*
//! send and decode — without booting a server or invoking any mutation:
//!
//! - **Call binding** — `sql_source` resolves to exactly one function whose *input* arity equals
//!   what the runtime sends (the positional args plus trailing injected params), the jsonb payload
//!   parameter (update path) is actually `jsonb`, and the trailing parameter names match the inject
//!   keys.
//! - **Response shape** — the function's result row carries `succeeded` and `state_changed` (both
//!   `boolean`, required by the `MutationResponse` decoder) and the optional columns it does
//!   declare have compatible types.
//!
//! The arity/shape logic ([`expected_call`]) mirrors the runtime arg-building in
//! `fraiseql-core`'s mutation runner exactly; [`check_mutation`] is a pure
//! comparison against catalog facts so it is unit-tested without a database.
//!
//! Out of scope (deliberate): the *behavioural* response invariants
//! (`succeeded ⇒ error_class IS NULL`, `http_status ∈ 100..=599`, …) are
//! properties of the function's runtime output, only observable by invoking it —
//! which would have database side effects. This check stays static and
//! read-only.

use std::fmt;

use anyhow::Result;
use fraiseql_core::schema::{
    CompiledSchema, FieldType, InputStyle, MutationDefinition, MutationOperation,
};

use crate::schema::pg_catalog::{PgCatalog, PgFunction};

#[cfg(test)]
mod tests;

/// How the runtime lays out the positional arguments for a mutation's SQL call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallShape {
    /// Update with a single `input` object → one `jsonb` payload argument.
    JsonbPayload,
    /// Insert/Delete/Custom with a single `input` object whose type is in the
    /// schema → one positional argument per input field.
    FlattenedFields,
    /// Flat arguments → one positional argument per declared mutation argument.
    FlatArgs,
}

/// What the runtime will send to a mutation's `sql_source`, derived purely from
/// the compiled schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedCall {
    /// Resolved function name (the `sql_source`, or the operation's table).
    pub sql_source:             String,
    /// Argument-layout shape (diagnostics only).
    pub shape:                  CallShape,
    /// Number of positional arguments before injected params.
    pub base_arity:             usize,
    /// Inject-param keys, in call order — appended after the base args.
    pub inject_names:           Vec<String>,
    /// Whether the first argument is the jsonb payload (update path).
    pub first_is_jsonb_payload: bool,
}

impl ExpectedCall {
    /// Total positional arity the server binds: base args plus inject params.
    #[must_use]
    pub fn total_arity(&self) -> usize {
        self.base_arity + self.inject_names.len()
    }
}

/// Severity of a [`ContractViolation`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Breaks the contract — the server would fail at runtime. Fails the check.
    Error,
    /// Likely a bug, but not a guaranteed failure. Does not fail the check.
    Warn,
}

/// A single mismatch between a mutation's compiled contract and the live
/// database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractViolation {
    /// No function of that name is visible on the search path.
    MissingFunction,
    /// Function(s) exist but none has the expected input arity.
    ArityMismatch {
        /// Arity the server will send.
        expected: usize,
        /// Distinct input arities of the existing overloads.
        found:    Vec<usize>,
    },
    /// Multiple overloads share the expected arity — the untyped positional call
    /// is ambiguous (`function is not unique`).
    AmbiguousFunction {
        /// The shared arity.
        arity: usize,
        /// How many overloads match it.
        count: usize,
    },
    /// The update path sends a jsonb payload but the first parameter is not jsonb.
    PayloadNotJsonb {
        /// The actual first-parameter type.
        actual: String,
    },
    /// A trailing parameter name does not match the inject key bound to it.
    InjectNameMismatch {
        /// Inject-key position (0-based, among inject params).
        position: usize,
        /// Inject key the server binds here.
        expected: String,
        /// The function's actual parameter name at that position.
        actual:   String,
    },
    /// A required response column (`succeeded` / `state_changed`) is absent.
    MissingRequiredColumn {
        /// The missing column.
        column: &'static str,
    },
    /// A required response column has the wrong type (must be `boolean`).
    RequiredColumnWrongType {
        /// The column.
        column: &'static str,
        /// Its actual type.
        actual: String,
    },
    /// An optional response column is present but has an incompatible type.
    OptionalColumnWrongType {
        /// The column.
        column:   &'static str,
        /// The expected type family.
        expected: &'static str,
        /// Its actual type.
        actual:   String,
    },
    /// The function returns a scalar / bare `record` — its response shape cannot
    /// be introspected.
    ResponseShapeUnverifiable,
}

impl ContractViolation {
    /// Severity of this violation.
    #[must_use]
    pub const fn severity(&self) -> Severity {
        match self {
            Self::MissingFunction
            | Self::ArityMismatch { .. }
            | Self::AmbiguousFunction { .. }
            | Self::PayloadNotJsonb { .. }
            | Self::MissingRequiredColumn { .. }
            | Self::RequiredColumnWrongType { .. } => Severity::Error,
            Self::InjectNameMismatch { .. }
            | Self::OptionalColumnWrongType { .. }
            | Self::ResponseShapeUnverifiable => Severity::Warn,
        }
    }
}

impl fmt::Display for ContractViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingFunction => {
                write!(f, "no such function on the search path (function does not exist)")
            },
            Self::ArityMismatch { expected, found } => {
                write!(f, "expected {expected} argument(s) but the function takes {found:?}")
            },
            Self::AmbiguousFunction { arity, count } => write!(
                f,
                "{count} overloads take {arity} argument(s) — the call is ambiguous (function is not unique)"
            ),
            Self::PayloadNotJsonb { actual } => write!(
                f,
                "update payload argument must be jsonb but the first parameter is `{actual}`"
            ),
            Self::InjectNameMismatch {
                position,
                expected,
                actual,
            } => write!(
                f,
                "inject param #{position} is bound positionally to parameter `{actual}` but the inject key is `{expected}`"
            ),
            Self::MissingRequiredColumn { column } => write!(
                f,
                "response row is missing required column `{column}` (the server cannot decode MutationResponse)"
            ),
            Self::RequiredColumnWrongType { column, actual } => {
                write!(f, "response column `{column}` is `{actual}`, expected boolean")
            },
            Self::OptionalColumnWrongType {
                column,
                expected,
                actual,
            } => write!(f, "response column `{column}` is `{actual}`, expected {expected}"),
            Self::ResponseShapeUnverifiable => {
                write!(f, "function returns a scalar/record — response shape cannot be verified")
            },
        }
    }
}

/// Derive what the runtime will send for `mutation`.
///
/// Returns `None` when the mutation is not database-backed (no `sql_source` and
/// no operation table — e.g. a federation/non-SQL mutation) and should be
/// skipped.
///
/// This mirrors the runtime arg-building in
/// `fraiseql-core/.../runners/mutation/mod.rs` exactly. A single structured
/// `input` arg is forwarded as ONE jsonb payload when the operation is `Update`,
/// the mutation opts in via `input_style = jsonb`, **or** the input type is not
/// in the schema (see `pass_as_single_jsonb`, pinned to `mutation/mod.rs:499-500`).
/// Otherwise a known input type flattens to one positional arg per field;
/// everything else is flat args.
#[must_use]
pub fn expected_call(
    mutation: &MutationDefinition,
    schema: &CompiledSchema,
) -> Option<ExpectedCall> {
    let sql_source = resolve_sql_source(mutation)?;

    let input_type_name = single_input_type_name(mutation);

    let (shape, base_arity, first_is_jsonb_payload) = if pass_as_single_jsonb(mutation, schema) {
        // Single-JSONB path: Update, `input_style = jsonb`, or unknown input type
        // → one jsonb payload arg. `first_is_jsonb_payload` enables the arg-1-is-
        // jsonb assertion in `check_mutation` for every such case.
        (CallShape::JsonbPayload, 1, true)
    } else if let Some(input_type) = input_type_name.and_then(|n| schema.find_input_type(n)) {
        // Insert/Delete/Custom + single input object found in schema → flatten fields.
        (CallShape::FlattenedFields, input_type.fields.len(), false)
    } else {
        // Flat args (a single non-Input `input`, or multiple scalar args).
        (CallShape::FlatArgs, mutation.arguments.len(), false)
    };

    Some(ExpectedCall {
        sql_source,
        shape,
        base_arity,
        inject_names: mutation.inject_params.keys().cloned().collect(),
        first_is_jsonb_payload,
    })
}

/// The name of a single `input` argument typed as an Input object, else `None`.
///
/// This is the structured-input form the compiled `input` arg carries
/// (`FieldType::Input`); it mirrors the runtime's `input_type_name`
/// (`mutation/mod.rs:444-456`) scoped to that form.
fn single_input_type_name(mutation: &MutationDefinition) -> Option<&str> {
    if mutation.arguments.len() == 1 && mutation.arguments[0].name == "input" {
        match &mutation.arguments[0].arg_type {
            FieldType::Input(name) => Some(name.as_str()),
            _ => None,
        }
    } else {
        None
    }
}

/// Faithful mirror of the runtime single-JSONB predicate
/// (`fraiseql-core/.../runners/mutation/mod.rs:499-500`):
/// ```text
/// pass_input_as_single_jsonb =
///     input_arg_is_structured && (is_update || jsonb_input_style || !known_input_type)
/// ```
/// A structured single `input` arg is forwarded as ONE jsonb payload when the
/// operation is `Update`, the mutation opts in via `input_style = jsonb`, or the
/// input type is not in the compiled schema. Keep this in sync with that line.
fn pass_as_single_jsonb(mutation: &MutationDefinition, schema: &CompiledSchema) -> bool {
    let input_type_name = single_input_type_name(mutation);
    let input_arg_is_structured = input_type_name.is_some();
    let is_update = matches!(&mutation.operation, MutationOperation::Update { .. });
    let jsonb_input_style = matches!(mutation.input_style, InputStyle::Jsonb);
    let known_input_type = input_type_name.and_then(|n| schema.find_input_type(n)).is_some();
    input_arg_is_structured && (is_update || jsonb_input_style || !known_input_type)
}

/// Resolve a mutation's SQL function name: `sql_source`, else the operation's
/// non-empty table, else `None` (not DB-backed).
fn resolve_sql_source(mutation: &MutationDefinition) -> Option<String> {
    if let Some(src) = &mutation.sql_source {
        return Some(src.clone());
    }
    match &mutation.operation {
        MutationOperation::Insert { table }
        | MutationOperation::Update { table }
        | MutationOperation::Delete { table }
            if !table.is_empty() =>
        {
            Some(table.clone())
        },
        _ => None,
    }
}

/// Compare an [`ExpectedCall`] against the candidate functions resolved from the
/// database. Pure — no I/O.
#[must_use]
pub fn check_mutation(
    expected: &ExpectedCall,
    candidates: &[PgFunction],
) -> Vec<ContractViolation> {
    let mut violations = Vec::new();

    if candidates.is_empty() {
        violations.push(ContractViolation::MissingFunction);
        return violations;
    }

    let want = expected.total_arity();
    let matched: Vec<&PgFunction> =
        candidates.iter().filter(|f| f.in_types.len() == want).collect();

    let func = match matched.as_slice() {
        [] => {
            let mut found: Vec<usize> = candidates.iter().map(|f| f.in_types.len()).collect();
            found.sort_unstable();
            found.dedup();
            violations.push(ContractViolation::ArityMismatch {
                expected: want,
                found,
            });
            return violations;
        },
        [one] => *one,
        many => {
            violations.push(ContractViolation::AmbiguousFunction {
                arity: want,
                count: many.len(),
            });
            return violations;
        },
    };

    // Call binding: the update path's first parameter must be jsonb.
    if expected.first_is_jsonb_payload {
        if let Some(first) = func.in_types.first() {
            if !is_jsonb(first) {
                violations.push(ContractViolation::PayloadNotJsonb {
                    actual: first.clone(),
                });
            }
        }
    }

    // Call binding: trailing parameter names should match the inject keys, in
    // order. Advisory — the runtime binds positionally — and only checkable when
    // the function declares parameter names.
    if !expected.inject_names.is_empty() {
        let start = func.in_types.len().saturating_sub(expected.inject_names.len());
        for (position, want_name) in expected.inject_names.iter().enumerate() {
            if let Some(Some(actual)) = func.in_names.get(start + position) {
                if actual != want_name {
                    violations.push(ContractViolation::InjectNameMismatch {
                        position,
                        expected: want_name.clone(),
                        actual: actual.clone(),
                    });
                }
            }
        }
    }

    check_response_shape(func, &mut violations);
    violations
}

/// Optional response columns and their expected type family.
const OPTIONAL_COLUMNS: &[(&str, &str)] = &[
    ("error_class", "text or enum"),
    ("status_detail", "text"),
    ("http_status", "an integer type"),
    ("message", "text"),
    ("entity_id", "uuid"),
    ("entity_type", "text"),
    ("entity", "jsonb"),
    ("updated_fields", "a text array"),
    ("cascade", "jsonb"),
    ("error_detail", "jsonb"),
    ("metadata", "jsonb"),
];

/// Validate the function's result row against the `MutationResponse` decoder:
/// `succeeded` + `state_changed` are required booleans; present optional columns
/// must have compatible types.
fn check_response_shape(func: &PgFunction, violations: &mut Vec<ContractViolation>) {
    if func.out_columns.is_empty() {
        violations.push(ContractViolation::ResponseShapeUnverifiable);
        return;
    }
    let find = |name: &str| func.out_columns.iter().find(|c| c.name == name);

    for column in ["succeeded", "state_changed"] {
        match find(column) {
            None => violations.push(ContractViolation::MissingRequiredColumn { column }),
            Some(c) if !is_bool(&c.type_name) => {
                violations.push(ContractViolation::RequiredColumnWrongType {
                    column,
                    actual: c.type_name.clone(),
                });
            },
            Some(_) => {},
        }
    }

    for &(column, expected) in OPTIONAL_COLUMNS {
        if let Some(c) = find(column) {
            if !optional_column_ok(column, c.is_enum, &c.type_name) {
                violations.push(ContractViolation::OptionalColumnWrongType {
                    column,
                    expected,
                    actual: c.type_name.clone(),
                });
            }
        }
    }
}

/// Whether an optional response column's type is compatible with the decoder.
fn optional_column_ok(column: &str, is_enum: bool, type_name: &str) -> bool {
    match column {
        // `error_class` decodes from text or a project enum.
        "error_class" => is_enum || is_text(type_name),
        "status_detail" | "message" | "entity_type" => is_text(type_name),
        "http_status" => is_int(type_name),
        "entity_id" => type_name == "uuid",
        "updated_fields" => type_name.ends_with("[]"),
        "entity" | "cascade" | "error_detail" | "metadata" => is_jsonb(type_name),
        _ => true,
    }
}

fn is_bool(type_name: &str) -> bool {
    type_name == "boolean"
}

fn is_jsonb(type_name: &str) -> bool {
    type_name == "jsonb" || type_name == "json"
}

fn is_int(type_name: &str) -> bool {
    matches!(type_name, "smallint" | "integer" | "bigint")
}

fn is_text(type_name: &str) -> bool {
    matches!(type_name, "text" | "varchar" | "name" | "bpchar" | "citext")
        || type_name.starts_with("character varying")
        || type_name.starts_with("character(")
        || type_name == "character"
}

// ─── Report ─────────────────────────────────────────────────────────────────

/// Per-mutation contract findings.
#[derive(Debug, Clone)]
pub struct MutationReport {
    /// GraphQL mutation name.
    pub mutation:   String,
    /// Resolved `sql_source` checked.
    pub sql_source: String,
    /// Violations found (empty entries are not stored in the report).
    pub violations: Vec<ContractViolation>,
}

/// Aggregate result of validating every mutation's contract.
#[derive(Debug, Clone, Default)]
pub struct ContractReport {
    /// DB-backed mutations checked.
    pub checked:   usize,
    /// Non-DB-backed mutations skipped.
    pub skipped:   usize,
    /// Mutations with at least one violation.
    pub mutations: Vec<MutationReport>,
}

impl ContractReport {
    /// Total error-severity violations across all mutations.
    #[must_use]
    pub fn error_count(&self) -> usize {
        self.count(Severity::Error)
    }

    /// Total warning-severity violations across all mutations.
    #[must_use]
    pub fn warn_count(&self) -> usize {
        self.count(Severity::Warn)
    }

    fn count(&self, severity: Severity) -> usize {
        self.mutations
            .iter()
            .flat_map(|m| &m.violations)
            .filter(|v| v.severity() == severity)
            .count()
    }

    /// Render the report as machine-readable JSON.
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        let mutations: Vec<serde_json::Value> = self
            .mutations
            .iter()
            .map(|m| {
                let findings: Vec<serde_json::Value> = m
                    .violations
                    .iter()
                    .map(|v| {
                        serde_json::json!({
                            "severity": match v.severity() {
                                Severity::Error => "error",
                                Severity::Warn => "warning",
                            },
                            "message": v.to_string(),
                        })
                    })
                    .collect();
                serde_json::json!({
                    "mutation": m.mutation,
                    "sqlSource": m.sql_source,
                    "findings": findings,
                })
            })
            .collect();
        serde_json::json!({
            "checked": self.checked,
            "skipped": self.skipped,
            "errors": self.error_count(),
            "warnings": self.warn_count(),
            "mutations": mutations,
        })
    }

    /// Print the report in human-readable form to stdout.
    pub fn print_text(&self) {
        println!("\nChecking mutation contract against the database...\n");
        if self.mutations.is_empty() {
            println!(
                "  All {} DB-backed mutation(s) match the database contract ({} skipped).",
                self.checked, self.skipped
            );
            return;
        }
        for m in &self.mutations {
            println!("  {} (sql_source: {})", m.mutation, m.sql_source);
            for v in &m.violations {
                let symbol = match v.severity() {
                    Severity::Error => "✗",
                    Severity::Warn => "!",
                };
                println!("    [{symbol}] {v}");
            }
        }
        println!(
            "\nSummary: {} error(s), {} warning(s) across {} checked mutation(s) ({} skipped).",
            self.error_count(),
            self.warn_count(),
            self.checked,
            self.skipped,
        );
    }
}

/// Validate every DB-backed mutation's contract in `schema` against `catalog`.
///
/// # Errors
///
/// Returns an error if any catalog query fails.
pub async fn validate_mutation_contract(
    schema: &CompiledSchema,
    catalog: &PgCatalog,
) -> Result<ContractReport> {
    let mut report = ContractReport::default();
    for mutation in &schema.mutations {
        let Some(expected) = expected_call(mutation, schema) else {
            report.skipped += 1;
            continue;
        };
        report.checked += 1;
        let candidates = catalog.resolve_functions(&expected.sql_source).await?;
        let violations = check_mutation(&expected, &candidates);
        if !violations.is_empty() {
            report.mutations.push(MutationReport {
                mutation: mutation.name.clone(),
                sql_source: expected.sql_source.clone(),
                violations,
            });
        }
    }
    Ok(report)
}

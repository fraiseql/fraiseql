//! Static server↔database mutation-contract validation (#397).
//!
//! For each DB-backed mutation in a compiled schema, this checks that the
//! `PostgreSQL` function the server *will* call matches what the server *will*
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
//! - **Literal stamps** — a literal `entity_type` the body stamps (`fraiseql.mutation_ok(…,
//!   p_entity_type => 'T')`, `fraiseql.mutation_err(…, 'T')`, `result.entity_type := 'T'`) must be
//!   a type the mutation can return on that outcome ([`StampContract`], the runtime's own
//!   derivation): the server refuses any other as a contract error (rulings AA 1, AG 3, AJ 2).
//!
//! Out of scope (deliberate): the *behavioural* response invariants
//! (`succeeded ⇒ error_class IS NULL`, `http_status ∈ 100..=599`, …) are
//! properties of the function's runtime output, only observable by invoking it —
//! which would have database side effects. This check stays static and
//! read-only.

use std::{fmt, sync::LazyLock};

use anyhow::Result;
use fraiseql_core::{
    runtime::StampContract,
    schema::{CompiledSchema, FieldType, InputStyle, MutationDefinition, MutationOperation},
};
use regex::Regex;

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
    /// Canonical (stored) key names the runtime writes into the single-JSONB
    /// payload — the declared input type's field names. Only populated on the
    /// single-JSONB path with a known input type; drives the payload-key
    /// consumption scan (#384 category 2).
    pub payload_keys:           Vec<String>,
    /// What the function may stamp in `entity_type`, per outcome — the runtime's own
    /// derivation (ruling AJ 1), read by the literal-stamp lint. `None` skips the lint.
    pub stamps:                 Option<StampContract>,
    /// Whether the mutation declares success fields (#1397): its response row must then
    /// carry them in a `result jsonb` column. Optional for every other mutation.
    pub requires_result:        bool,
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
    /// The mutation declares success fields (#1397) but its response row has no `result`
    /// column, the one they are read from.
    MissingResultColumn,
    /// The mutation declares success fields but its response row's `result` column is not
    /// `jsonb`.
    ResultColumnWrongType {
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
    /// A declared input field's payload key is never referenced in the function
    /// body, while the body does extract *other* keys — the classic
    /// silently-dropped input (#384 category 2). Text-scan based, so advisory.
    PayloadKeyUnreferenced {
        /// The declared input field.
        field:     String,
        /// The canonical (stored) key probed for, when it differs from `field`.
        snake_key: String,
    },
    /// The function returns a scalar / bare `record` — its response shape cannot
    /// be introspected.
    ResponseShapeUnverifiable,
    /// The body stamps a literal `entity_type` this mutation cannot return on that outcome
    /// (rulings AA 1, AG 3, AJ 2): the server refuses such a write as a contract error and
    /// rolls it back.
    OffContractStamp {
        /// The outcome the stamp is written on.
        arm:     StampArm,
        /// The literal stamp.
        stamp:   String,
        /// The types that outcome can be stamped with.
        allowed: Vec<String>,
    },
}

/// The outcome a literal stamp is written on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StampArm {
    /// `mutation_ok(…)`: a success.
    Success,
    /// `mutation_err(…)`: a failure.
    Error,
    /// An assignment to `entity_type`: either outcome.
    Either,
}

impl fmt::Display for StampArm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Success => "a success",
            Self::Error => "a failure",
            Self::Either => "the result",
        })
    }
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
            | Self::RequiredColumnWrongType { .. }
            | Self::MissingResultColumn
            | Self::ResultColumnWrongType { .. }
            | Self::OffContractStamp { .. } => Severity::Error,
            Self::InjectNameMismatch { .. }
            | Self::OptionalColumnWrongType { .. }
            | Self::PayloadKeyUnreferenced { .. }
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
            Self::PayloadKeyUnreferenced { field, snake_key } => {
                let probed = if field == snake_key {
                    format!("'{field}'")
                } else {
                    format!("'{field}' / '{snake_key}'")
                };
                write!(
                    f,
                    "input field `{field}`: the function body extracts other payload keys but \
                     never references {probed} — the value would be silently dropped (text scan)"
                )
            },
            Self::MissingResultColumn => write!(
                f,
                "the mutation declares success fields, but its response row has no `result \
                 jsonb` column to carry them: declare `result jsonb` after the 13 \
                 mutation_response columns in this function's own row type, and build the row \
                 with `fraiseql.mutation_ok_result(…)` / `mutation_err_result(…)` (helpers \
                 2.4.0, `fraiseql setup`)"
            ),
            Self::ResultColumnWrongType { actual } => write!(
                f,
                "the mutation declares success fields, so its response column `result` must \
                 be jsonb, but is `{actual}`"
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
            Self::OffContractStamp {
                arm,
                stamp,
                allowed,
            } => write!(
                f,
                "the function stamps entity_type '{stamp}' on {arm}, which this mutation cannot \
                 return (it can return: {}) — the server refuses such a write as a contract \
                 error and rolls it back",
                if allowed.is_empty() {
                    "nothing".to_string()
                } else {
                    allowed.join(", ")
                }
            ),
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

    // Payload keys for the consumption scan (#384 category 2): only on the
    // single-JSONB path with a *known* input type — those field names are the
    // canonical keys the runtime writes into the payload.
    let payload_keys = if matches!(shape, CallShape::JsonbPayload) {
        input_type_name
            .and_then(|n| schema.find_input_type(n))
            .map(|t| t.fields.iter().map(|f| f.name.clone()).collect())
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    Some(ExpectedCall {
        sql_source,
        shape,
        base_arity,
        inject_names: mutation.inject_params.keys().cloned().collect(),
        first_is_jsonb_payload,
        payload_keys,
        stamps: Some(StampContract::of(schema, mutation)),
        requires_result: !mutation.success_fields.is_empty(),
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

    // Payload-key consumption (#384 category 2): a declared input field whose
    // key the function body never reads is silently dropped at runtime.
    check_payload_key_consumption(expected, func, &mut violations);

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

    check_response_shape(expected, func, &mut violations);
    check_literal_stamps(expected, func, &mut violations);
    violations
}

/// Judge every literal `entity_type` stamp in the function body against the mutation's
/// [`StampContract`] (ruling AJ 2), emitting [`ContractViolation::OffContractStamp`] for one
/// outside its outcome's set.
///
/// A text scan, conservative as the payload-key scan is: `plpgsql` / `sql` bodies only;
/// comments are stripped and nothing inside a string literal is read as code; only a
/// literal is judged (`NULL`, a variable or an expression is left to the runtime, which
/// refuses an off-contract stamp whatever produced it); `=` is never read as an assignment,
/// since it is also a comparison. Recognised: the stamp argument of `mutation_ok` (3rd) and
/// `mutation_err` (5th), bare or `fraiseql.`-qualified, by name (`p_entity_type => …` or
/// `:=`) or position; and `<var>.entity_type := '…'`.
fn check_literal_stamps(
    expected: &ExpectedCall,
    func: &PgFunction,
    violations: &mut Vec<ContractViolation>,
) {
    let Some(contract) = &expected.stamps else {
        return;
    };
    if !matches!(func.language.as_str(), "plpgsql" | "sql") {
        return;
    }
    let mut seen = Vec::new();
    for (arm, stamp) in literal_stamps(&func.source) {
        let allowed: Vec<String> = match arm {
            StampArm::Success => contract.success.clone(),
            StampArm::Error => contract.error.clone(),
            StampArm::Either => {
                let mut both = contract.success.clone();
                both.extend(
                    contract.error.iter().filter(|t| !contract.success.contains(t)).cloned(),
                );
                both
            },
        };
        if allowed.contains(&stamp) || seen.contains(&(arm, stamp.clone())) {
            continue;
        }
        seen.push((arm, stamp.clone()));
        violations.push(ContractViolation::OffContractStamp {
            arm,
            stamp,
            allowed,
        });
    }
}

/// A helper call whose stamp argument is read: `(name, stamp position, outcome)`.
const STAMPING_HELPERS: &[(&str, usize, StampArm)] = &[
    ("mutation_ok", 2, StampArm::Success),
    ("mutation_err", 4, StampArm::Error),
];

/// A helper call's name, bare or `fraiseql.`-qualified, followed by its opening parenthesis.
static HELPER_CALL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:\bfraiseql\s*\.\s*|[^A-Za-z0-9_.$]|^)(mutation_ok|mutation_err)\s*\(")
        .expect("helper call regex is valid")
});

/// An assignment of a literal to a record's `entity_type` (`:=` only).
static STAMP_ASSIGNMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b[A-Za-z_][A-Za-z0-9_$]*\s*\.\s*entity_type\s*:=\s*('(?:[^']|'')*')")
        .expect("stamp assignment regex is valid")
});

/// The literal stamps in `source`, each with the outcome it is written on.
fn literal_stamps(source: &str) -> Vec<(StampArm, String)> {
    let (code, in_string) = mask_comments_and_strings(source);
    let mut stamps = Vec::new();
    for call in HELPER_CALL.captures_iter(&code) {
        let (Some(name), Some(whole)) = (call.get(1), call.get(0)) else {
            continue;
        };
        if in_string[name.start()] {
            continue;
        }
        let Some(&(_, position, arm)) =
            STAMPING_HELPERS.iter().find(|(h, ..)| h.eq_ignore_ascii_case(name.as_str()))
        else {
            continue;
        };
        let args = call_arguments(&code, &in_string, whole.end());
        let stamp_arg =
            args.iter().enumerate().find_map(|(index, arg)| match named_argument(arg) {
                Some((param, value)) => {
                    param.eq_ignore_ascii_case("p_entity_type").then_some(value)
                },
                None => (index == position).then_some(arg.as_str()),
            });
        if let Some(stamp) = stamp_arg.and_then(string_literal) {
            stamps.push((arm, stamp));
        }
    }
    for assignment in STAMP_ASSIGNMENT.captures_iter(&code) {
        let Some(literal) = assignment.get(1) else {
            continue;
        };
        let start = assignment.get(0).map_or(literal.start(), |m| m.start());
        if in_string[start] {
            continue;
        }
        if let Some(stamp) = string_literal(literal.as_str()) {
            stamps.push((StampArm::Either, stamp));
        }
    }
    stamps
}

/// `source` with every comment blanked out (same length, so offsets stay valid), and for
/// each byte whether it lies inside a string literal (`'…'` with `''` escapes, or a
/// dollar-quoted `$tag$…$tag$`). String contents are kept: the stamps are literals.
fn mask_comments_and_strings(source: &str) -> (String, Vec<bool>) {
    let bytes = source.as_bytes();
    let mut code = bytes.to_vec();
    let mut in_string = vec![false; bytes.len()];
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'-' if bytes.get(i + 1) == Some(&b'-') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    code[i] = b' ';
                    i += 1;
                }
            },
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                let mut depth = 0usize;
                while i < bytes.len() {
                    if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
                        depth += 1;
                        code[i] = b' ';
                        code[i + 1] = b' ';
                        i += 2;
                    } else if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/') {
                        depth -= 1;
                        code[i] = b' ';
                        code[i + 1] = b' ';
                        i += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        if bytes[i] != b'\n' {
                            code[i] = b' ';
                        }
                        i += 1;
                    }
                }
            },
            b'\'' => {
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\'' {
                        if bytes.get(i + 1) == Some(&b'\'') {
                            in_string[i] = true;
                            in_string[i + 1] = true;
                            i += 2;
                            continue;
                        }
                        i += 1;
                        break;
                    }
                    in_string[i] = true;
                    i += 1;
                }
            },
            b'$' => {
                let tag_end = bytes[i + 1..]
                    .iter()
                    .position(|b| !(b.is_ascii_alphanumeric() || *b == b'_'))
                    .map(|n| i + 1 + n);
                match tag_end {
                    Some(end) if bytes[end] == b'$' => {
                        let tag = &bytes[i..=end];
                        let body_start = end + 1;
                        let close = bytes[body_start..]
                            .windows(tag.len())
                            .position(|w| w == tag)
                            .map_or(bytes.len(), |n| body_start + n);
                        for flag in &mut in_string[body_start..close] {
                            *flag = true;
                        }
                        i = (close + tag.len()).min(bytes.len());
                    },
                    _ => i += 1,
                }
            },
            _ => i += 1,
        }
    }
    // Only ASCII bytes were replaced (by ASCII spaces), so the text is still UTF-8.
    (String::from_utf8(code).unwrap_or_else(|_| source.to_string()), in_string)
}

/// The top-level arguments of the call whose `(` ends at `open_end` in `code` (comments
/// already blanked), trimmed; commas inside parentheses, brackets and string literals do not
/// split.
fn call_arguments(code: &str, in_string: &[bool], open_end: usize) -> Vec<String> {
    let bytes = code.as_bytes();
    let mut args = Vec::new();
    let mut depth = 0usize;
    let mut start = open_end;
    let mut i = open_end;
    while i < bytes.len() {
        if !in_string[i] {
            match bytes[i] {
                b'(' | b'[' => depth += 1,
                b')' | b']' if depth > 0 => depth -= 1,
                b')' => {
                    args.push(code[start..i].trim().to_string());
                    break;
                },
                b',' if depth == 0 => {
                    args.push(code[start..i].trim().to_string());
                    start = i + 1;
                },
                _ => {},
            }
        }
        i += 1;
    }
    args
}

/// `param => value` or `param := value` → `(param, value)`.
fn named_argument(arg: &str) -> Option<(&str, &str)> {
    let at = arg.find("=>").or_else(|| arg.find(":="))?;
    let param = arg[..at].trim();
    let is_identifier = !param.is_empty()
        && param.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'$');
    is_identifier.then(|| (param, arg[at + 2..].trim()))
}

/// The value of a plain string literal (`'T'`, `''` escapes, optionally `::text` /
/// `::varchar`), else `None`.
fn string_literal(expr: &str) -> Option<String> {
    let rest = expr.trim().strip_prefix('\'')?;
    let mut value = String::new();
    let mut chars = rest.char_indices();
    let close = loop {
        let (at, c) = chars.next()?;
        if c != '\'' {
            value.push(c);
        } else if rest[at + 1..].starts_with('\'') {
            value.push('\'');
            chars.next();
        } else {
            break at;
        }
    };
    let tail = rest[close + 1..].trim().to_ascii_lowercase();
    let plain = tail.is_empty()
        || tail.strip_prefix("::").is_some_and(|t| matches!(t.trim(), "text" | "varchar"));
    plain.then_some(value)
}

/// Scan the function body for each declared payload key (#384 category 2).
///
/// Emits [`ContractViolation::PayloadKeyUnreferenced`] (warn-grade) for every
/// declared input field whose key — probed in both its declared and snake_case
/// form, single-quoted — never appears in the body. Deliberately conservative
/// about when it runs at all, because it is a text scan, not a parse:
///
/// - only `plpgsql` / `sql` bodies are scannable source text (for other languages `prosrc` is a
///   symbol name);
/// - a body containing a whole-payload consumer (`jsonb_populate_record` and kin) reads every key
///   without naming any — skip;
/// - a body with **no** jsonb extraction operator (`->`, `->>`, `#>`, `?`) never reads keys inline
///   (it forwards the payload to a helper) — skip. This also means the scan only fires when the
///   body demonstrably extracts *some* keys but not the declared one, which is exactly the
///   silently-dropped-input shape.
fn check_payload_key_consumption(
    expected: &ExpectedCall,
    func: &PgFunction,
    violations: &mut Vec<ContractViolation>,
) {
    const WHOLE_PAYLOAD_MARKERS: &[&str] = &[
        "jsonb_populate_record",
        "json_populate_record",
        "jsonb_to_record",
        "jsonb_each",
        "jsonb_object_keys",
    ];
    const EXTRACTION_OPS: &[&str] = &["->", "#>", "?"];

    if !expected.first_is_jsonb_payload || expected.payload_keys.is_empty() {
        return;
    }
    if !matches!(func.language.as_str(), "plpgsql" | "sql") {
        return;
    }
    if WHOLE_PAYLOAD_MARKERS.iter().any(|m| func.source.contains(m)) {
        return;
    }
    if !EXTRACTION_OPS.iter().any(|op| func.source.contains(op)) {
        return;
    }

    for key in &expected.payload_keys {
        let snake_key = fraiseql_core::utils::to_snake_case(key);
        let quoted = format!("'{key}'");
        let quoted_snake = format!("'{snake_key}'");
        if !func.source.contains(&quoted) && !func.source.contains(&quoted_snake) {
            violations.push(ContractViolation::PayloadKeyUnreferenced {
                field: key.clone(),
                snake_key,
            });
        }
    }
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
fn check_response_shape(
    expected: &ExpectedCall,
    func: &PgFunction,
    violations: &mut Vec<ContractViolation>,
) {
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

    // #1397: `result` carries declared success fields; a mutation without any never reads it.
    if expected.requires_result {
        match find("result") {
            None => violations.push(ContractViolation::MissingResultColumn),
            Some(c) if !is_jsonb(&c.type_name) => {
                violations.push(ContractViolation::ResultColumnWrongType {
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

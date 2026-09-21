//! Argument-**value** validation for a root field — GraphQL § 5.6.1 (*Values of
//! Correct Type*), § 5.8.5 (*All Variable Usages Are Allowed*) and § 6.1.2
//! (*`CoerceVariableValues`*).
//!
//! Its sibling next door ([`super::argument_validation`]) checks argument
//! *names*; this one checks that the value written against a name has that
//! name's type. Before it, nothing did — on the literal path and the variable
//! path alike — and the consequence was not a lenient server but a **wrong
//! answer**:
//!
//! ```graphql
//! { products(limit: "2") { sku } }     # 12 rows, exit 0, no `errors` array
//! ```
//!
//! `limit` and `offset` are read out of the merged argument map with
//! `as_u64()`. A `String`, a `Bool`, a `Float`, a negative number or one past
//! `u32` all answer `None` there, and `None` means *no clause was emitted* — so
//! a request that explicitly asked to be bounded came back unbounded (#1197).
//! Every other argument instead reached PostgreSQL, which answered with a
//! `Database` error whose message can carry a **stored value** back to an
//! unauthenticated caller.
//!
//! The read sites fail closed on their own now
//! ([`crate::runtime::coerce_pagination_arg`]); this
//! module is what turns "the engine refused to paginate" into a diagnostic that
//! names the argument, the type it declares, and the type the document wrote.
//!
//! # What this adjudicates, and what it deliberately does not
//!
//! Following [`crate::graphql::validate_selection_set`] and § 5.4.1's module:
//! **reject what the document positively contradicts, pass everything it cannot
//! adjudicate.** A mismatch is reported only when *both* sides are one of the
//! built-in scalars in [`Scalar`] — the ten whose value space is a property of
//! the spec rather than of a project's own scalar wiring.
//!
//! Outside that set, execution is unchanged:
//!
//! * **Custom scalars, input objects, lists and vectors.** A project may back any of these with any
//!   JSON shape, so a disagreement here is not evidence of a client mistake.
//! * **Nested input-object fields**, for the scalar check above. Only the value written *at* the
//!   argument is adjudicated as a scalar, not the keys inside a `where:` predicate. Those have
//!   their own surface and their own operators.
//!
//! # Enums are the exception, and were wrongly inside the exclusion (#1362)
//!
//! Enums sat in that first bullet until #1362, and the bullet's own rationale is
//! what makes it wrong for them. A custom scalar's value space *is* a project's
//! choice — `Email` can be backed by anything the project's SQL accepts. An
//! enum's is not: [`EnumDefinition::values`] enumerates it, exhaustively, in the
//! compiled schema, and introspection publishes the same list. So a value that
//! is not one of those members is not an undecidable disagreement; it is
//! positively contradicted by the schema, which is exactly the standard the
//! paragraph above sets.
//!
//! Nothing checked it. `find_enum` had three callers — the § 5.8.2 *name* check,
//! a `SortDirection` presence test and introspection's kind resolution — and
//! every consumer of `EnumDefinition::values` was a generator (the client
//! emitters, the OpenAPI schema), never a validator. `BANANA`, `"pending"` and
//! `42` all reached the resolver, on the literal path and the variable path
//! alike, against an argument the schema says has three members.
//!
//! Enum membership is therefore adjudicated, and **nested input-object fields
//! are walked for it** — unlike the scalar check, which stops at the argument.
//! It has to: the shape the defect was reported against is an enum *inside* an
//! input object, and a `where:` predicate reaches its enums the same way.
//!
//! [`EnumDefinition::values`]: crate::schema::EnumDefinition::values
//! * **Nullability.** An explicit `null` is accepted for every argument, including a non-null one.
//!   That is § 5.6.1's other half; it changes which *documents* are valid rather than which
//!   *answers* are correct, so it is not folded in here.
//! * **Mutations**, for the scalar check. Their arguments are input objects almost without
//!   exception, which the paragraph above excludes anyway. The *enum* check does cover them, from
//!   the mutation chokepoint rather than from here — see [`validate_enum_argument_values`].
//!
//! # Variable *values* are the half a spec-shaped fix would miss
//!
//! § 5.8.5 compares a variable's **declared type** against the argument's, which
//! catches `query($n: String!) { products(limit: $n) }`. It says nothing about
//! `query($n: Int!)` supplied with `{"n": "2"}` — the declaration is impeccable
//! and the *value* is wrong. That document returned the whole table too, so
//! [`validate_variable_values`] checks supplied values against their own
//! declarations (§ 6.1.2), including the case where a non-null variable is not
//! supplied at all: that also dropped its argument and widened the result.

use serde_json::Value;

use crate::{
    error::{FraiseQLError, Result},
    graphql::{
        types::{GraphQLArgument, VariableDefinition},
        value_json,
    },
    schema::{ArgumentDefinition, CompiledSchema, EnumDefinition, FieldType},
};

/// A built-in scalar whose value space this module is willing to adjudicate.
///
/// Membership is the whole leniency policy: a type that maps to `None` on both
/// constructors below is passed through untouched, because a project can back
/// it with any JSON shape and a disagreement would not be evidence of a client
/// mistake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scalar {
    Int,
    Float,
    String,
    Boolean,
    Id,
    Uuid,
    DateTime,
    Date,
    Time,
    Decimal,
}

impl Scalar {
    /// The scalar an argument's compiled type adjudicates as, or `None` for
    /// everything this module leaves alone.
    const fn from_field_type(field_type: &FieldType) -> Option<Self> {
        match field_type {
            FieldType::Int => Some(Self::Int),
            FieldType::Float => Some(Self::Float),
            FieldType::String => Some(Self::String),
            FieldType::Boolean => Some(Self::Boolean),
            FieldType::Id => Some(Self::Id),
            FieldType::Uuid => Some(Self::Uuid),
            FieldType::DateTime => Some(Self::DateTime),
            FieldType::Date => Some(Self::Date),
            FieldType::Time => Some(Self::Time),
            FieldType::Decimal => Some(Self::Decimal),
            _ => None,
        }
    }

    /// The scalar a *declared variable type name* adjudicates as.
    ///
    /// A name outside this set is a project's own scalar, an enum or an input
    /// object, and is not adjudicated — see the module header.
    fn from_type_name(name: &str) -> Option<Self> {
        match name {
            "Int" => Some(Self::Int),
            "Float" => Some(Self::Float),
            "String" => Some(Self::String),
            "Boolean" => Some(Self::Boolean),
            "ID" => Some(Self::Id),
            "UUID" => Some(Self::Uuid),
            "DateTime" => Some(Self::DateTime),
            "Date" => Some(Self::Date),
            "Time" => Some(Self::Time),
            "Decimal" => Some(Self::Decimal),
            _ => None,
        }
    }

    /// The name this scalar publishes, for error messages.
    const fn name(self) -> &'static str {
        match self {
            Self::Int => "Int",
            Self::Float => "Float",
            Self::String => "String",
            Self::Boolean => "Boolean",
            Self::Id => "ID",
            Self::Uuid => "UUID",
            Self::DateTime => "DateTime",
            Self::Date => "Date",
            Self::Time => "Time",
            Self::Decimal => "Decimal",
        }
    }

    /// Why `value` does not belong to this scalar, phrased for the client, or
    /// `None` when it does belong.
    ///
    /// Separate from [`Self::accepts`] so an integer that is merely too large
    /// does not read as "you wrote an Int where an Int was expected" — the
    /// message that first came out of `limit: 99999999999999`.
    fn rejection(self, value: &Value) -> Option<String> {
        if self.accepts(value) {
            return None;
        }
        if self == Self::Int && value.is_i64() {
            return Some(format!("an Int outside the 32-bit range ({}..={})", i32::MIN, i32::MAX));
        }
        Some(format!("a {} value", json_shape(value)))
    }

    /// Does `value` belong to this scalar's value space?
    ///
    /// `null` belongs to every one of them: nullability is not adjudicated here
    /// (module header).
    fn accepts(self, value: &Value) -> bool {
        if value.is_null() {
            return true;
        }
        match self {
            // § 3.5.1: Int is 32-bit signed, and a Float literal is not an Int
            // even when its fractional part is zero. The range half is
            // load-bearing rather than pedantic — `limit: 99999999999999`
            // overflowed `u32` at the read site and dropped the clause.
            Self::Int => value.as_i64().is_some_and(|v| i32::try_from(v).is_ok()),
            Self::Float => value.is_number(),
            Self::String | Self::Uuid | Self::DateTime | Self::Date | Self::Time => {
                value.is_string()
            },
            Self::Boolean => value.is_boolean(),
            // § 3.5.5: ID serializes as a String but accepts an integer input.
            Self::Id => value.is_string() || value.as_i64().is_some(),
            // Decimal travels as a string to keep precision, and an integer
            // literal is an ordinary way to write one.
            Self::Decimal => value.is_string() || value.is_number(),
        }
    }

    /// May a variable declared as `self` be used where `location` is expected
    /// (§ 5.8.5)?
    ///
    /// Deliberately wider than the spec's `AreTypesCompatible`, which admits
    /// only identical named types: a client declaring `$id: String!` for a
    /// `UUID` argument is writing the type its code generator produced for a
    /// custom scalar, not making the mistake this rule exists to catch. What is
    /// refused is a declaration whose value space cannot supply the location's
    /// — `String!` at `Int`, which is exactly how the unbounded page arrived.
    fn usable_at(self, location: Self) -> bool {
        if self == location {
            return true;
        }
        matches!(
            (self, location),
            (Self::Int, Self::Float | Self::Decimal | Self::Id)
                | (Self::Float, Self::Decimal)
                | (
                    Self::String,
                    Self::Id
                        | Self::Uuid
                        | Self::DateTime
                        | Self::Date
                        | Self::Time
                        | Self::Decimal
                )
                | (Self::Id, Self::String | Self::Uuid)
                | (Self::Uuid, Self::String | Self::Id)
                | (Self::DateTime | Self::Date | Self::Time | Self::Decimal, Self::String)
        )
    }
}

/// The scalar a supplied JSON value looks like, for an error message that says
/// what arrived rather than echoing it.
fn json_shape(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "Boolean",
        Value::Number(n) => {
            if n.is_f64() {
                "Float"
            } else {
                "Int"
            }
        },
        Value::String(_) => "String",
        Value::Array(_) => "a list",
        Value::Object(_) => "an object",
    }
}

/// Check the value written at each of `provided` against the type `declared`
/// gives that argument (§ 5.6.1), and the declared type of each variable used
/// there against the same (§ 5.8.5).
///
/// `field_label` names the field the way a client reads a schema —
/// `Query.orders` — and `declared` is the field's published argument list
/// ([`QueryDefinition::graphql_arguments`](crate::schema::QueryDefinition::graphql_arguments)),
/// which is where the auto-wired `limit: Int` and `offset: Int` acquire a type.
/// An argument with no entry there is not adjudicated: the relay cursor window
/// and `nearest` are read by their own runners and are accepted by name alone.
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] naming the field, the argument, the
/// type it declares and the type the document wrote. The offending value is
/// **not** quoted back — the shape is what the client needs, and a value in an
/// error message is how #1197's second half returned a stored row to its caller.
pub fn validate_argument_values(
    field_label: &str,
    declared: &[ArgumentDefinition],
    provided: &[GraphQLArgument],
    variable_defs: &[VariableDefinition],
) -> Result<()> {
    for arg in provided {
        let Some(def) = declared.iter().find(|d| d.name == arg.name) else {
            continue;
        };
        let Some(location) = Scalar::from_field_type(&def.arg_type) else {
            continue;
        };

        // An enum literal is a bare name that JSON has to carry as a string;
        // whether it belongs at a scalar argument is an enum question, not this
        // one.
        if arg.value_type == "enum" {
            continue;
        }

        let value = value_json::decode(&arg.value_json)?;

        if let Some(var_name) = value_json::variable_name(&value) {
            check_variable_usage(field_label, &arg.name, location, var_name, variable_defs)?;
        } else if let Some(reason) = location.rejection(&value) {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Argument `{}` on {field_label} has type `{}`, but the document wrote {reason}",
                    arg.name,
                    location.name(),
                ),
                path:    Some(arg.name.clone()),
            });
        }
    }
    Ok(())
}

/// § 5.8.5 for one argument: the variable used there must be declared at a type
/// whose values the argument can accept.
fn check_variable_usage(
    field_label: &str,
    arg_name: &str,
    location: Scalar,
    var_name: &str,
    variable_defs: &[VariableDefinition],
) -> Result<()> {
    // An undeclared reference is § 5.8.3's error, raised before this runs.
    let Some(def) = variable_defs.iter().find(|v| v.name == var_name) else {
        return Ok(());
    };
    // A list-typed variable at a scalar argument is a shape question, and § 3.11
    // lets a single value stand for a list, so the wrapper is not adjudicated.
    if def.var_type.list {
        return Ok(());
    }
    let Some(declared) = Scalar::from_type_name(&def.var_type.name) else {
        return Ok(());
    };
    if declared.usable_at(location) {
        return Ok(());
    }
    Err(FraiseQLError::Validation {
        message: format!(
            "Variable `${var_name}` is declared as `{}` but is used for argument `{arg_name}` on \
             {field_label}, which has type `{}`",
            def.var_type.name,
            location.name()
        ),
        path:    Some(arg_name.to_string()),
    })
}

/// Check each supplied variable against the type its operation declares, and
/// refuse a non-null variable that carries no value at all (§ 6.1.2).
///
/// This is the half § 5.8.5 cannot reach. `query($n: Int!) { products(limit:
/// $n) }` is a correct *usage* whatever `{"n": "2"}` does to it, and what it
/// did was return the whole table: the value failed `as_u64()` at the read site
/// and the `LIMIT` clause was never emitted. The same happened when `$n` was
/// declared `Int!` and simply not supplied.
///
/// A variable with a **default** and no supplied value is left alone: the
/// default is what applies, and it was written against the same declaration.
/// A *nullable* variable with no value is also left alone, deliberately — that
/// is what lets `limit: $limit` fall back to the query's compiled default
/// instead of forcing `LIMIT NULL`.
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] naming the variable and its declared
/// type. As above, the supplied value is described, never quoted.
pub fn validate_variable_values(
    operation_name: Option<&str>,
    variable_defs: &[VariableDefinition],
    values: Option<&Value>,
) -> Result<()> {
    let supplied = values.and_then(Value::as_object);

    for def in variable_defs {
        let value = supplied.and_then(|map| map.get(&def.name));

        let Some(value) = value else {
            if !def.var_type.nullable && def.default_value.is_none() {
                return Err(FraiseQLError::Validation {
                    message: format!(
                        "Variable `${}` is declared `{}!`{} but no value was supplied",
                        def.name,
                        def.var_type.name,
                        operation_label(operation_name)
                    ),
                    path:    Some(def.name.clone()),
                });
            }
            continue;
        };

        if value.is_null() && !def.var_type.nullable {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Variable `${}` is declared `{}!`{} but was supplied as null",
                    def.name,
                    def.var_type.name,
                    operation_label(operation_name)
                ),
                path:    Some(def.name.clone()),
            });
        }

        // A list declaration carries its items' type, not the value's; § 3.11
        // also lets a bare value stand for a one-element list. Neither is
        // adjudicated here.
        if def.var_type.list {
            continue;
        }
        let Some(declared) = Scalar::from_type_name(&def.var_type.name) else {
            continue;
        };
        if let Some(reason) = declared.rejection(value) {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Variable `${}` is declared `{}`{} but was supplied {reason}",
                    def.name,
                    def.var_type.name,
                    operation_label(operation_name),
                ),
                path:    Some(def.name.clone()),
            });
        }
    }
    Ok(())
}

// ── Enum membership — § 5.6.1 and § 6.1.2 for the one kind the schema fully
// ── specifies (#1362) ────────────────────────────────────────────────────────

/// How deep the walk follows nested input objects before giving up.
///
/// The recursion is driven by the *value*, which is finite, so this is a stack
/// guard and not a termination argument: a self-referential input object like
/// `OrderWhereInput._and: [OrderWhereInput!]` recurses only as far as the client
/// actually nested. Matches `value_json`'s own literal-nesting cap, so a document
/// that parsed cannot be refused here for depth it was already allowed.
const MAX_WALK_DEPTH: usize = 32;

/// At most this many members are named in a refusal before it summarises.
const MAX_MEMBERS_LISTED: usize = 12;

/// Adjudicate enum membership for the values written at a field's arguments,
/// from a **resolved** argument map (#1362).
///
/// This is the entry the mutation chokepoint uses, and the reason it takes a map
/// rather than a document's `[GraphQLArgument]`: `execute_mutation_impl` is where
/// every transport that writes converges, and REST, gRPC and MCP arrive there
/// with a JSON payload and no GraphQL document at all. Inline literals are
/// already merged into that same map (#719), so one call covers a literal, a
/// variable and a payload alike.
///
/// `provided` is the map arguments are read out of — the value under each
/// argument's own name. An argument with no entry is not adjudicated; whether it
/// was required is the mutation runner's question, asked a few lines later.
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] naming the path to the offending field
/// and the members its enum declares.
pub fn validate_enum_argument_values(
    schema: &CompiledSchema,
    field_label: &str,
    declared: &[ArgumentDefinition],
    provided: Option<&Value>,
) -> Result<()> {
    let Some(map) = provided.and_then(Value::as_object) else {
        return Ok(());
    };
    for arg in declared {
        let Some(value) = map.get(&arg.name) else {
            continue;
        };
        walk_field_type(schema, field_label, &arg.name, &arg.arg_type, value, 0)?;
    }
    Ok(())
}

/// Adjudicate enum membership for the **literals** a document writes at a field's
/// arguments (#1362).
///
/// The read path's counterpart to [`validate_enum_argument_values`]. A value that
/// is a variable *reference* is skipped here and adjudicated by
/// [`validate_enum_variable_values`] against its own declaration, so neither
/// check has to resolve the other's half and a reference cannot be mistaken for
/// an object whose single key happens to be the variable marker.
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] as above.
pub fn validate_enum_argument_literals(
    schema: &CompiledSchema,
    field_label: &str,
    declared: &[ArgumentDefinition],
    provided: &[GraphQLArgument],
) -> Result<()> {
    for arg in provided {
        let Some(def) = declared.iter().find(|d| d.name == arg.name) else {
            continue;
        };
        let value = value_json::decode(&arg.value_json)?;
        if value_json::variable_name(&value).is_some() {
            continue;
        }
        walk_field_type(schema, field_label, &arg.name, &def.arg_type, &value, 0)?;
    }
    Ok(())
}

/// Adjudicate enum membership for supplied variable values against their own
/// declarations — § 6.1.2, for enums (#1362).
///
/// Kept separate from [`validate_variable_values`] rather than folded into it
/// because that function answers a question about built-in scalars and needs no
/// schema; this one cannot be asked without one. They are called together at
/// every site, so the pair is the check.
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] naming the variable and the members its
/// enum declares.
pub fn validate_enum_variable_values(
    schema: &CompiledSchema,
    operation_name: Option<&str>,
    variable_defs: &[VariableDefinition],
    values: Option<&Value>,
) -> Result<()> {
    let Some(map) = values.and_then(Value::as_object) else {
        return Ok(());
    };
    for def in variable_defs {
        let Some(value) = map.get(&def.name) else {
            continue;
        };
        let subject = format!("Variable `${}`{}", def.name, operation_label(operation_name));
        walk_type_ref(schema, &subject, "", &def.var_type.name, value, 0)?;
    }
    Ok(())
}

/// Walk a compiled [`FieldType`] against `value`, adjudicating every enum under it.
fn walk_field_type(
    schema: &CompiledSchema,
    subject: &str,
    path: &str,
    declared: &FieldType,
    value: &Value,
    depth: usize,
) -> Result<()> {
    if value.is_null() || depth > MAX_WALK_DEPTH {
        return Ok(());
    }
    match declared {
        FieldType::List(inner) => walk_list(schema, subject, path, value, depth, |v, p, d| {
            walk_field_type(schema, subject, p, inner, v, d)
        }),
        FieldType::Enum(name) => check_membership(schema, subject, path, name, value),
        // The compiler emits an input-type reference as `Object`, never `Input`
        // (`parse_field_type` has no `Input` variant), so both have to resolve
        // through the input registry — the same pairing `execute_mutation_impl`
        // makes when it decides whether an argument is a structured input.
        FieldType::Object(name) | FieldType::Input(name) => {
            walk_input_object(schema, subject, path, name, value, depth)
        },
        _ => Ok(()),
    }
}

/// Walk a **written** GraphQL type reference — the form an input field and a
/// variable declaration carry — against `value`.
///
/// Input fields store their type as a string (`"[OrderStatus!]"`), not a
/// [`FieldType`], so the wrappers are peeled here rather than matched.
fn walk_type_ref(
    schema: &CompiledSchema,
    subject: &str,
    path: &str,
    declared: &str,
    value: &Value,
    depth: usize,
) -> Result<()> {
    if depth > MAX_WALK_DEPTH {
        return Ok(());
    }
    let declared = declared.trim();
    // `!` before `[`: `[X]!` peels to `[X]` and then to `X`, and `[X!]` to `X!`
    // to `X`. Peeling brackets first would leave a stray `!` on the inner name.
    if let Some(inner) = declared.strip_suffix('!') {
        return walk_type_ref(schema, subject, path, inner, value, depth);
    }
    if let Some(inner) = declared.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        return walk_list(schema, subject, path, value, depth, |v, p, d| {
            walk_type_ref(schema, subject, p, inner, v, d)
        });
    }
    if value.is_null() {
        return Ok(());
    }
    if schema.find_enum(declared).is_some() {
        return check_membership(schema, subject, path, declared, value);
    }
    walk_input_object(schema, subject, path, declared, value, depth)
}

/// Apply `walk` to each element of a list value.
///
/// A value that is not an array is walked as a single element rather than
/// skipped: § 3.11 lets a bare value stand for a one-element list, and that
/// coercion is how `where: {status: {in: PENDING}}` is written in practice. A
/// list declaration must not become a hole an unchecked enum fits through.
fn walk_list(
    _schema: &CompiledSchema,
    _subject: &str,
    path: &str,
    value: &Value,
    depth: usize,
    mut walk: impl FnMut(&Value, &str, usize) -> Result<()>,
) -> Result<()> {
    match value.as_array() {
        Some(items) => {
            for (index, item) in items.iter().enumerate() {
                walk(item, &format!("{path}[{index}]"), depth + 1)?;
            }
            Ok(())
        },
        None => walk(value, path, depth + 1),
    }
}

/// Walk the fields of a declared input object, if `name` is one.
///
/// A name that is not a registered input object resolves to nothing and is
/// passed through: a custom scalar, an output object or a type this schema does
/// not declare is not this check's business.
fn walk_input_object(
    schema: &CompiledSchema,
    subject: &str,
    path: &str,
    name: &str,
    value: &Value,
    depth: usize,
) -> Result<()> {
    let (Some(input_type), Some(object)) = (schema.find_input_type(name), value.as_object()) else {
        return Ok(());
    };
    for field in &input_type.fields {
        // The client writes the *surface* name, which under `camelCase` differs
        // from the stored one. Both are accepted: `display_name` is what the
        // required-field check (#414) looks the value up by, and the canonical
        // name is what a payload arriving over REST or gRPC carries. Validating
        // only one of the two would leave the other transport's enums unchecked
        // — and a value present under either key is a value that reaches SQL.
        let surface = schema.display_name(&field.name);
        let value = object
            .get(surface.as_str())
            .or_else(|| (surface != field.name).then(|| object.get(&field.name)).flatten());
        let Some(value) = value else {
            continue;
        };
        let child = if path.is_empty() {
            surface
        } else {
            format!("{path}.{surface}")
        };
        walk_type_ref(schema, subject, &child, &field.field_type, value, depth + 1)?;
    }
    Ok(())
}

/// The adjudication itself: `value` must name a member of `enum_name`.
fn check_membership(
    schema: &CompiledSchema,
    subject: &str,
    path: &str,
    enum_name: &str,
    value: &Value,
) -> Result<()> {
    let Some(def) = schema.find_enum(enum_name) else {
        return Ok(());
    };
    // A GraphQL enum value is a bare name, which JSON carries as a string —
    // `value_json` encodes a literal `PENDING` and a variable-supplied
    // `"PENDING"` identically, which is what lets one check cover both paths.
    if let Some(written) = value.as_str() {
        if def.values.iter().any(|member| member.name == written) {
            return Ok(());
        }
    }
    let wrote = if value.is_string() {
        "a name that is not one of its members".to_string()
    } else {
        format!("{} , which is not a name at all", json_shape(value))
    };
    let at = if path.is_empty() {
        String::new()
    } else {
        format!(" at `{path}`")
    };
    Err(FraiseQLError::Validation {
        // The offending value is described, never quoted back — the same rule
        // the scalar half follows, and for the same reason (#1197). The members
        // are safe to name: introspection publishes exactly this list.
        message: format!(
            "{subject}{at} has enum type `{enum_name}`, but the document wrote {wrote}. \
             Valid members: {}",
            member_list(def)
        ),
        path:    (!path.is_empty()).then(|| path.to_string()),
    })
}

/// The members of `def`, capped so a large enum does not produce an unreadable
/// error.
fn member_list(def: &EnumDefinition) -> String {
    let total = def.values.len();
    let shown: Vec<&str> =
        def.values.iter().take(MAX_MEMBERS_LISTED).map(|v| v.name.as_str()).collect();
    if total > shown.len() {
        format!("{} (and {} more)", shown.join(", "), total - shown.len())
    } else {
        shown.join(", ")
    }
}

/// The " in operation ..." clause an error message carries, or nothing for an
/// anonymous operation.
fn operation_label(operation_name: Option<&str>) -> String {
    operation_name.map_or_else(String::new, |name| format!(" in operation `{name}`"))
}

#[cfg(test)]
#[path = "argument_value_validation_tests.rs"]
mod argument_value_validation_tests;

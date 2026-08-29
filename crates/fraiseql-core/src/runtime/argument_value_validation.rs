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
//! * **Custom scalars, enums, input objects, lists and vectors.** A project may back any of these
//!   with any JSON shape, so a disagreement here is not evidence of a client mistake.
//! * **Nested input-object fields.** Only the value written *at* the argument is checked, not the
//!   keys inside a `where:` predicate. Those have their own surface and their own operators.
//! * **Nullability.** An explicit `null` is accepted for every argument, including a non-null one.
//!   That is § 5.6.1's other half; it changes which *documents* are valid rather than which
//!   *answers* are correct, so it is not folded in here.
//! * **Mutations.** Their arguments are input objects almost without exception, which the paragraph
//!   above excludes anyway.
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
    schema::{ArgumentDefinition, FieldType},
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

/// The " in operation ..." clause an error message carries, or nothing for an
/// anonymous operation.
fn operation_label(operation_name: Option<&str>) -> String {
    operation_name.map_or_else(String::new, |name| format!(" in operation `{name}`"))
}

#[cfg(test)]
#[path = "argument_value_validation_tests.rs"]
mod argument_value_validation_tests;

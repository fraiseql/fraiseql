//! Non-null completion of a response — GraphQL § 6.4.4 (*Handling Execution Errors*, #1522),
//! and the result coercion of an enum whose members store values (§ 3.9, #1580) and of an
//! `Int` (§ 3.5.1, #1928).
//!
//! A field the schema publishes as non-null (`String!`, `[Edge!]!`) must never be answered
//! with `null`. When the stored value is missing or `null`, § 6.4.4 makes it a **field
//! error**: the error is added to `errors` with the field's response path, and the `null`
//! propagates to the nearest ancestor position that allows it (a nullable field or list
//! item), up to `data` itself when every position above is non-null.
//!
//! Before this nothing checked it: a row with no `name` came back `{"name": null}` under a
//! `String!`, and a typed client generated from the schema crashed on a value it was told
//! could not occur.
//!
//! # The types are the ones introspection publishes
//!
//! The rule is applied against [`IntrospectionBuilder::build`], the same model `__schema`
//! and `__type` answer from, so what the runtime enforces is exactly what it advertises:
//! the relay `Connection`/`Edge` wrappers, the translations siblings of localized fields
//! and the root operation types included. A type or field that model does not carry is
//! passed through unadjudicated, the governing rule of the validators next door.
//!
//! # One pass, after execution
//!
//! Completion runs once over a root field's finished value, guided by the operation's
//! selection set (aliases are response keys; inline fragments apply when their type
//! condition matches the object's `__typename`; `@skip`/`@include` are evaluated against the
//! request's variables), rather than inside each projector. Every read path — regular,
//! relay, composed, node, federation `_entities`, aggregates, mutation payloads and
//! subscription events — produces a value of the published type, so one walk over that
//! value is the one place the rule can be applied to all of them alike.

use std::{
    cell::OnceCell,
    collections::{HashMap, HashSet},
};

use ahash::AHashMap;
use fraiseql_db::types::temporal::Temporal;
use serde_json::{Map, Value};

use crate::{
    graphql::{DirectiveEvaluator, FieldSelection},
    schema::{
        BUILTIN_SCALARS, CompiledSchema, FieldType, IntrospectionBuilder, IntrospectionType,
        TypeKind,
    },
};

/// How deep the walk follows a response before passing the rest through. A response is
/// depth-limited upstream (GATE 1); this bounds a hand-built one.
const MAX_COMPLETION_DEPTH: usize = 64;

/// An output type reference, wrappers included: `[Edge!]!` is
/// `NonNull(List(NonNull(Named("Edge"))))`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum TypeRef {
    NonNull(Box<TypeRef>),
    List(Box<TypeRef>),
    /// An enum: its value is checked against the members (§ 3.9 result coercion).
    Enum(String),
    /// A built-in scalar: its value is held to the form the transport publishes it in.
    Scalar(Scalar),
    /// An object, interface or union, or a scalar its value is served as read at.
    Named(String),
}

impl TypeRef {
    fn from_introspection(t: &IntrospectionType) -> Option<Self> {
        match t.kind {
            TypeKind::NonNull => {
                Some(Self::NonNull(Box::new(Self::from_introspection(t.of_type.as_deref()?)?)))
            },
            TypeKind::List => {
                Some(Self::List(Box::new(Self::from_introspection(t.of_type.as_deref()?)?)))
            },
            TypeKind::Enum => t.name.clone().map(Self::Enum),
            TypeKind::Scalar => t.name.as_deref().map(|name| {
                scalar_named(name).map_or_else(|| Self::Named(name.to_string()), Self::Scalar)
            }),
            _ => t.name.clone().map(Self::Named),
        }
    }
}

/// The output types of a schema, as introspection publishes them: every object and
/// interface's fields with their wrapped types, and every abstract type's possible types.
///
/// Every response is completed against these, a lookup per object and per field, so the
/// maps hash with `ahash`. They hold the schema's own names and are built once, so no
/// request can grow them.
#[derive(Debug, Clone, Default)]
pub struct OutputTypes {
    /// Object and interface type name → field name → published type.
    fields:   AHashMap<String, AHashMap<String, TypeRef>>,
    /// Interface and union type name → the object types it may resolve to.
    possible: AHashMap<String, HashSet<String>>,
    /// Enum type name → its members by stored value (#1580): a stored value is served as
    /// its member's name, and one that is no member's is a field error.
    enums:    AHashMap<String, EnumMembers>,
}

/// A level a composed read embeds into each row of the level above it: the related rows
/// of `type_name` written under `output_key` (an object, a list of them, or `null`), each
/// holding the selected fields and its own embedded levels.
#[derive(Debug, Clone)]
pub struct EmbeddedLevel {
    output_key: String,
    type_name:  String,
    /// The level as a field selecting its rows' fields, as the walk takes an object.
    selection:  FieldSelection,
    embedded:   Vec<EmbeddedLevel>,
}

impl EmbeddedLevel {
    /// The level written under `output_key`, of related rows of `type_name` holding
    /// `fields` and the levels `embedded` into each.
    #[must_use]
    pub fn new(output_key: &str, type_name: &str, fields: &[String], embedded: Vec<Self>) -> Self {
        let field = |name: &str| FieldSelection {
            name:          name.to_string(),
            alias:         None,
            arguments:     Vec::new(),
            nested_fields: Vec::new(),
            directives:    Vec::new(),
        };
        Self {
            output_key: output_key.to_string(),
            type_name: type_name.to_string(),
            selection: FieldSelection {
                nested_fields: fields.iter().map(|name| field(name)).collect(),
                ..field(output_key)
            },
            embedded,
        }
    }
}

/// An enum's members by the value each is stored as: a string (its name, or a string
/// value) or an integer.
#[derive(Debug, Clone)]
struct EnumMembers {
    strings:    AHashMap<String, String>,
    ints:       AHashMap<i64, String>,
    definition: crate::schema::EnumDefinition,
}

impl EnumMembers {
    fn of(e: &crate::schema::EnumDefinition) -> Self {
        let mut strings = AHashMap::new();
        let mut ints = AHashMap::new();
        for v in &e.values {
            match &v.value {
                Some(crate::schema::StoredEnumValue::Int(n)) => {
                    ints.insert(*n, v.name.clone());
                },
                Some(crate::schema::StoredEnumValue::String(s)) => {
                    strings.insert(s.clone(), v.name.clone());
                },
                None => {
                    strings.insert(v.name.clone(), v.name.clone());
                },
            }
        }
        Self {
            strings,
            ints,
            definition: e.clone(),
        }
    }

    /// The member `stored` is the stored value of.
    fn member_of(&self, stored: &Value) -> Option<&String> {
        match stored {
            Value::String(s) => self.strings.get(s.as_str()),
            Value::Number(n) => n.as_i64().and_then(|n| self.ints.get(&n)),
            _ => None,
        }
    }
}

impl OutputTypes {
    /// The output types `schema` publishes through introspection.
    #[must_use]
    pub fn from_schema(schema: &CompiledSchema) -> Self {
        let introspection = IntrospectionBuilder::build(schema);
        let mut types = Self::default();
        for t in &introspection.types {
            let Some(name) = &t.name else {
                continue;
            };
            if let Some(fields) = &t.fields {
                let fields = fields
                    .iter()
                    .filter_map(|f| {
                        TypeRef::from_introspection(&f.field_type).map(|r| (f.name.clone(), r))
                    })
                    .collect();
                types.fields.insert(name.clone(), fields);
            }
            if let Some(possible) = &t.possible_types {
                types
                    .possible
                    .insert(name.clone(), possible.iter().map(|p| p.name.clone()).collect());
            }
        }
        for e in &schema.enums {
            types.enums.insert(e.name.clone(), EnumMembers::of(e));
        }
        #[cfg(feature = "federation")]
        types.add_federation(schema);
        types
    }

    /// The federation entry point the subgraph SDL publishes (`_entities(representations:
    /// [_Any!]!): [_Entity]!`, `union _Entity` of the entity types), which introspection
    /// does not carry.
    #[cfg(feature = "federation")]
    fn add_federation(&mut self, schema: &CompiledSchema) {
        let Some(metadata) = schema.federation_metadata() else {
            return;
        };
        let entity = TypeRef::Named("_Entity".to_string());
        self.fields.entry("Query".to_string()).or_default().insert(
            "_entities".to_string(),
            TypeRef::NonNull(Box::new(TypeRef::List(Box::new(entity)))),
        );
        self.possible
            .insert("_Entity".to_string(), metadata.types.iter().map(|t| t.name.clone()).collect());
    }

    /// Whether `type_name` publishes `field` as non-null: a field error there nulls the
    /// nearest nullable ancestor rather than the field (§ 6.4.4). A field the type does not
    /// publish is not.
    #[must_use]
    pub fn is_non_null(&self, type_name: &str, field: &str) -> bool {
        self.type_fields(type_name)
            .and_then(|(_, fields)| fields.get(field))
            .is_some_and(|field_ref| matches!(field_ref, TypeRef::NonNull(_)))
    }

    /// The published fields of `type_name`, and the name as the schema spells it.
    fn type_fields(&self, type_name: &str) -> Option<(&str, &AHashMap<String, TypeRef>)> {
        self.fields
            .get_key_value(type_name)
            .map(|(name, fields)| (name.as_str(), fields))
    }

    /// Complete `response["data"]` (§ 6.4.4): each root field of `selections` on
    /// `root_type`, in place. A violation nulls the nearest nullable position, or `data`,
    /// and adds an entry to `response["errors"]`.
    ///
    /// `selections` is the operation's root selection set with fragment spreads expanded
    /// and directives not yet evaluated; `variables` (the request's, an object) evaluates
    /// them.
    pub fn complete(
        &self,
        response: &mut Value,
        root_type: &str,
        selections: &[FieldSelection],
        variables: Option<&Value>,
    ) {
        Self::complete_with(&Walk::new(self, variables), response, root_type, selections);
    }

    /// [`complete`](Self::complete), for REST, which publishes its fields as OpenAPI
    /// types: each built-in scalar position is held to the type it is published as.
    ///
    /// - An `Int` is an `integer` of any size (no `int32` format), so a `bigint` is served; a whole
    ///   number written with a fraction, or a numeric string in its canonical form, is that
    ///   integer.
    /// - A `Float` is a `number` (a numeric string in JSON's syntax is it), a `Boolean` a `boolean`
    ///   (the text `true` or `false` is it).
    /// - A `String` or an `ID` serves a value of any other kind as its JSON text and never refuses:
    ///   a REST client has no errors document to read a refusal from.
    /// - A `Decimal` is a `string` of its digits: an integer is its exact text.
    /// - A date, a time or a date-time is a `string` in its scalar's format, as on GraphQL; a UUID
    ///   is a `string`. Any other value is refused.
    /// - A JSON value and a project's own scalar are served as read.
    ///
    /// Any other value is refused with an entry in `response["errors"]` naming the field,
    /// its published type and the stored value's kind, never the value. GraphQL publishes
    /// the same fields as § 3.5's scalars, and [`complete`](Self::complete) holds each to
    /// its own result coercion.
    ///
    /// `embedded` are the levels a composed read embeds into each row of the root field
    /// `root_key`: each related row is completed against its own type, as a root row is.
    pub fn complete_as_published(
        &self,
        response: &mut Value,
        root_type: &str,
        selections: &[FieldSelection],
        root_key: &str,
        embedded: &[EmbeddedLevel],
    ) {
        let walk = Walk::new(self, None).holding_published_types();
        Self::complete_with(&walk, response, root_type, selections);
        if embedded.is_empty() {
            return;
        }
        let mut errors = Vec::new();
        if let Some(rows) = response.get_mut("data").and_then(|data| data.get_mut(root_key)) {
            let mut path = vec![Step::Key(root_key)];
            walk.embedded(rows, embedded, &mut path, &mut errors);
        }
        Self::add_errors(response, errors);
    }

    fn complete_with(
        walk: &Walk<'_>,
        response: &mut Value,
        root_type: &str,
        selections: &[FieldSelection],
    ) {
        let mut errors = Vec::new();
        let Some(data) = response.get_mut("data").and_then(Value::as_object_mut) else {
            return;
        };
        let mut null_data = false;
        let mut path = Vec::new();
        if let Some((root_type, fields)) = walk.types.type_fields(root_type) {
            walk.for_each_field(selections, root_type, &mut |sel| {
                let Some(field_ref) = fields.get(&sel.name) else {
                    return;
                };
                let key = sel.response_key();
                let Some(value) = data.get_mut(key) else {
                    return;
                };
                path.push(Step::Key(key));
                let label = Label::Field(root_type, &sel.name);
                if walk.position(value, field_ref, sel, label, &mut path, &mut errors, 0).is_err() {
                    null_data = true;
                }
                path.pop();
            });
        }
        if null_data {
            response["data"] = Value::Null;
        }
        Self::add_errors(response, errors);
    }

    /// Append `errors` to `response["errors"]`, creating it.
    fn add_errors(response: &mut Value, errors: Vec<Value>) {
        if errors.is_empty() {
            return;
        }
        match response.get_mut("errors").and_then(Value::as_array_mut) {
            Some(existing) => existing.extend(errors),
            None => {
                if let Some(object) = response.as_object_mut() {
                    object.insert("errors".to_string(), Value::Array(errors));
                }
            },
        }
    }
}

impl OutputTypes {
    /// Complete one value of `type_name` at a nullable position (§ 6.4.4), against the
    /// sub-selection `selections` (spreads expanded, directives evaluated): the value, or
    /// `null` when a violation propagated to it, and the errors, their paths rooted at
    /// `root_key`. A subscription event is one: every subscription field is nullable.
    #[must_use]
    pub fn complete_value(
        &self,
        mut value: Value,
        type_name: &str,
        selections: &[FieldSelection],
        root_key: &str,
    ) -> (Value, Vec<Value>) {
        let walk = Walk::new(self, None);
        let sel = FieldSelection {
            name:          root_key.to_string(),
            alias:         None,
            arguments:     Vec::new(),
            nested_fields: selections.to_vec(),
            directives:    Vec::new(),
        };
        let mut errors = Vec::new();
        let mut path = vec![Step::Key(root_key)];
        let field_ref = TypeRef::Named(type_name.to_string());
        if walk
            .position(
                &mut value,
                &field_ref,
                &sel,
                Label::Root(root_key),
                &mut path,
                &mut errors,
                0,
            )
            .is_err()
        {
            value = Value::Null;
        }
        (value, errors)
    }
}

/// A non-null violation below, to be absorbed by the nearest nullable position.
struct Propagate;

/// One step of a response path, borrowed from the walk. Rendered only into an error.
#[derive(Clone, Copy)]
enum Step<'a> {
    Key(&'a str),
    Index(usize),
}

/// The field an error names: `Type.field`, or a root key on its own. Rendered only into an
/// error.
#[derive(Clone, Copy)]
enum Label<'a> {
    Field(&'a str, &'a str),
    Root(&'a str),
}

impl std::fmt::Display for Label<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Field(owner, field) => write!(f, "{owner}.{field}"),
            Self::Root(key) => f.write_str(key),
        }
    }
}

/// The § 6.4.4 field error for a `null` at a non-null position.
fn violation(label: Label<'_>, path: &[Step<'_>]) -> Value {
    serde_json::json!({
        "message": format!("Cannot return null for non-nullable field {label}."),
        "path": response_path(path),
    })
}

/// The field error for a stored value of an enum position that is no member's stored value.
fn not_a_member(label: Label<'_>, path: &[Step<'_>], members: &EnumMembers) -> Value {
    serde_json::json!({
        "message": members.definition.not_a_member_message(&label),
        "path": response_path(path),
    })
}

/// The field error for a stored value a built-in scalar position cannot serve without loss.
///
/// It says what kind of value is stored, and why a string is not a temporal scalar's
/// value, never the value.
fn not_a_scalar(
    label: &dyn std::fmt::Display,
    scalar: Scalar,
    stored: &Value,
    path: &[Value],
) -> Value {
    serde_json::json!({
        "message": not_a_scalar_message(label, scalar, stored),
        "path": path,
    })
}

/// The message of [`not_a_scalar`]'s error.
fn not_a_scalar_message(label: &dyn std::fmt::Display, scalar: Scalar, stored: &Value) -> String {
    let name = scalar_name(scalar);
    let (article, gloss) = match scalar {
        Scalar::Int => ("an", ", a signed 32-bit integer"),
        Scalar::Id => ("an", ""),
        _ => ("a", ""),
    };
    let why = match (scalar, stored) {
        (Scalar::Temporal(temporal), Value::String(text)) => temporal.refusal(text),
        _ => None,
    };
    format!(
        "`{label}` is {article} `{name}`{gloss}, and the stored value is {} that {}",
        kind_of(stored),
        why.unwrap_or("is not one")
    )
}

/// What an `ID` field serves for `stored`, as GraphQL completion serves it.
///
/// `Ok(None)` for the value as stored (a string, or `null`), `Ok(Some(text))` for an
/// integer's decimal text, and `Err` with completion's message for any other kind.
///
/// An `ID` is a string (§ 3.5.5), and an integer the one other kind it names. A transport
/// with no completion of its own (gRPC) asks this, so it serves and refuses the same values
/// in the same words. `label` names the field, as `Type.field`. The message never quotes the
/// value.
///
/// # Errors
///
/// Completion's message, naming the field and the stored value's kind.
pub fn id_value(label: &dyn std::fmt::Display, stored: &Value) -> Result<Option<String>, String> {
    match coerce_built_in::<false>(Scalar::Id, stored) {
        Coercion::Served => Ok(None),
        Coercion::Rewrite(Value::String(text)) => Ok(Some(text)),
        Coercion::Rewrite(_) | Coercion::Refused => {
            Err(not_a_scalar_message(label, Scalar::Id, stored))
        },
    }
}

/// Why a field of `field_type` cannot serve `stored`, worded as completion refuses it, or
/// `None` when it serves the value as stored.
///
/// Only a temporal field (`DateTime`, `LocalDateTime`, `Date`, `Time`) is held here: its
/// value is a string in its scalar's format on every transport, so a transport with no
/// completion of its own (gRPC) refuses the same values, in the same words, by asking this.
/// `label` names the field, as `Type.field`. The message never quotes the value.
#[must_use]
pub fn temporal_refusal(
    label: &dyn std::fmt::Display,
    field_type: &FieldType,
    stored: &Value,
) -> Option<String> {
    let scalar @ Scalar::Temporal(_) = Scalar::of(field_type)? else {
        return None;
    };
    match coerce_built_in::<false>(scalar, stored) {
        Coercion::Refused => Some(not_a_scalar_message(label, scalar, stored)),
        Coercion::Served | Coercion::Rewrite(_) => None,
    }
}

/// The error for a stored value REST cannot serve as the OpenAPI type `scalar` is
/// published as. It names the stored value's kind (also under `"stored"`, which tells the
/// read's refusal from a missing value), never the value.
fn not_published(
    label: &dyn std::fmt::Display,
    scalar: Scalar,
    stored: &Value,
    path: &[Value],
) -> Value {
    serde_json::json!({
        "message": format!(
            "`{label}` is published as an OpenAPI `{}`, and the stored value is {} that is \
             not one",
            scalar.openapi_type(),
            kind_of(stored)
        ),
        "path": path,
        "stored": kind_of(stored),
    })
}

/// The JSON kind of `value`, with its article, for a message that must not quote it.
const fn kind_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a Boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a String",
        Value::Array(_) => "a list",
        Value::Object(_) => "an object",
    }
}

/// The source of the error refusing a REST read for a value it cannot serve.
///
/// The value is of a kind its published type cannot hold, or missing at a non-null field.
/// Buffered or streamed, the refusal carries it, so a transport can tell it from a failure
/// of the read itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompletionRefused;

impl std::fmt::Display for CompletionRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a stored value its published type cannot hold, or a missing one")
    }
}

impl std::error::Error for CompletionRefused {}

impl CompletionRefused {
    /// Whether `error` is a read refused for a value it could not serve.
    #[must_use]
    pub fn refused(error: &crate::error::FraiseQLError) -> bool {
        matches!(
            error,
            crate::error::FraiseQLError::Internal { source: Some(source), .. }
                if source.is::<Self>()
        )
    }
}

/// How § 3.5 result coercion serves a stored value at a built-in scalar position.
#[derive(Debug, Clone, PartialEq)]
enum Coercion {
    /// As it is.
    Served,
    /// As this value, which holds exactly the stored information in the scalar's form: a
    /// whole number written with a fraction at an `Int` (`3.0`), a numeric string in its
    /// canonical form at an `Int` or a `Float`, an integer's or a Boolean's text at a
    /// `String`, the text `true` or `false` at a `Boolean`.
    Rewrite(Value),
    /// Refused: a field error. Serving it would lose information or guess.
    Refused,
}

/// How § 3.5 serves `value` at a position of the scalar named `scalar`, or `None` for a
/// scalar this rule does not adjudicate (a custom scalar, or one GraphQL serves as read).
#[cfg(test)]
fn coerce_scalar(scalar: &str, value: &Value) -> Option<Coercion> {
    scalar_named(scalar)
        .filter(|scalar| !matches!(scalar, Scalar::Decimal | Scalar::Uuid))
        .map(|scalar| coerce_built_in::<false>(scalar, value))
}

/// How `value` is served at a position of the built-in `scalar`: by § 3.5 (`OPEN_API`
/// false), or as the OpenAPI type REST publishes it as (`OPEN_API` true; see
/// [`OutputTypes::complete_as_published`]). Every completion asks this, the aggregate
/// rows' included, so one table decides what each scalar may hold on each transport; the
/// walk resolves a field's scalar once, as [`TypeRef::Scalar`].
#[inline]
fn coerce_built_in<const OPEN_API: bool>(scalar: Scalar, value: &Value) -> Coercion {
    if served_as_is::<OPEN_API>(scalar, value) {
        Coercion::Served
    } else {
        coerce_other::<OPEN_API>(scalar, value)
    }
}

/// The name GraphQL publishes `scalar` by, where § 3.5 can refuse a value of it.
const fn scalar_name(scalar: Scalar) -> &'static str {
    match scalar {
        Scalar::Int => "Int",
        Scalar::Float => "Float",
        Scalar::String => "String",
        Scalar::Boolean => "Boolean",
        Scalar::Id => "ID",
        Scalar::Temporal(Temporal::DateTime) => "DateTime",
        Scalar::Temporal(Temporal::LocalDateTime) => "LocalDateTime",
        Scalar::Temporal(Temporal::Date) => "Date",
        Scalar::Temporal(Temporal::Time) => "Time",
        // Served as read by GraphQL, so never named in its refusal.
        Scalar::Decimal | Scalar::Uuid => "",
    }
}

/// A built-in scalar whose stored value completion holds to the form a transport
/// publishes it in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scalar {
    Int,
    Float,
    String,
    Boolean,
    /// Served as a string (§ 3.5.5): an integer is the one other kind it names.
    Id,
    /// Its digits, as a string. GraphQL serves it as read.
    Decimal,
    /// A date, a time or a date-time: a string in its scalar's format, on every
    /// transport.
    Temporal(Temporal),
    /// A UUID: a string. GraphQL serves it as read.
    Uuid,
}

impl Scalar {
    /// The scalar a field of `field_type` is, by its kind.
    ///
    /// Exhaustive, so a new [`FieldType`] stops the crate compiling until it says which
    /// rule its stored values are held to on each transport.
    const fn of(field_type: &FieldType) -> Option<Self> {
        match field_type {
            FieldType::Int => Some(Self::Int),
            FieldType::Float => Some(Self::Float),
            // A bit vector's and a sparse vector's value is their text.
            FieldType::String | FieldType::BitVector | FieldType::SparseVector => {
                Some(Self::String)
            },
            FieldType::Boolean => Some(Self::Boolean),
            FieldType::Id => Some(Self::Id),
            FieldType::Decimal => Some(Self::Decimal),
            FieldType::DateTime => Some(Self::Temporal(Temporal::DateTime)),
            FieldType::LocalDateTime => Some(Self::Temporal(Temporal::LocalDateTime)),
            FieldType::Date => Some(Self::Temporal(Temporal::Date)),
            FieldType::Time => Some(Self::Temporal(Temporal::Time)),
            FieldType::Uuid => Some(Self::Uuid),
            // Any JSON, the dense vectors published as JSON, a project's own scalar, and
            // what is no scalar.
            FieldType::Json
            | FieldType::Vector
            | FieldType::HalfVector
            | FieldType::Scalar(_)
            | FieldType::List { .. }
            | FieldType::Object(_)
            | FieldType::Enum(_)
            | FieldType::Input(_)
            | FieldType::Interface(_)
            | FieldType::Union(_) => None,
        }
    }

    /// The OpenAPI type REST publishes the scalar as.
    const fn openapi_type(self) -> &'static str {
        match self {
            Self::Int => "integer",
            Self::Float => "number",
            Self::Boolean => "boolean",
            Self::String | Self::Id | Self::Decimal | Self::Temporal(_) | Self::Uuid => "string",
        }
    }
}

/// The built-in scalar introspection publishes as `name`, resolved once per field when
/// the output types are built.
fn scalar_named(name: &str) -> Option<Scalar> {
    BUILTIN_SCALARS
        .iter()
        .filter(|(_, field_type)| field_type.to_graphql_string() == name)
        .find_map(|(_, field_type)| Scalar::of(field_type))
}

/// Whether `value` is one `scalar` holds as served: `null`, or a value of its kind (a
/// GraphQL `Int`'s a signed 32-bit integer, an OpenAPI `integer` any integer). Every value
/// a scalar position usually holds, decided in the walk without a call.
#[inline]
fn served_as_is<const OPEN_API: bool>(scalar: Scalar, value: &Value) -> bool {
    match (scalar, value) {
        (_, Value::Null)
        | (Scalar::Float, Value::Number(_))
        | (Scalar::String | Scalar::Id | Scalar::Decimal | Scalar::Uuid, Value::String(_))
        | (Scalar::Boolean, Value::Bool(_)) => true,
        (Scalar::Decimal | Scalar::Uuid, _) => !OPEN_API,
        (Scalar::Int, Value::Number(number)) => {
            if OPEN_API {
                !number.is_f64()
            } else {
                number.as_i64().is_some_and(|n| i32::try_from(n).is_ok())
            }
        },
        _ => false,
    }
}

/// [`coerce_built_in`] for a value [`served_as_is`] does not serve: never `null`, nor a
/// value of the scalar's own kind but an `Int`'s number outside its range.
fn coerce_other<const OPEN_API: bool>(scalar: Scalar, value: &Value) -> Coercion {
    use Scalar::{Boolean, Decimal, Float, Id, Int, String, Temporal};
    match (scalar, value) {
        // REST publishes a `String` or an `ID` as a `string`, and its client has no errors
        // document to read a refusal from: a value of any other kind is its JSON text. A
        // fraction is written as the `f64` it was read as (`1.50` → `"1.5"`), and an
        // object without the spacing the database wrote it with.
        (String | Id, _) if OPEN_API => Coercion::Rewrite(Value::String(value.to_string())),
        (Int, Value::Number(number)) => coerce_int::<OPEN_API>(number),
        (Int, Value::String(text)) => {
            canonical_int::<OPEN_API>(text).map_or(Coercion::Refused, Coercion::Rewrite)
        },
        (Float, Value::String(text)) => {
            number_text(text).map_or(Coercion::Refused, |n| Coercion::Rewrite(Value::Number(n)))
        },
        // An integer's text is exact on every path; a fraction's is not: the `Value` path
        // holds `1.50` as the `f64` 1.5, so its text would not be the stored one. An `ID`
        // is a string (§ 3.5.5), and an integer the one other kind it names; a `Decimal`'s
        // digits reach here only as an integer's (a read takes them as text).
        (String | Id | Decimal, Value::Number(number)) if !number.is_f64() => {
            Coercion::Rewrite(Value::String(number.to_string()))
        },
        (String, Value::Bool(b)) => Coercion::Rewrite(Value::String(b.to_string())),
        (Boolean, Value::String(text)) => match text.as_str() {
            "true" => Coercion::Rewrite(Value::Bool(true)),
            "false" => Coercion::Rewrite(Value::Bool(false)),
            _ => Coercion::Refused,
        },
        // A date, a time or a date-time: a string in its scalar's format, and nothing
        // else. PostgreSQL writes a `timestamp` without an offset, which a `DateTime`
        // would serve to be read in whatever zone each client assumes.
        (Temporal(temporal), Value::String(text)) => match temporal.refusal(text) {
            None => Coercion::Served,
            Some(_) => Coercion::Refused,
        },
        _ => Coercion::Refused,
    }
}

/// A number at an `Int` position. § 3.5.1: a signed 32-bit integer is served, a whole
/// number written with a fraction or an exponent (`3.0`, `-0`, `3e2`) is that integer, and
/// any other number is refused. An OpenAPI `integer` has no range: any integer is served,
/// and a whole number written with a fraction is that integer.
fn coerce_int<const OPEN_API: bool>(number: &serde_json::Number) -> Coercion {
    if !number.is_f64() {
        // An integer past `i64` is a `u64`, outside § 3.5.1's range whatever its value.
        let in_range = number.as_i64().is_some_and(|n| i32::try_from(n).is_ok());
        return if OPEN_API || in_range {
            Coercion::Served
        } else {
            Coercion::Refused
        };
    }
    let Some(float) = number.as_f64().filter(|float| float.fract() == 0.0) else {
        return Coercion::Refused;
    };
    if OPEN_API {
        // `2^63` is the first whole `f64` an `i64` cannot hold; past it the number is
        // served as read, a whole number all the same.
        let two_63 = 2.0_f64.powi(63);
        let in_i64 = (-two_63..two_63).contains(&float);
        #[allow(clippy::cast_possible_truncation)]
        // Reason: a whole number checked to lie within `i64` just above
        return if in_i64 {
            Coercion::Rewrite(Value::from(float as i64))
        } else {
            Coercion::Served
        };
    }
    if !(f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&float) {
        return Coercion::Refused;
    }
    #[allow(clippy::cast_possible_truncation)]
    // Reason: a whole number checked to lie within `i32` just above
    Coercion::Rewrite(Value::from(float as i32))
}

/// The integer `text` writes in its canonical form (`42`, `-7`, `0`: no sign but a leading
/// `-`, no leading zero, no `-0`), if its position can hold it: a signed 32-bit integer
/// under § 3.5.1, any integer JSON's numbers hold exactly under OpenAPI. Any other text
/// would be served as a value its client never wrote.
fn canonical_int<const OPEN_API: bool>(text: &str) -> Option<Value> {
    let digits = text.strip_prefix('-').unwrap_or(text);
    let canonical = !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
        && (digits == "0" || !digits.starts_with('0'))
        && text != "-0";
    if !canonical {
        return None;
    }
    if OPEN_API {
        text.parse::<i64>()
            .map(Value::from)
            .or_else(|_| text.parse::<u64>().map(Value::from))
            .ok()
    } else {
        text.parse::<i32>().ok().map(Value::from)
    }
}

/// The number `text` is, written in JSON's number syntax and nothing around it, if one can
/// hold it: `1e400` cannot.
fn number_text(text: &str) -> Option<serde_json::Number> {
    let bare = !text.is_empty() && text.trim() == text;
    bare.then(|| serde_json::from_str(text).ok()).flatten()
}

fn response_path(path: &[Step<'_>]) -> Vec<Value> {
    path.iter()
        .map(|step| match *step {
            Step::Key(key) => Value::String(key.to_string()),
            Step::Index(index) => Value::from(index),
        })
        .collect()
}

/// One completion: the types and the request's variables.
///
/// Completion runs on every response, so the walk allocates nothing unless it records an
/// error: values are completed in place, the path and the field label are borrowed and
/// rendered only into an error, and the variables become the map directives are evaluated
/// against only when a selection carries a directive.
struct Walk<'a> {
    types:               &'a OutputTypes,
    variables:           Option<&'a Value>,
    directive_variables: OnceCell<HashMap<String, Value>>,
    /// Whether a built-in scalar position is held to the OpenAPI type REST publishes it as
    /// rather than to § 3.5: see [`OutputTypes::complete_as_published`].
    open_api:            bool,
}

impl<'a> Walk<'a> {
    const fn new(types: &'a OutputTypes, variables: Option<&'a Value>) -> Self {
        Self {
            types,
            variables,
            directive_variables: OnceCell::new(),
            open_api: false,
        }
    }

    /// The same walk, holding each built-in scalar position to the OpenAPI type REST
    /// publishes it as: see [`OutputTypes::complete_as_published`].
    const fn holding_published_types(mut self) -> Self {
        self.open_api = true;
        self
    }

    /// Whether `@skip`/`@include` keep `sel`.
    fn included(&self, sel: &FieldSelection) -> bool {
        if sel.directives.is_empty() {
            return true;
        }
        let variables = self.directive_variables.get_or_init(|| {
            self.variables
                .and_then(Value::as_object)
                .map(|map| map.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default()
        });
        // A directive that cannot be evaluated was refused before execution; included is
        // the reading that adjudicates the most.
        DirectiveEvaluator::evaluate_directives(sel, variables).unwrap_or(true)
    }

    /// Visit the fields `selections` selects on an object of type `type_name`: inline
    /// fragments whose condition the type satisfies are lifted, fields excluded by
    /// `@skip`/`@include` and meta-fields are skipped.
    fn for_each_field<'s>(
        &self,
        selections: &'s [FieldSelection],
        type_name: &str,
        visit: &mut impl FnMut(&'s FieldSelection),
    ) {
        for sel in selections {
            if !self.included(sel) {
                continue;
            }
            if let Some(condition) = sel.name.strip_prefix("...on ") {
                if self.satisfies(type_name, condition.trim()) {
                    self.for_each_field(&sel.nested_fields, type_name, visit);
                }
                continue;
            }
            if sel.name.starts_with("...") || sel.name.starts_with("__") {
                continue;
            }
            visit(sel);
        }
    }

    /// Whether an object of type `type_name` matches a fragment on `condition`.
    fn satisfies(&self, type_name: &str, condition: &str) -> bool {
        condition == type_name
            || self.types.possible.get(condition).is_some_and(|p| p.contains(type_name))
    }

    /// Complete `value`, in place, at a position of type `field_ref`.
    ///
    /// `Err(Propagate)` when the position is non-null and its value is `null`, whether it
    /// arrived so (an error is recorded here, at `path`) or became so by a violation below
    /// (already recorded there). A nullable position absorbs a violation below by becoming
    /// `null`.
    #[allow(clippy::too_many_arguments)] // Reason: one recursive step of one walk; a struct would hold the same state
    fn position<'s>(
        &self,
        value: &mut Value,
        field_ref: &TypeRef,
        sel: &'s FieldSelection,
        label: Label<'s>,
        path: &mut Vec<Step<'s>>,
        errors: &mut Vec<Value>,
        depth: usize,
    ) -> Result<(), Propagate>
    where
        'a: 's,
    {
        match field_ref {
            TypeRef::NonNull(inner) => {
                self.nullable(value, inner, sel, label, path, errors, depth)?;
                if value.is_null() {
                    errors.push(violation(label, path));
                    return Err(Propagate);
                }
                Ok(())
            },
            other => {
                if self.nullable(value, other, sel, label, path, errors, depth).is_err() {
                    *value = Value::Null;
                }
                Ok(())
            },
        }
    }

    /// Complete a value, in place, at a nullable position: `Err(Propagate)` when a
    /// non-null violation below makes it `null`.
    #[allow(clippy::too_many_arguments)] // Reason: as `position`
    fn nullable<'s>(
        &self,
        value: &mut Value,
        field_ref: &TypeRef,
        sel: &'s FieldSelection,
        label: Label<'s>,
        path: &mut Vec<Step<'s>>,
        errors: &mut Vec<Value>,
        depth: usize,
    ) -> Result<(), Propagate>
    where
        'a: 's,
    {
        if value.is_null() || depth > MAX_COMPLETION_DEPTH {
            return Ok(());
        }
        match field_ref {
            TypeRef::NonNull(_) => self.position(value, field_ref, sel, label, path, errors, depth),
            // A stored value is served as its member's name (#1580), and a value that is no
            // member's is a field error, never served (§ 3.9 result coercion).
            TypeRef::Enum(name) => {
                let Some(members) = self.types.enums.get(name.as_str()) else {
                    return Ok(());
                };
                let Some(member) = members.member_of(value) else {
                    errors.push(not_a_member(label, path, members));
                    return Err(Propagate);
                };
                if value.as_str() != Some(member.as_str()) {
                    *value = Value::String(member.clone());
                }
                Ok(())
            },
            TypeRef::List(inner) => {
                let Value::Array(items) = value else {
                    return Ok(());
                };
                // Every item is completed, so every violation is reported, before one
                // that propagated nulls the list.
                let mut propagated = false;
                for (index, item) in items.iter_mut().enumerate() {
                    path.push(Step::Index(index));
                    if self.position(item, inner, sel, label, path, errors, depth + 1).is_err() {
                        propagated = true;
                    }
                    path.pop();
                }
                if propagated { Err(Propagate) } else { Ok(()) }
            },
            TypeRef::Scalar(scalar) => {
                let coercion = if self.open_api {
                    coerce_built_in::<true>(*scalar, value)
                } else {
                    coerce_built_in::<false>(*scalar, value)
                };
                match coercion {
                    Coercion::Served => Ok(()),
                    Coercion::Rewrite(coerced) => {
                        *value = coerced;
                        Ok(())
                    },
                    Coercion::Refused => {
                        let path = response_path(path);
                        errors.push(if self.open_api {
                            not_published(&label, *scalar, value, &path)
                        } else {
                            not_a_scalar(&label, *scalar, value, &path)
                        });
                        Err(Propagate)
                    },
                }
            },
            TypeRef::Named(name) => match value {
                Value::Object(map) => self.object(map, name, sel, path, errors, depth),
                _ => Ok(()),
            },
        }
    }

    /// Complete an object value of declared type `declared`, in place, against `sel`'s
    /// sub-selection.
    fn object<'s>(
        &self,
        map: &mut Map<String, Value>,
        declared: &str,
        sel: &'s FieldSelection,
        path: &mut Vec<Step<'s>>,
        errors: &mut Vec<Value>,
        depth: usize,
    ) -> Result<(), Propagate>
    where
        'a: 's,
    {
        // The object is of the type its `__typename` names, when that is a type the schema
        // publishes: an abstract type's variant, or the error type a failed mutation
        // resolves to under a bare success return type. Without one, the declared type's
        // own fields (an interface's) are all that can be adjudicated.
        let types = self.types;
        let Some((concrete, fields)) = map
            .get("__typename")
            .and_then(Value::as_str)
            .and_then(|t| types.type_fields(t))
            .or_else(|| types.type_fields(declared))
        else {
            return Ok(());
        };
        // Every field is completed, so every violation is reported, before one that
        // propagated nulls the object.
        let mut propagated = false;
        self.for_each_field(&sel.nested_fields, concrete, &mut |child| {
            let Some(field_ref) = fields.get(&child.name) else {
                return;
            };
            let key = child.response_key();
            path.push(Step::Key(key));
            let label = Label::Field(concrete, &child.name);
            // Completed in place, never removed and re-inserted: the response keeps the
            // document's field order (§ 6.3). An absent field is completed as `null` and
            // stays absent.
            let completed = match map.get_mut(key) {
                Some(value) => {
                    self.position(value, field_ref, child, label, path, errors, depth + 1)
                },
                None => self.position(
                    &mut Value::Null,
                    field_ref,
                    child,
                    label,
                    path,
                    errors,
                    depth + 1,
                ),
            };
            path.pop();
            if completed.is_err() {
                propagated = true;
            }
        });
        if propagated { Err(Propagate) } else { Ok(()) }
    }

    /// Complete the levels `embedded` into each row of `rows` (a row, or a list of them).
    /// Each related row is completed against its type; a violation's error is recorded, and
    /// the read it belongs to is refused, so nothing is nulled.
    fn embedded<'s>(
        &self,
        rows: &mut Value,
        embedded: &'s [EmbeddedLevel],
        path: &mut Vec<Step<'s>>,
        errors: &mut Vec<Value>,
    ) where
        'a: 's,
    {
        match rows {
            Value::Array(rows) => {
                for (index, row) in rows.iter_mut().enumerate() {
                    path.push(Step::Index(index));
                    self.embedded_into(row, embedded, path, errors);
                    path.pop();
                }
            },
            row => self.embedded_into(row, embedded, path, errors),
        }
    }

    /// [`embedded`](Self::embedded) for one row.
    fn embedded_into<'s>(
        &self,
        row: &mut Value,
        embedded: &'s [EmbeddedLevel],
        path: &mut Vec<Step<'s>>,
        errors: &mut Vec<Value>,
    ) where
        'a: 's,
    {
        let Value::Object(row) = row else {
            return;
        };
        for level in embedded {
            let Some(related) = row.get_mut(&level.output_key) else {
                continue;
            };
            path.push(Step::Key(&level.output_key));
            let mut complete = |related: &mut Value, path: &mut Vec<Step<'s>>| {
                if let Value::Object(map) = related {
                    // A violation is in `errors`, which refuses the read.
                    let _ = self.object(map, &level.type_name, &level.selection, path, errors, 1);
                }
                self.embedded_into(related, &level.embedded, path, errors);
            };
            match related {
                Value::Array(items) => {
                    for (index, item) in items.iter_mut().enumerate() {
                        path.push(Step::Index(index));
                        complete(item, path);
                        path.pop();
                    }
                },
                related => complete(related, path),
            }
            path.pop();
        }
    }
}

mod rows;

#[cfg(test)]
mod tests;

//! Per-file Rust emitters. Each `pub(super)` function returns the body of one
//! generated module; orchestration and header stamping live in the parent.
//!
//! The `GraphQL` documents come from `client::common` and are byte-identical to
//! every other language's; only the type rendering here is Rust-specific.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
};

use fraiseql_core::schema::{
    ArgumentDefinition, CompiledSchema, EnumDefinition, FieldDefinition, InputObjectDefinition,
    InterfaceDefinition, MutationDefinition, QueryDefinition, TypeDefinition, UnionDefinition,
};

use super::{
    Ctx,
    render::{
        custom_scalar_name, field_type_rs, field_type_rs_nullable, named_scalar_rs,
        parse_input_type, pascal_case, rs_ident,
    },
};
use crate::client::common::{
    self, const_name, finish, input_base_name, leaf_fields, referenced_named_type,
    selection_for_return,
};

/// Derives every generated data struct carries.
///
/// `Eq`/`Hash` are deliberately absent: a schema with one `Float` field would
/// stop deriving them, and a derive list that depends on the schema is a derive
/// list that changes under the author.
const STRUCT_DERIVES: &str =
    "#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]";

const RELAY_HELPERS: &str = r#"/// Relay cursor-pagination page descriptor.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PageInfo {
    /// Whether a further page exists after this one.
    #[serde(rename = "hasNextPage")]
    pub has_next_page: bool,
    /// Whether a page exists before this one.
    #[serde(rename = "hasPreviousPage")]
    pub has_previous_page: bool,
    /// Cursor of the first edge, when the page is non-empty.
    #[serde(rename = "startCursor")]
    pub start_cursor: Option<String>,
    /// Cursor of the last edge, when the page is non-empty.
    #[serde(rename = "endCursor")]
    pub end_cursor: Option<String>,
}

/// One Relay connection edge: a cursor and its node.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Edge<T> {
    /// Opaque cursor addressing this edge.
    pub cursor: String,
    /// The edge's node.
    pub node: T,
}

/// A Relay connection over `T`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Connection<T> {
    /// The page's edges, in server order.
    pub edges: Vec<Edge<T>>,
    /// Pagination state for this page.
    #[serde(rename = "pageInfo")]
    pub page_info: PageInfo,
    /// Total matching rows, when the query was compiled to count them.
    #[serde(rename = "totalCount", default, skip_serializing_if = "Option::is_none")]
    pub total_count: Option<i32>,
}
"#;

// =============================================================================
// types.rs
// =============================================================================

pub(super) fn types(ctx: &Ctx) -> String {
    let schema = ctx.schema;
    let mut refs: BTreeSet<&str> = BTreeSet::new();
    for t in &schema.types {
        collect_leaf_refs(&t.fields, &mut refs);
    }
    for i in &schema.interfaces {
        collect_leaf_refs(&i.fields, &mut refs);
    }

    let mut out = String::new();
    out.push_str(&render_imports(ctx, &refs, "types"));
    if !out.is_empty() {
        out.push('\n');
    }

    if ctx.has_relay {
        out.push_str(RELAY_HELPERS);
        out.push('\n');
    }
    for iface in &schema.interfaces {
        emit_interface(&mut out, iface);
        out.push('\n');
    }
    for ty in &schema.types {
        emit_object(&mut out, ctx, ty);
        out.push('\n');
    }
    for union in &schema.unions {
        emit_union(&mut out, union);
        out.push('\n');
    }
    finish(out)
}

fn collect_leaf_refs<'a>(fields: &'a [FieldDefinition], refs: &mut BTreeSet<&'a str>) {
    for field in leaf_fields(fields) {
        if let Some(name) = referenced_named_type(&field.field_type) {
            refs.insert(name);
        }
    }
}

/// One struct field: its comment lines, attributes, Rust name and Rust type.
struct RsField {
    comments: Vec<String>,
    attrs:    Vec<String>,
    name:     String,
    rs_type:  String,
}

fn emit_struct(out: &mut String, name: &str, fields: &[RsField]) {
    out.push_str(STRUCT_DERIVES);
    out.push('\n');
    if fields.is_empty() {
        let _ = writeln!(out, "pub struct {name};");
        return;
    }
    let _ = writeln!(out, "pub struct {name} {{");
    for field in fields {
        for comment in &field.comments {
            let _ = writeln!(out, "    {comment}");
        }
        for attr in &field.attrs {
            let _ = writeln!(out, "    {attr}");
        }
        let _ = writeln!(out, "    pub {}: {},", field.name, field.rs_type);
    }
    out.push_str("}\n");
}

/// A `#[serde(rename = "...")]` attribute, emitted only when the Rust
/// identifier differs from the `GraphQL` name it stands for.
fn rename_attr(gql_name: &str, rs_name: &str, extra: &str) -> Option<String> {
    if gql_name == rs_name && extra.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    if gql_name != rs_name {
        parts.push(format!("rename = \"{gql_name}\""));
    }
    if !extra.is_empty() {
        parts.push(extra.to_string());
    }
    Some(format!("#[serde({})]", parts.join(", ")))
}

fn leaf_struct_fields(fields: &[FieldDefinition]) -> Vec<RsField> {
    leaf_fields(fields)
        .into_iter()
        .map(|field| {
            let mut comments = Vec::new();
            if let Some(desc) = field.description.as_deref() {
                comments.push(doc_comment(desc));
            }
            if let Some(scalar) = custom_scalar_name(&field.field_type) {
                comments.push(format!("// TODO: brand custom scalar `{scalar}`"));
            }
            let name = rs_ident(field.name.as_str());
            RsField {
                attrs: rename_attr(field.name.as_str(), &name, "").into_iter().collect(),
                comments,
                name,
                rs_type: field_type_rs_nullable(&field.field_type, field.nullable),
            }
        })
        .collect()
}

/// The `__typename` discriminant every selection set requests.
///
/// `default` is load-bearing: `serde`'s internally tagged representation strips
/// the tag before handing the rest of the object to the variant, so a member
/// decoded *through* a union never sees `__typename` at all. `skip_serializing`
/// is the other half — re-emitting it would collide with the tag `serde` writes.
fn typename_field() -> RsField {
    RsField {
        comments: vec!["// The concrete type name the server returned.".to_string()],
        attrs:    vec!["#[serde(rename = \"__typename\", default, skip_serializing)]".to_string()],
        name:     "typename".to_string(),
        rs_type:  "String".to_string(),
    }
}

/// Interfaces become plain structs: Rust has no data inheritance, and the
/// generated documents select an interface's leaf fields directly.
fn emit_interface(out: &mut String, iface: &InterfaceDefinition) {
    push_doc(out, iface.description.as_deref());
    let mut fields = vec![typename_field()];
    fields.extend(leaf_struct_fields(&iface.fields));
    emit_struct(out, &iface.name, &fields);
}

fn emit_object(out: &mut String, ctx: &Ctx, ty: &TypeDefinition) {
    push_doc(out, ty.description.as_deref());
    let mut fields = vec![typename_field()];
    if ctx.error_typenames.contains(ty.name.as_str()) {
        fields.push(RsField {
            comments: vec![
                "// Error class injected by the mutation runtime (the `error_class`).".to_string(),
            ],
            attrs:    Vec::new(),
            name:     "status".to_string(),
            rs_type:  "String".to_string(),
        });
    }
    fields.extend(leaf_struct_fields(&ty.fields));
    emit_struct(out, ty.name.as_str(), &fields);
}

/// A `GraphQL` union becomes an internally tagged enum — the one place Rust's
/// type system maps onto `GraphQL`'s exactly.
fn emit_union(out: &mut String, union: &UnionDefinition) {
    push_doc(out, union.description.as_deref());
    if union.member_types.is_empty() {
        let _ = writeln!(out, "pub type {} = serde_json::Value;", union.name);
        return;
    }
    out.push_str(STRUCT_DERIVES);
    out.push_str("\n#[serde(tag = \"__typename\")]\n");
    let _ = writeln!(out, "pub enum {} {{", union.name);
    for member in &union.member_types {
        let _ = writeln!(out, "    /// The `{member}` member of this union.");
        let _ = writeln!(out, "    {member}({member}),");
    }
    out.push_str("}\n");
}

// =============================================================================
// enums.rs
// =============================================================================

pub(super) fn enums(schema: &CompiledSchema) -> String {
    let mut out = String::new();
    for def in &schema.enums {
        emit_enum(&mut out, def);
        out.push('\n');
    }
    finish(out)
}

fn emit_enum(out: &mut String, def: &EnumDefinition) {
    push_doc(out, def.description.as_deref());
    if def.values.is_empty() {
        let _ = writeln!(out, "pub type {} = serde_json::Value;", def.name);
        return;
    }
    out.push_str(
        "#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]\n",
    );
    let _ = writeln!(out, "pub enum {} {{", def.name);
    for value in &def.values {
        if let Some(desc) = value.description.as_deref() {
            let _ = writeln!(out, "    {}", doc_comment(desc));
        }
        let variant = pascal_case(&value.name);
        if variant != value.name {
            let _ = writeln!(out, "    #[serde(rename = \"{}\")]", value.name);
        }
        let _ = writeln!(out, "    {variant},");
    }
    out.push_str("}\n");
}

// =============================================================================
// inputs.rs
// =============================================================================

pub(super) fn inputs(ctx: &Ctx) -> String {
    let schema = ctx.schema;
    let mut refs: BTreeSet<&str> = BTreeSet::new();
    for input in &schema.input_types {
        for field in &input.fields {
            let base = input_base_name(&field.field_type);
            if let Some(name) =
                schema.enums.iter().find(|e| e.name == base).map(|e| e.name.as_str())
            {
                refs.insert(name);
            }
        }
    }

    let mut out = String::new();
    out.push_str(&render_imports(ctx, &refs, "inputs"));
    if !out.is_empty() {
        out.push('\n');
    }
    for input in &schema.input_types {
        emit_input(&mut out, input);
        out.push('\n');
    }
    finish(out)
}

fn emit_input(out: &mut String, input: &InputObjectDefinition) {
    push_doc(out, input.description.as_deref());
    let fields: Vec<RsField> = input
        .fields
        .iter()
        .map(|field| {
            let parsed = parse_input_type(&field.field_type);
            let name = rs_ident(&field.name);
            let (rs_type, extra) = if parsed.required {
                (parsed.rs, String::new())
            } else {
                (
                    format!("Option<{}>", parsed.rs),
                    "default, skip_serializing_if = \"Option::is_none\"".to_string(),
                )
            };
            RsField {
                comments: field.description.as_deref().map(doc_comment).into_iter().collect(),
                attrs: rename_attr(&field.name, &name, &extra).into_iter().collect(),
                name,
                rs_type,
            }
        })
        .collect();
    emit_struct(out, &input.name, &fields);
}

// =============================================================================
// queries.rs
// =============================================================================

pub(super) fn queries(ctx: &Ctx) -> String {
    let schema = ctx.schema;
    let mut refs: BTreeSet<&str> = BTreeSet::new();
    let mut operations = Vec::new();
    for q in &schema.queries {
        refs.insert(&q.return_type);
        for arg in &q.arguments {
            if let Some(name) = referenced_named_type(&arg.arg_type) {
                refs.insert(name);
            }
        }
        if q.relay {
            refs.insert("Connection");
        }
        operations.push(build_operation(&q.graphql_arguments(), q.relay));
    }

    let mut out = String::new();
    out.push_str(&runtime_imports(&operations));
    out.push_str(&render_imports(ctx, &refs, "queries"));
    out.push('\n');

    for (q, op) in schema.queries.iter().zip(&operations) {
        emit_query(&mut out, ctx, q, op);
        out.push('\n');
    }
    finish(out)
}

fn emit_query(out: &mut String, ctx: &Ctx, q: &QueryDefinition, op: &Operation) {
    let selection = selection_for_return(ctx, &q.return_type, q.relay);
    let document = common::render_document("query", &q.name, &op.gql, &selection);
    let result = query_result_rs(q);

    emit_document_const(out, &q.name, &document);
    push_doc(out, q.description.as_deref());
    emit_operation_fn(out, &q.name, op, &result);
}

fn query_result_rs(q: &QueryDefinition) -> String {
    let node = type_name_to_rs(&q.return_type);
    if q.relay {
        return format!("Connection<{node}>");
    }
    let base = if q.returns_list {
        format!("Vec<{node}>")
    } else {
        node
    };
    if q.nullable {
        format!("Option<{base}>")
    } else {
        base
    }
}

// =============================================================================
// mutations.rs
// =============================================================================

pub(super) fn mutations(ctx: &Ctx) -> String {
    let schema = ctx.schema;
    let mut refs: BTreeSet<&str> = BTreeSet::new();
    let mut operations = Vec::new();
    for m in &schema.mutations {
        refs.insert(&m.return_type);
        for arg in &m.arguments {
            if let Some(name) = referenced_named_type(&arg.arg_type) {
                refs.insert(name);
            }
        }
        operations.push(build_operation(&m.arguments, false));
    }

    let mut out = String::new();
    out.push_str(&runtime_imports(&operations));
    out.push_str(&render_imports(ctx, &refs, "mutations"));
    out.push('\n');

    emit_error_guard(&mut out, ctx);

    for (m, op) in schema.mutations.iter().zip(&operations) {
        emit_mutation(&mut out, ctx, m, op);
        out.push('\n');
    }
    finish(out)
}

fn emit_error_guard(out: &mut String, ctx: &Ctx) {
    if ctx.error_typenames.is_empty() {
        return;
    }
    let literals = ctx
        .error_typenames
        .iter()
        .map(|n| format!("\"{n}\""))
        .collect::<Vec<_>>()
        .join(", ");
    out.push_str("/// The typed-error members this schema declares.\n");
    let _ = writeln!(out, "pub const ERROR_TYPENAMES: &[&str] = &[{literals}];\n");
    out.push_str(
        "/// Whether a result's `__typename` names one of the schema's typed-error members.\n#[must_use]\npub fn is_error_typename(typename: &str) -> bool {\n    ERROR_TYPENAMES.contains(&typename)\n}\n\n",
    );
}

fn emit_mutation(out: &mut String, ctx: &Ctx, m: &MutationDefinition, op: &Operation) {
    let selection = selection_for_return(ctx, &m.return_type, false);
    let document = common::render_document("mutation", &m.name, &op.gql, &selection);
    let result = type_name_to_rs(&m.return_type);

    emit_document_const(out, &m.name, &document);
    push_doc(out, m.description.as_deref());
    emit_operation_fn(out, &m.name, op, &result);
}

// =============================================================================
// relationships.rs
// =============================================================================

pub(super) fn relationships(schema: &CompiledSchema) -> String {
    let mut out = String::new();
    out.push_str("/// One declared relationship between two types.\n#[derive(Debug, Clone, Copy, PartialEq, Eq)]\npub struct RelationshipMeta {\n    /// Field name carrying the relationship.\n    pub name: &'static str,\n    /// Type the relationship points at.\n    pub target_type: &'static str,\n    /// `oneToMany`, `manyToOne` or `oneToOne`.\n    pub cardinality: &'static str,\n    /// Column holding the foreign key.\n    pub foreign_key: &'static str,\n    /// Column the foreign key references.\n    pub referenced_key: &'static str,\n}\n\n");
    out.push_str("/// Every type that declares relationships, with its own.\n#[derive(Debug, Clone, Copy, PartialEq, Eq)]\npub struct TypeRelationships {\n    /// The declaring type's name.\n    pub type_name: &'static str,\n    /// Its relationships, in declaration order.\n    pub relationships: &'static [RelationshipMeta],\n}\n\n");
    out.push_str("/// Relationship metadata for every type that declares one.\npub const RELATIONSHIPS: &[TypeRelationships] = &[\n");
    for ty in &schema.types {
        if ty.relationships.is_empty() {
            continue;
        }
        let _ = writeln!(out, "    TypeRelationships {{");
        let _ = writeln!(out, "        type_name: \"{}\",", ty.name);
        out.push_str("        relationships: &[\n");
        for rel in &ty.relationships {
            let card = cardinality_label(rel.cardinality);
            let _ = writeln!(
                out,
                "            RelationshipMeta {{ name: \"{}\", target_type: \"{}\", cardinality: \"{card}\", foreign_key: \"{}\", referenced_key: \"{}\" }},",
                rel.name, rel.target_type, rel.foreign_key, rel.referenced_key
            );
        }
        out.push_str("        ],\n");
        out.push_str("    },\n");
    }
    out.push_str("];\n");
    finish(out)
}

const fn cardinality_label(card: fraiseql_core::schema::Cardinality) -> &'static str {
    use fraiseql_core::schema::Cardinality;
    match card {
        Cardinality::OneToMany => "oneToMany",
        Cardinality::OneToOne => "oneToOne",
        // Reason: Cardinality is #[non_exhaustive]; ManyToOne (and any future
        // variant) defaults to the object-embed cardinality.
        _ => "manyToOne",
    }
}

// =============================================================================
// mod.rs
// =============================================================================

/// The generated `mod.rs`: submodule declarations plus the data-type re-exports.
///
/// Operations are deliberately **not** glob re-exported. `GraphQL` allows a
/// query and a mutation to share a name, and two glob re-exports carrying the
/// same identifier are an ambiguity the consumer meets at their own use site;
/// `queries::` / `mutations::` says which one is meant.
pub(super) fn module(modules: &[&str]) -> String {
    let mut out = String::new();
    out.push_str("//! Generated FraiseQL client.\n//!\n//! Data types are re-exported here; operations stay under [`queries`] and\n//! [`mutations`], because `GraphQL` permits a query and a mutation to share a\n//! name and a glob re-export of both would make that name ambiguous.\n\n");

    let mut sorted: Vec<&str> = modules.to_vec();
    sorted.sort_unstable();
    for module in &sorted {
        let _ = writeln!(out, "pub mod {module};");
    }
    out.push('\n');

    // The runtime's names are re-exported explicitly, not by glob. A schema may
    // declare a type called `Error`, and two globs carrying one name trip
    // `ambiguous_glob_reexports` — which is deny-by-default, so the consumer's
    // build breaks. An explicit re-export takes precedence over a glob instead.
    out.push_str(
        "pub use client::{Error, FraiseqlClient, ResponseError, Transport, var, var_opt};\n",
    );
    for module in &sorted {
        if matches!(*module, "client" | "queries" | "mutations") {
            continue;
        }
        let _ = writeln!(out, "pub use {module}::*;");
    }
    out
}

// =============================================================================
// Operation building (shared by queries & mutations)
// =============================================================================

/// A built operation: the shared `GraphQL` half plus the Rust parameters.
pub(super) struct Operation {
    gql:    common::GqlOperation,
    params: Vec<RsParam>,
}

struct RsParam {
    /// Original `GraphQL` variable name (the key sent on the wire).
    gql_name: String,
    /// Snake-cased, keyword-safe Rust parameter name.
    rs_name:  String,
    /// Rust type expression (no nullability).
    rs_type:  String,
    optional: bool,
}

fn build_operation(arguments: &[ArgumentDefinition], relay: bool) -> Operation {
    let gql = common::build_gql_operation(arguments, relay);
    let mut params = Vec::new();

    for arg in arguments {
        params.push(RsParam {
            gql_name: arg.name.clone(),
            rs_name:  rs_ident(&arg.name),
            rs_type:  field_type_rs(&arg.arg_type),
            optional: arg.nullable,
        });
    }
    if relay {
        // Spec-standard forward pagination; both optional.
        params.push(RsParam {
            gql_name: "first".to_string(),
            rs_name:  "first".to_string(),
            rs_type:  "i32".to_string(),
            optional: true,
        });
        params.push(RsParam {
            gql_name: "after".to_string(),
            rs_name:  "after".to_string(),
            rs_type:  "String".to_string(),
            optional: true,
        });
    }

    Operation { gql, params }
}

/// The runtime `use` line for a file, naming only the helpers its operations
/// actually call — an unused import is a warning, and the gate denies warnings.
fn runtime_imports(operations: &[Operation]) -> String {
    let mut names = vec!["Error", "FraiseqlClient", "Transport"];
    if operations.iter().any(|op| op.params.iter().any(|p| !p.optional)) {
        names.push("var");
    }
    if operations.iter().any(|op| op.params.iter().any(|p| p.optional)) {
        names.push("var_opt");
    }
    names.sort_unstable();
    format!("use super::client::{{{}}};\n", names.join(", "))
}

/// The `pub const <NAME>: &str` holding the operation document.
///
/// The raw-string form needs no escaping and a `GraphQL` document cannot contain
/// the `"#` terminator: its only string-ish content is field and argument names.
fn emit_document_const(out: &mut String, name: &str, document: &str) {
    let _ = writeln!(out, "/// The `{name}` operation document.");
    let _ = writeln!(out, "pub const {}: &str = r#\"{document}\"#;\n", const_name(name));
}

/// Emit the `pub fn ...` wrapper that calls `client.request` and unwraps the
/// root field. Optional (nullable) arguments are `Option<T>` and omitted from
/// the request when `None`.
fn emit_operation_fn(out: &mut String, name: &str, op: &Operation, result: &str) {
    let doc_const = const_name(name);
    let fn_name = rs_ident(name);

    let _ = writeln!(out, "/// Execute the `{name}` operation.");
    out.push_str("///\n/// # Errors\n///\n/// Returns the transport's error, the response's `errors` array, or a\n/// deserialization failure — see [`Error`].\n");
    let _ = writeln!(out, "pub fn {fn_name}<T: Transport>(");
    out.push_str("    client: &FraiseqlClient<T>,\n");
    for p in &op.params {
        if p.optional {
            let _ = writeln!(out, "    {}: Option<{}>,", p.rs_name, p.rs_type);
        } else {
            let _ = writeln!(out, "    {}: {},", p.rs_name, p.rs_type);
        }
    }
    let _ = writeln!(out, ") -> Result<{result}, Error> {{");

    if op.params.is_empty() {
        let _ =
            writeln!(out, "    client.request({doc_const}, serde_json::Map::new(), \"{name}\")");
    } else {
        out.push_str("    let mut variables = serde_json::Map::new();\n");
        for p in &op.params {
            if p.optional {
                let _ = writeln!(
                    out,
                    "    var_opt(&mut variables, \"{}\", {}.as_ref())?;",
                    p.gql_name, p.rs_name
                );
            } else {
                let _ =
                    writeln!(out, "    var(&mut variables, \"{}\", &{})?;", p.gql_name, p.rs_name);
            }
        }
        let _ = writeln!(out, "    client.request({doc_const}, variables, \"{name}\")");
    }
    out.push_str("}\n");
}

// =============================================================================
// Small shared helpers
// =============================================================================

/// Resolve a schema type name to its Rust type (scalars mapped, else name).
fn type_name_to_rs(name: &str) -> String {
    named_scalar_rs(name).map_or_else(|| name.to_string(), str::to_string)
}

/// Emit one author-supplied description as a single-line `//` comment.
///
/// Comments — not `///` doc comments — carry descriptions everywhere so
/// schema-author text can never escape into code. In Rust that matters twice
/// over: a `///` block is markdown, and a fenced code block inside one becomes a
/// doctest the consumer's `cargo test` would try to compile and run.
fn doc_comment(description: &str) -> String {
    format!("// {}", description.replace('\n', " "))
}

fn push_doc(out: &mut String, description: Option<&str>) {
    if let Some(desc) = description {
        let _ = writeln!(out, "{}", doc_comment(desc));
    }
}

/// Render sorted `use super::module::{a, b};` lines for the given names, grouped
/// by the generated module that defines each name. Names not defined in any
/// generated module (scalars) are dropped.
fn render_imports(ctx: &Ctx, names: &BTreeSet<&str>, current: &str) -> String {
    let mut by_module: BTreeMap<&'static str, BTreeSet<&str>> = BTreeMap::new();
    for &name in names {
        if let Some(module) = ctx.module_of(name) {
            if module != current {
                by_module.entry(module).or_default().insert(name);
            }
        }
    }

    let mut out = String::new();
    for (module, idents) in by_module {
        let idents: Vec<&str> = idents.into_iter().collect();
        if let [single] = idents[..] {
            let _ = writeln!(out, "use super::{module}::{single};");
        } else {
            let _ = writeln!(out, "use super::{module}::{{{}}};", idents.join(", "));
        }
    }
    out
}

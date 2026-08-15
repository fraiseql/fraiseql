//! Per-file Go emitters. Each `pub(super)` function returns the body of one
//! generated file (after the `package` clause); orchestration and header
//! stamping live in the parent.
//!
//! The `GraphQL` documents come from `client::common` and are byte-identical to
//! every other language's; only the type rendering here is Go-specific.
//!
//! Everything lands in one package, so — unlike the `TypeScript` and Python
//! generators — no file needs cross-module imports. What *is* Go-specific is
//! gofmt's column alignment: struct fields and const blocks are padded here so
//! the emitted source is already `gofmt`-clean (the CI gate asserts it).

use std::fmt::Write as _;

use fraiseql_core::schema::{
    ArgumentDefinition, CompiledSchema, EnumDefinition, FieldDefinition, InputObjectDefinition,
    InterfaceDefinition, MutationDefinition, QueryDefinition, TypeDefinition, UnionDefinition,
};

use super::{
    Ctx,
    render::{
        custom_scalar_name, field_type_go_nullable, go_enum_suffix, go_export, go_param_name,
        is_nilable_go, named_scalar_go, parse_input_type,
    },
};
use crate::client::common::{self, finish, leaf_fields, selection_for_return};

const RELAY_HELPERS: &str = "// PageInfo is the Relay cursor-pagination page descriptor.\ntype PageInfo struct {\n\tHasNextPage     bool    `json:\"hasNextPage\"`\n\tHasPreviousPage bool    `json:\"hasPreviousPage\"`\n\tStartCursor     *string `json:\"startCursor\"`\n\tEndCursor       *string `json:\"endCursor\"`\n}\n\n// Edge is one Relay connection edge: a cursor and its node.\ntype Edge[T any] struct {\n\tCursor string `json:\"cursor\"`\n\tNode   T      `json:\"node\"`\n}\n\n// Connection is a Relay connection over T.\ntype Connection[T any] struct {\n\tEdges      []Edge[T] `json:\"edges\"`\n\tPageInfo   PageInfo  `json:\"pageInfo\"`\n\tTotalCount *int      `json:\"totalCount,omitempty\"`\n}\n";

// =============================================================================
// types.go
// =============================================================================

pub(super) fn types(ctx: &Ctx) -> String {
    let schema = ctx.schema;
    let mut out = String::new();
    // A union's generated UnmarshalJSON is the only thing in this file that
    // needs imports; without one, an import block would not compile.
    if schema.unions.iter().any(|u| !u.member_types.is_empty()) {
        out.push_str("import (\n\t\"encoding/json\"\n\t\"fmt\"\n)\n\n");
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

/// One struct field: its comment lines, Go name, Go type and struct tag.
struct GoField {
    comments: Vec<String>,
    name:     String,
    go_type:  String,
    tag:      String,
}

/// Emit a struct with gofmt's column alignment.
///
/// gofmt aligns a *run* of consecutive field lines; a full-line comment ends the
/// run (it has no tab-separated cells for `text/tabwriter` to align against), so
/// runs are computed the same way here.
fn emit_struct(out: &mut String, name: &str, fields: &[GoField]) {
    let _ = writeln!(out, "type {name} struct {{");
    let mut run: Vec<&GoField> = Vec::new();
    for field in fields {
        if !field.comments.is_empty() {
            flush_run(out, &run);
            run.clear();
            for comment in &field.comments {
                let _ = writeln!(out, "\t{comment}");
            }
        }
        run.push(field);
    }
    flush_run(out, &run);
    out.push_str("}\n");
}

/// Write the entries of a composite literal, aligned as gofmt aligns them:
/// every `key:` token padded to the run's widest, then one space, then the
/// value. `indent` is the literal's own indentation.
fn write_literal_entries(out: &mut String, indent: &str, entries: &[(String, String)]) {
    let width = entries.iter().map(|(key, _)| key.len() + 1).max().unwrap_or(0);
    for (key, value) in entries {
        let key = format!("{key}:");
        let _ = writeln!(out, "{indent}{key:<width$} {value},");
    }
}

/// Write one alignment run, padding names and types to the run's widest.
fn flush_run(out: &mut String, run: &[&GoField]) {
    let name_width = run.iter().map(|f| f.name.len()).max().unwrap_or(0);
    let type_width = run.iter().map(|f| f.go_type.len()).max().unwrap_or(0);
    for field in run {
        let _ = writeln!(
            out,
            "\t{name:<name_width$} {go_type:<type_width$} `{tag}`",
            name = field.name,
            go_type = field.go_type,
            tag = field.tag,
        );
    }
}

fn leaf_struct_fields(fields: &[FieldDefinition]) -> Vec<GoField> {
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
            GoField {
                comments,
                name: go_export(field.name.as_str()),
                go_type: field_type_go_nullable(&field.field_type, field.nullable),
                tag: format!("json:\"{}\"", field.name),
            }
        })
        .collect()
}

/// The `__typename` discriminant every selection set requests.
fn typename_field() -> GoField {
    GoField {
        comments: Vec::new(),
        name:     "Typename".to_string(),
        go_type:  "string".to_string(),
        tag:      "json:\"__typename\"".to_string(),
    }
}

/// Interfaces become plain structs: Go has no inheritance for data, and the
/// generated documents select an interface's leaf fields directly.
fn emit_interface(out: &mut String, iface: &InterfaceDefinition) {
    push_doc(out, &iface.name, iface.description.as_deref());
    let mut fields = vec![typename_field()];
    fields.extend(leaf_struct_fields(&iface.fields));
    emit_struct(out, &iface.name, &fields);
}

fn emit_object(out: &mut String, ctx: &Ctx, ty: &TypeDefinition) {
    push_doc(out, ty.name.as_str(), ty.description.as_deref());
    let mut fields = vec![typename_field()];
    if ctx.error_typenames.contains(ty.name.as_str()) {
        fields.push(GoField {
            comments: vec![
                "// Status is the error class injected by the mutation runtime.".to_string(),
            ],
            name:     "Status".to_string(),
            go_type:  "string".to_string(),
            tag:      "json:\"status\"".to_string(),
        });
    }
    fields.extend(leaf_struct_fields(&ty.fields));
    emit_struct(out, ty.name.as_str(), &fields);
}

/// A `GraphQL` union becomes a struct holding the discriminant plus one pointer
/// per member, decoded by a generated `UnmarshalJSON`.
///
/// Go has no sum type; the alternative — flattening every member's fields into
/// one struct — loses which member actually arrived, which is the only thing a
/// union is for.
fn emit_union(out: &mut String, union: &UnionDefinition) {
    let name = &union.name;
    push_doc(out, name, union.description.as_deref());
    let _ = writeln!(
        out,
        "// {name} is a GraphQL union: after decoding, Typename names the member that\n// arrived and exactly that member's pointer is non-nil."
    );

    let mut fields = vec![typename_field()];
    for member in &union.member_types {
        fields.push(GoField {
            comments: Vec::new(),
            name:     go_export(member),
            go_type:  format!("*{member}"),
            // Members are populated by UnmarshalJSON from the whole payload, not
            // by a field of their own; `-` keeps encoding/json off them.
            tag:      "json:\"-\"".to_string(),
        });
    }
    emit_struct(out, name, &fields);

    if union.member_types.is_empty() {
        return;
    }

    out.push('\n');
    let _ = writeln!(out, "// UnmarshalJSON decodes the member named by the payload's __typename.");
    let _ = writeln!(out, "func (u *{name}) UnmarshalJSON(data []byte) error {{");
    out.push_str("\tvar probe struct {\n\t\tTypename string `json:\"__typename\"`\n\t}\n");
    out.push_str("\tif err := json.Unmarshal(data, &probe); err != nil {\n\t\treturn err\n\t}\n");
    out.push_str("\tu.Typename = probe.Typename\n");
    out.push_str("\tswitch probe.Typename {\n");
    for member in &union.member_types {
        let _ = writeln!(out, "\tcase \"{member}\":");
        let _ = writeln!(out, "\t\tu.{} = new({member})", go_export(member));
        let _ = writeln!(out, "\t\treturn json.Unmarshal(data, u.{})", go_export(member));
    }
    out.push_str("\tdefault:\n");
    let _ = writeln!(
        out,
        "\t\treturn fmt.Errorf(\"unknown __typename %q for union {name}\", probe.Typename)"
    );
    out.push_str("\t}\n}\n");
}

// =============================================================================
// enums.go
// =============================================================================

pub(super) fn enums(schema: &CompiledSchema) -> String {
    let mut out = String::new();
    for def in &schema.enums {
        emit_enum(&mut out, def);
        out.push('\n');
    }
    finish(out)
}

/// A `GraphQL` enum becomes a defined string type plus one constant per value.
///
/// The constant's suffix is the value in `PascalCase` (`IN_PROGRESS` →
/// `OrderStatusInProgress`); the constant's *value* is always the wire spelling.
fn emit_enum(out: &mut String, def: &EnumDefinition) {
    let name = &def.name;
    push_doc(out, name, def.description.as_deref());
    let _ = writeln!(out, "type {name} string");
    if def.values.is_empty() {
        return;
    }
    out.push('\n');

    let rows: Vec<(String, String)> = def
        .values
        .iter()
        .map(|v| (format!("{name}{}", go_enum_suffix(&v.name)), format!("\"{}\"", v.name)))
        .collect();
    let width = rows.iter().map(|(ident, _)| ident.len()).max().unwrap_or(0);

    out.push_str("const (\n");
    for (ident, literal) in &rows {
        let _ = writeln!(out, "\t{ident:<width$} {name} = {literal}");
    }
    out.push_str(")\n");
}

// =============================================================================
// inputs.go
// =============================================================================

pub(super) fn inputs(schema: &CompiledSchema) -> String {
    let mut out = String::new();
    for input in &schema.input_types {
        emit_input(&mut out, input);
        out.push('\n');
    }
    finish(out)
}

fn emit_input(out: &mut String, input: &InputObjectDefinition) {
    push_doc(out, &input.name, input.description.as_deref());
    let fields: Vec<GoField> = input
        .fields
        .iter()
        .map(|field| {
            let parsed = parse_input_type(&field.field_type);
            let (go_type, tag) = if parsed.required {
                (parsed.go, format!("json:\"{}\"", field.name))
            } else if is_nilable_go(&parsed.go) {
                (parsed.go, format!("json:\"{},omitempty\"", field.name))
            } else {
                (format!("*{}", parsed.go), format!("json:\"{},omitempty\"", field.name))
            };
            GoField {
                comments: field.description.as_deref().map(doc_comment).into_iter().collect(),
                name: go_export(&field.name),
                go_type,
                tag,
            }
        })
        .collect();
    emit_struct(out, &input.name, &fields);
}

// =============================================================================
// queries.go
// =============================================================================

pub(super) fn queries(ctx: &Ctx) -> String {
    let mut out = String::new();
    for q in &ctx.schema.queries {
        emit_query(&mut out, ctx, q);
        out.push('\n');
    }
    finish(out)
}

fn emit_query(out: &mut String, ctx: &Ctx, q: &QueryDefinition) {
    // Render the auto-wired `where`/`orderBy`/`limit`/`offset` arguments derived
    // from `auto_params` so the generated query can paginate and filter.
    let arguments = q.graphql_arguments();
    let op = build_operation(&arguments, q.relay);
    let selection = selection_for_return(ctx, &q.return_type, q.relay);
    let document = common::render_document("query", &q.name, &op.gql, &selection);
    let result = query_result_go(q);

    emit_document_const(out, &q.name, &document);
    push_doc(out, &go_export(&q.name), q.description.as_deref());
    emit_operation_fn(out, &q.name, &op, &result);
}

fn query_result_go(q: &QueryDefinition) -> String {
    let node = type_name_to_go(&q.return_type);
    if q.relay {
        return format!("Connection[{node}]");
    }
    if q.returns_list {
        return format!("[]{node}");
    }
    if q.nullable && !is_nilable_go(&node) {
        format!("*{node}")
    } else {
        node
    }
}

// =============================================================================
// mutations.go
// =============================================================================

pub(super) fn mutations(ctx: &Ctx) -> String {
    let mut out = String::new();
    emit_error_guard(&mut out, ctx);
    for m in &ctx.schema.mutations {
        emit_mutation(&mut out, ctx, m);
        out.push('\n');
    }
    finish(out)
}

fn emit_error_guard(out: &mut String, ctx: &Ctx) {
    if ctx.error_typenames.is_empty() {
        return;
    }
    out.push_str(
        "// ErrorTypenames is the set of typed-error members the schema declares.\nvar ErrorTypenames = map[string]bool{\n",
    );
    let entries: Vec<(String, String)> = ctx
        .error_typenames
        .iter()
        .map(|name| (format!("\"{name}\""), "true".to_string()))
        .collect();
    write_literal_entries(out, "\t", &entries);
    out.push_str("}\n\n");
    out.push_str(
        "// IsErrorResult reports whether a result's __typename names one of the schema's\n// typed-error members.\nfunc IsErrorResult(typename string) bool { return ErrorTypenames[typename] }\n\n",
    );
}

fn emit_mutation(out: &mut String, ctx: &Ctx, m: &MutationDefinition) {
    let op = build_operation(&m.arguments, false);
    let selection = selection_for_return(ctx, &m.return_type, false);
    let document = common::render_document("mutation", &m.name, &op.gql, &selection);
    let result = type_name_to_go(&m.return_type);

    emit_document_const(out, &m.name, &document);
    push_doc(out, &go_export(&m.name), m.description.as_deref());
    emit_operation_fn(out, &m.name, &op, &result);
}

// =============================================================================
// relationships.go
// =============================================================================

pub(super) fn relationships(schema: &CompiledSchema) -> String {
    let mut out = String::new();
    out.push_str("// RelationshipMeta describes one declared relationship between two types.\ntype RelationshipMeta struct {\n\tTargetType    string\n\tCardinality   string\n\tForeignKey    string\n\tReferencedKey string\n}\n\n");
    out.push_str("// Relationships maps each type's name to its relationships, by field name.\nvar Relationships = map[string]map[string]RelationshipMeta{\n");
    for ty in &schema.types {
        if ty.relationships.is_empty() {
            continue;
        }
        let _ = writeln!(out, "\t\"{}\": {{", ty.name);
        let entries: Vec<(String, String)> = ty
            .relationships
            .iter()
            .map(|rel| {
                let card = cardinality_label(rel.cardinality);
                (
                    format!("\"{}\"", rel.name),
                    format!(
                        "{{TargetType: \"{}\", Cardinality: \"{card}\", ForeignKey: \"{}\", ReferencedKey: \"{}\"}}",
                        rel.target_type, rel.foreign_key, rel.referenced_key
                    ),
                )
            })
            .collect();
        write_literal_entries(&mut out, "\t\t", &entries);
        out.push_str("\t},\n");
    }
    out.push_str("}\n");
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
// Operation building (shared by queries & mutations)
// =============================================================================

/// A built operation: the shared `GraphQL` half plus the Go parameters.
struct Operation {
    gql:    common::GqlOperation,
    params: Vec<GoParam>,
}

struct GoParam {
    /// Original `GraphQL` variable name (the key sent on the wire).
    gql_name: String,
    /// Keyword-safe Go parameter name.
    go_name:  String,
    /// Go type expression, already carrying nullability.
    go_type:  String,
    optional: bool,
}

fn build_operation(arguments: &[ArgumentDefinition], relay: bool) -> Operation {
    let gql = common::build_gql_operation(arguments, relay);
    let mut params = Vec::new();

    for arg in arguments {
        params.push(GoParam {
            gql_name: arg.name.clone(),
            go_name:  go_param_name(&arg.name),
            go_type:  field_type_go_nullable(&arg.arg_type, arg.nullable),
            optional: arg.nullable,
        });
    }
    if relay {
        // Spec-standard forward pagination; both optional.
        params.push(GoParam {
            gql_name: "first".to_string(),
            go_name:  "first".to_string(),
            go_type:  "*int".to_string(),
            optional: true,
        });
        params.push(GoParam {
            gql_name: "after".to_string(),
            go_name:  "after".to_string(),
            go_type:  "*string".to_string(),
            optional: true,
        });
    }

    Operation { gql, params }
}

/// The unexported `const <name>Document = ...` holding the operation document.
///
/// Backticks are Go's raw-string delimiter and cannot be escaped inside one; a
/// `GraphQL` document never contains one (names, punctuation and the argument
/// values are all variables), so this is safe by construction.
fn emit_document_const(out: &mut String, name: &str, document: &str) {
    let _ = writeln!(out, "const {}Document = `{document}`\n", go_param_name(name));
}

/// Emit the operation wrapper that calls [`Client::Request`] and unwraps the
/// root field.
///
/// Operations are **methods on `*Client`**, not package functions, and that is
/// load-bearing rather than stylistic: Go has a single exported namespace per
/// package, and the canonical `GraphQL` schema has both a `user` query and a
/// `User` type — as package functions the two would redeclare the same
/// identifier. A method lives in the receiver's namespace, so `(*Client).User`
/// and the `User` struct coexist.
///
/// Every argument is positional: Go has neither keyword nor default arguments,
/// so optional ones are nilable and simply omitted from the variables map when
/// nil. A typed nil inside an `any` is not `== nil`, so the omission is decided
/// on the typed parameter here rather than after boxing it.
fn emit_operation_fn(out: &mut String, name: &str, op: &Operation, result: &str) {
    let exported = go_export(name);
    let _ = write!(out, "func (c *Client) {exported}(");
    for (index, p) in op.params.iter().enumerate() {
        let separator = if index == 0 { "" } else { ", " };
        let _ = write!(out, "{separator}{} {}", p.go_name, p.go_type);
    }
    let _ = writeln!(out, ") ({result}, error) {{");

    let _ = writeln!(out, "\tvar data struct {{");
    let _ = writeln!(out, "\t\tResult {result} `json:\"{name}\"`");
    out.push_str("\t}\n");

    let required: Vec<&GoParam> = op.params.iter().filter(|p| !p.optional).collect();
    let optional: Vec<&GoParam> = op.params.iter().filter(|p| p.optional).collect();

    let variables = if op.params.is_empty() {
        "nil".to_string()
    } else {
        if required.is_empty() {
            out.push_str("\tvariables := map[string]any{}\n");
        } else {
            out.push_str("\tvariables := map[string]any{\n");
            let entries: Vec<(String, String)> = required
                .iter()
                .map(|p| (format!("\"{}\"", p.gql_name), p.go_name.clone()))
                .collect();
            write_literal_entries(out, "\t\t", &entries);
            out.push_str("\t}\n");
        }
        for p in &optional {
            let _ = writeln!(out, "\tif {} != nil {{", p.go_name);
            let _ = writeln!(out, "\t\tvariables[\"{}\"] = {}", p.gql_name, p.go_name);
            out.push_str("\t}\n");
        }
        "variables".to_string()
    };

    let _ = writeln!(
        out,
        "\tif err := c.Request({}Document, {variables}, &data); err != nil {{",
        go_param_name(name)
    );
    out.push_str("\t\treturn data.Result, err\n\t}\n");
    out.push_str("\treturn data.Result, nil\n}\n");
}

// =============================================================================
// Small shared helpers
// =============================================================================

/// Resolve a schema type name to its Go type (scalars mapped, else name).
fn type_name_to_go(name: &str) -> String {
    named_scalar_go(name).map_or_else(|| name.to_string(), str::to_string)
}

/// Emit one author-supplied description as a single-line `//` comment.
///
/// Comments (not string literals) carry descriptions everywhere so schema-author
/// text can never escape into code: a `//` comment ends only at a newline, and
/// newlines are collapsed to spaces here.
fn doc_comment(description: &str) -> String {
    format!("// {}", description.replace('\n', " "))
}

/// Emit a declaration's doc comment, opening with the identifier as Go
/// convention (and `go doc`) expects.
fn push_doc(out: &mut String, ident: &str, description: Option<&str>) {
    if let Some(desc) = description {
        let _ = writeln!(out, "// {ident} — {}", desc.replace('\n', " "));
    }
}

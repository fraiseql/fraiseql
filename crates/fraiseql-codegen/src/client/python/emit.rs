//! Per-file Python emitters. Each `pub(super)` function returns the body of one
//! generated module; orchestration and header stamping live in the parent.
//!
//! The `GraphQL` documents come from `client::common` and are byte-identical to
//! the `TypeScript` generator's; only the type rendering here is Python-specific.

use std::{collections::BTreeSet, fmt::Write as _};

use fraiseql_core::schema::{
    ArgumentDefinition, CompiledSchema, EnumDefinition, FieldDefinition, InputObjectDefinition,
    InterfaceDefinition, MutationDefinition, QueryDefinition, TypeDefinition, UnionDefinition,
};

use super::{
    Ctx,
    render::{
        custom_scalar_name, field_type_py, field_type_py_nullable, is_py_keyword, named_scalar_py,
        parse_input_type, py_param_name,
    },
};
use crate::client::common::{
    self, const_name, finish, input_base_name, leaf_fields, referenced_named_type,
    selection_for_return,
};

const RELAY_HELPERS: &str = "class PageInfo(TypedDict):\n    hasNextPage: bool\n    hasPreviousPage: bool\n    startCursor: str | None\n    endCursor: str | None\n\n\nclass Edge[T](TypedDict):\n    cursor: str\n    node: T\n\n\nclass Connection[T](TypedDict):\n    edges: list[Edge[T]]\n    pageInfo: PageInfo\n    totalCount: NotRequired[int]\n";

// =============================================================================
// types.py
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
    out.push_str("from __future__ import annotations\n\n");
    out.push_str("from typing import Any, Literal, NotRequired, TypedDict\n");
    push_imports(&mut out, &render_imports(ctx, &refs, "types"));
    out.push('\n');

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

/// One `(name, python_type)` row of a `TypedDict`, plus its comment lines.
struct DictField {
    comments: Vec<String>,
    name:     String,
    py:       String,
}

/// Emit a `TypedDict` from prepared rows, choosing the class or functional
/// syntax: class syntax normally, functional (`X = TypedDict("X", {...})`) when
/// any field name is a Python keyword — the class form would be a syntax error.
fn emit_typed_dict(out: &mut String, name: &str, fields: &[DictField]) {
    let needs_functional = fields.iter().any(|f| is_py_keyword(&f.name));
    if needs_functional {
        let _ = writeln!(out, "{name} = TypedDict(\n    \"{name}\",\n    {{");
        for field in fields {
            for comment in &field.comments {
                let _ = writeln!(out, "        {comment}");
            }
            let _ = writeln!(out, "        \"{}\": {},", field.name, field.py);
        }
        out.push_str("    },\n)\n");
        return;
    }

    let _ = writeln!(out, "class {name}(TypedDict):");
    if fields.is_empty() {
        out.push_str("    pass\n");
        return;
    }
    for field in fields {
        for comment in &field.comments {
            let _ = writeln!(out, "    {comment}");
        }
        let _ = writeln!(out, "    {}: {}", field.name, field.py);
    }
}

fn leaf_dict_fields(fields: &[FieldDefinition]) -> Vec<DictField> {
    leaf_fields(fields)
        .into_iter()
        .map(|field| {
            let mut comments = Vec::new();
            if let Some(desc) = field.description.as_deref() {
                comments.push(doc_comment(desc));
            }
            if let Some(scalar) = custom_scalar_name(&field.field_type) {
                comments.push(format!("# TODO: brand custom scalar `{scalar}`"));
            }
            DictField {
                comments,
                name: field.name.to_string(),
                py: field_type_py_nullable(&field.field_type, field.nullable),
            }
        })
        .collect()
}

fn emit_interface(out: &mut String, iface: &InterfaceDefinition) {
    push_doc(out, iface.description.as_deref());
    let mut fields = vec![DictField {
        comments: Vec::new(),
        name:     "__typename".to_string(),
        py:       "str".to_string(),
    }];
    fields.extend(leaf_dict_fields(&iface.fields));
    emit_typed_dict(out, &iface.name, &fields);
}

/// Objects do not inherit their interfaces (unlike the TS generator's
/// `extends`): a `TypedDict` subclass may not narrow an inherited field, and
/// `__typename` narrows from `str` to a literal. Fields are emitted flat.
fn emit_object(out: &mut String, ctx: &Ctx, ty: &TypeDefinition) {
    push_doc(out, ty.description.as_deref());
    let name = ty.name.as_str();
    let mut fields = vec![DictField {
        comments: Vec::new(),
        name:     "__typename".to_string(),
        py:       format!("Literal[\"{name}\"]"),
    }];
    if ctx.error_typenames.contains(name) {
        fields.push(DictField {
            comments: vec![
                "# Error class injected by the mutation runtime (the `error_class`).".to_string(),
            ],
            name:     "status".to_string(),
            py:       "str".to_string(),
        });
    }
    fields.extend(leaf_dict_fields(&ty.fields));
    emit_typed_dict(out, name, &fields);
}

fn emit_union(out: &mut String, union: &UnionDefinition) {
    push_doc(out, union.description.as_deref());
    let members = if union.member_types.is_empty() {
        "Any".to_string()
    } else {
        union.member_types.join(" | ")
    };
    let _ = writeln!(out, "type {} = {members}", union.name);
}

// =============================================================================
// enums.py
// =============================================================================

pub(super) fn enums(schema: &CompiledSchema) -> String {
    let mut out = String::new();
    out.push_str("from __future__ import annotations\n\nfrom typing import Any, Literal\n\n");
    for def in &schema.enums {
        emit_enum(&mut out, def);
        out.push('\n');
    }
    finish(out)
}

fn emit_enum(out: &mut String, def: &EnumDefinition) {
    push_doc(out, def.description.as_deref());
    let members = if def.values.is_empty() {
        "Any".to_string()
    } else {
        def.values
            .iter()
            .map(|v| format!("\"{}\"", v.name))
            .collect::<Vec<_>>()
            .join(", ")
    };
    if def.values.is_empty() {
        let _ = writeln!(out, "type {} = {members}", def.name);
    } else {
        let _ = writeln!(out, "type {} = Literal[{members}]", def.name);
    }
}

// =============================================================================
// inputs.py
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
    out.push_str("from __future__ import annotations\n\n");
    out.push_str("from typing import Any, NotRequired, TypedDict\n");
    push_imports(&mut out, &render_imports(ctx, &refs, "inputs"));
    out.push('\n');
    for input in &schema.input_types {
        emit_input(&mut out, input);
        out.push('\n');
    }
    finish(out)
}

fn emit_input(out: &mut String, input: &InputObjectDefinition) {
    push_doc(out, input.description.as_deref());
    let fields: Vec<DictField> = input
        .fields
        .iter()
        .map(|field| {
            let parsed = parse_input_type(&field.field_type);
            let py = if parsed.required {
                parsed.py
            } else {
                format!("NotRequired[{} | None]", parsed.py)
            };
            DictField {
                comments: field.description.as_deref().map(doc_comment).into_iter().collect(),
                name: field.name.clone(),
                py,
            }
        })
        .collect();
    emit_typed_dict(out, &input.name, &fields);
}

// =============================================================================
// queries.py
// =============================================================================

pub(super) fn queries(ctx: &Ctx) -> String {
    let schema = ctx.schema;
    // Collect the referenced names from the arguments the emitter actually
    // *renders* — `graphql_arguments`, not `q.arguments`. The auto-wired
    // `where`/`orderBy` are absent from `q.arguments` by design, and since
    // #1154 they are named input types rather than `JSON`, so collecting from
    // the narrower list emits a document that references a type it never
    // imports and the generated client does not compile.
    let rendered: Vec<Vec<ArgumentDefinition>> =
        schema.queries.iter().map(|q| q.graphql_arguments(schema)).collect();

    let mut refs: BTreeSet<&str> = BTreeSet::new();
    for (q, arguments) in schema.queries.iter().zip(&rendered) {
        refs.insert(&q.return_type);
        for arg in arguments {
            if let Some(name) = referenced_named_type(&arg.arg_type) {
                refs.insert(name);
            }
        }
        if q.relay {
            refs.insert("Connection");
        }
    }

    let mut out = String::new();
    out.push_str("from __future__ import annotations\n\n");
    out.push_str("from typing import Any, cast\n\n");
    out.push_str("from .client import FraiseqlClient, omit_none\n");
    push_imports(&mut out, &render_imports(ctx, &refs, "queries"));
    out.push('\n');

    for q in &schema.queries {
        emit_query(&mut out, ctx, q);
        out.push('\n');
    }
    finish(out)
}

fn emit_query(out: &mut String, ctx: &Ctx, q: &QueryDefinition) {
    // Render the auto-wired `where`/`orderBy`/`limit`/`offset` arguments derived
    // from `auto_params` so the generated query can paginate and filter.
    let arguments = q.graphql_arguments(ctx.schema);
    let op = build_operation(&arguments, q.relay);
    let selection = selection_for_return(ctx, &q.return_type, q.relay);
    let document = common::render_document("query", &q.name, &op.gql, &selection);
    let result = query_result_py(q);

    push_doc(out, q.description.as_deref());
    let _ = writeln!(out, "_{} = \"\"\"{document}\"\"\"\n\n", const_name(&q.name));
    emit_operation_fn(out, &q.name, &op, &result);
}

fn query_result_py(q: &QueryDefinition) -> String {
    let node = type_name_to_py(&q.return_type);
    if q.relay {
        return format!("Connection[{node}]");
    }
    let base = if q.returns_list {
        format!("list[{node}]")
    } else {
        node
    };
    if q.nullable {
        format!("{base} | None")
    } else {
        base
    }
}

// =============================================================================
// mutations.py
// =============================================================================

pub(super) fn mutations(ctx: &Ctx) -> String {
    let schema = ctx.schema;
    let mut refs: BTreeSet<&str> = BTreeSet::new();
    for m in &schema.mutations {
        refs.insert(&m.return_type);
        for arg in &m.arguments {
            if let Some(name) = referenced_named_type(&arg.arg_type) {
                refs.insert(name);
            }
        }
    }

    let mut out = String::new();
    out.push_str("from __future__ import annotations\n\n");
    if ctx.error_typenames.is_empty() {
        out.push_str("from typing import Any, cast\n\n");
    } else {
        out.push_str("from collections.abc import Mapping\n");
        out.push_str("from typing import Any, Literal, cast\n\n");
    }
    out.push_str("from .client import FraiseqlClient, omit_none\n");
    push_imports(&mut out, &render_imports(ctx, &refs, "mutations"));
    out.push('\n');

    emit_error_guard(&mut out, ctx);

    for m in &schema.mutations {
        emit_mutation(&mut out, ctx, m);
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
    let _ = writeln!(out, "type ErrorTypename = Literal[{literals}]\n");
    let _ = writeln!(out, "ERROR_TYPENAMES: frozenset[str] = frozenset({{{literals}}})\n\n");
    out.push_str("def is_error_result(value: Mapping[str, Any]) -> bool:\n");
    out.push_str(
        "    \"\"\"Whether a mutation result is one of the schema's typed-error members.\"\"\"\n",
    );
    out.push_str("    return value.get(\"__typename\") in ERROR_TYPENAMES\n\n\n");
}

fn emit_mutation(out: &mut String, ctx: &Ctx, m: &MutationDefinition) {
    let op = build_operation(&m.arguments, false);
    let selection = selection_for_return(ctx, &m.return_type, false);
    let document = common::render_document("mutation", &m.name, &op.gql, &selection);
    let result = type_name_to_py(&m.return_type);

    push_doc(out, m.description.as_deref());
    let _ = writeln!(out, "_{} = \"\"\"{document}\"\"\"\n\n", const_name(&m.name));
    emit_operation_fn(out, &m.name, &op, &result);
}

// =============================================================================
// relationships.py
// =============================================================================

pub(super) fn relationships(schema: &CompiledSchema) -> String {
    let mut out = String::new();
    out.push_str("from __future__ import annotations\n\n");
    out.push_str("from typing import Literal, TypedDict\n\n");
    out.push_str(
        "type RelationshipCardinality = Literal[\"oneToMany\", \"manyToOne\", \"oneToOne\"]\n\n\n",
    );
    out.push_str("class RelationshipMeta(TypedDict):\n");
    out.push_str("    targetType: str\n");
    out.push_str("    cardinality: RelationshipCardinality\n");
    out.push_str("    foreignKey: str\n");
    out.push_str("    referencedKey: str\n\n\n");
    out.push_str("RELATIONSHIPS: dict[str, dict[str, RelationshipMeta]] = {\n");
    for ty in &schema.types {
        if ty.relationships.is_empty() {
            continue;
        }
        let _ = writeln!(out, "    \"{}\": {{", ty.name);
        for rel in &ty.relationships {
            let card = cardinality_label(rel.cardinality);
            let _ = writeln!(
                out,
                "        \"{}\": {{\"targetType\": \"{}\", \"cardinality\": \"{card}\", \"foreignKey\": \"{}\", \"referencedKey\": \"{}\"}},",
                rel.name, rel.target_type, rel.foreign_key, rel.referenced_key
            );
        }
        out.push_str("    },\n");
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
// __init__.py
// =============================================================================

pub(super) fn init(modules: &[&str]) -> String {
    let mut out = String::new();
    for module in modules {
        let _ = writeln!(out, "from .{module} import *");
    }
    out
}

// =============================================================================
// Operation building (shared by queries & mutations)
// =============================================================================

/// A built operation: the shared `GraphQL` half plus the Python parameters.
struct Operation {
    gql:    common::GqlOperation,
    params: Vec<PyParam>,
}

struct PyParam {
    /// Original `GraphQL` variable name (the key sent on the wire).
    gql_name: String,
    /// Keyword-safe Python parameter name.
    py_name:  String,
    /// Python type expression (no nullability).
    py_type:  String,
    optional: bool,
}

fn build_operation(arguments: &[ArgumentDefinition], relay: bool) -> Operation {
    let gql = common::build_gql_operation(arguments, relay);
    let mut params = Vec::new();

    for arg in arguments {
        params.push(PyParam {
            gql_name: arg.name.clone(),
            py_name:  py_param_name(&arg.name),
            py_type:  field_type_py(&arg.arg_type),
            optional: arg.nullable,
        });
    }
    if relay {
        // Spec-standard forward pagination; both optional.
        params.push(PyParam {
            gql_name: "first".to_string(),
            py_name:  "first".to_string(),
            py_type:  "int".to_string(),
            optional: true,
        });
        params.push(PyParam {
            gql_name: "after".to_string(),
            py_name:  "after".to_string(),
            py_type:  "str".to_string(),
            optional: true,
        });
    }

    Operation { gql, params }
}

/// Emit the `def ...` wrapper that calls `client.request` and unwraps the root
/// field. Optional (nullable) arguments default to `None` and are omitted from
/// the request via `omit_none`.
fn emit_operation_fn(out: &mut String, name: &str, op: &Operation, result: &str) {
    let doc_const = format!("_{}", const_name(name));

    let _ = writeln!(out, "def {name}(");
    out.push_str("    client: FraiseqlClient,\n");
    if !op.params.is_empty() {
        out.push_str("    *,\n");
        for p in &op.params {
            if p.optional {
                let _ = writeln!(out, "    {}: {} | None = None,", p.py_name, p.py_type);
            } else {
                let _ = writeln!(out, "    {}: {},", p.py_name, p.py_type);
            }
        }
    }
    let _ = writeln!(out, ") -> {result}:");

    if op.params.is_empty() {
        let _ = writeln!(out, "    data = client.request({doc_const})");
    } else {
        let entries = op
            .params
            .iter()
            .map(|p| format!("\"{}\": {}", p.gql_name, p.py_name))
            .collect::<Vec<_>>()
            .join(", ");
        if op.params.iter().any(|p| p.optional) {
            let _ = writeln!(out, "    variables = omit_none({{{entries}}})");
        } else {
            let _ = writeln!(out, "    variables = {{{entries}}}");
        }
        let _ = writeln!(out, "    data = client.request({doc_const}, variables)");
    }
    let _ = writeln!(out, "    return cast(\"{result}\", data[\"{name}\"])");
}

// =============================================================================
// Small shared helpers
// =============================================================================

/// Resolve a schema type name to its Python type (scalars mapped, else name).
fn type_name_to_py(name: &str) -> String {
    named_scalar_py(name).map_or_else(|| name.to_string(), str::to_string)
}

/// Emit one author-supplied description as a single-line `#` comment.
///
/// Comments (not docstrings) carry descriptions everywhere so schema-author
/// text can never escape into code: a `#` comment ends only at a newline, and
/// newlines are collapsed to spaces here.
fn doc_comment(description: &str) -> String {
    format!("# {}", description.replace('\n', " "))
}

fn push_doc(out: &mut String, description: Option<&str>) {
    if let Some(desc) = description {
        let _ = writeln!(out, "{}", doc_comment(desc));
    }
}

fn push_imports(out: &mut String, imports: &str) {
    if !imports.is_empty() {
        out.push_str(imports);
    }
}

/// Render sorted `from .module import a, b` lines for the given names, grouped
/// by the generated module that defines each name. Names not defined in any
/// generated module (scalars) are dropped.
fn render_imports(ctx: &Ctx, names: &BTreeSet<&str>, current: &str) -> String {
    use std::collections::BTreeMap;

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
        let joined = idents.into_iter().collect::<Vec<_>>().join(", ");
        let _ = writeln!(out, "from .{module} import {joined}");
    }
    out
}

//! Per-file `TypeScript` emitters. Each `pub(super)` function returns the body of
//! one generated module; orchestration and header stamping live in the parent.

use std::{collections::BTreeSet, fmt::Write as _};

use fraiseql_core::schema::{
    ArgumentDefinition, CompiledSchema, EnumDefinition, FieldDefinition, InputObjectDefinition,
    InterfaceDefinition, MutationDefinition, QueryDefinition, TypeDefinition, UnionDefinition,
};

use super::{
    Ctx, input_base_name, leaf_fields, referenced_named_type,
    render::{
        arg_graphql_type, custom_scalar_name, field_type_ts, field_type_ts_nullable,
        named_scalar_ts, parse_input_type,
    },
    render_imports,
};

const RELAY_HELPERS: &str = "export interface PageInfo {\n  hasNextPage: boolean;\n  hasPreviousPage: boolean;\n  startCursor: string | null;\n  endCursor: string | null;\n}\n\nexport interface Edge<T> {\n  cursor: string;\n  node: T;\n}\n\nexport interface Connection<T> {\n  edges: Edge<T>[];\n  pageInfo: PageInfo;\n  totalCount?: number;\n}\n";

// =============================================================================
// types.ts
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
    push_imports(&mut out, &render_imports(ctx, &refs, "types"));

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

fn emit_interface(out: &mut String, iface: &InterfaceDefinition) {
    push_doc(out, "", iface.description.as_deref());
    let _ = writeln!(out, "export interface {} {{", iface.name);
    out.push_str("  __typename: string;\n");
    for field in leaf_fields(&iface.fields) {
        emit_field_line(out, field);
    }
    out.push_str("}\n");
}

fn emit_object(out: &mut String, ctx: &Ctx, ty: &TypeDefinition) {
    push_doc(out, "", ty.description.as_deref());
    let name = ty.name.as_str();
    let extends = if ty.implements.is_empty() {
        String::new()
    } else {
        format!(" extends {}", ty.implements.join(", "))
    };
    let _ = writeln!(out, "export interface {name}{extends} {{");
    let _ = writeln!(out, "  __typename: \"{name}\";");
    if ctx.error_typenames.contains(name) {
        out.push_str(
            "  /** Error class injected by the mutation runtime (the `error_class`). */\n",
        );
        out.push_str("  status: string;\n");
    }
    for field in leaf_fields(&ty.fields) {
        emit_field_line(out, field);
    }
    out.push_str("}\n");
}

fn emit_field_line(out: &mut String, field: &FieldDefinition) {
    push_doc(out, "  ", field.description.as_deref());
    if let Some(scalar) = custom_scalar_name(&field.field_type) {
        let _ = writeln!(out, "  // TODO: brand custom scalar `{scalar}`");
    }
    let ts = field_type_ts_nullable(&field.field_type, field.nullable);
    let _ = writeln!(out, "  {}: {ts};", field.name);
}

fn emit_union(out: &mut String, union: &UnionDefinition) {
    push_doc(out, "", union.description.as_deref());
    let members = if union.member_types.is_empty() {
        "never".to_string()
    } else {
        union.member_types.join(" | ")
    };
    let _ = writeln!(out, "export type {} = {members};", union.name);
}

// =============================================================================
// enums.ts
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
    push_doc(out, "", def.description.as_deref());
    let members = if def.values.is_empty() {
        "never".to_string()
    } else {
        def.values
            .iter()
            .map(|v| format!("\"{}\"", v.name))
            .collect::<Vec<_>>()
            .join(" | ")
    };
    let _ = writeln!(out, "export type {} = {members};", def.name);
}

// =============================================================================
// inputs.ts
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
    push_imports(&mut out, &render_imports(ctx, &refs, "inputs"));
    for input in &schema.input_types {
        emit_input(&mut out, input);
        out.push('\n');
    }
    finish(out)
}

fn emit_input(out: &mut String, input: &InputObjectDefinition) {
    push_doc(out, "", input.description.as_deref());
    let _ = writeln!(out, "export interface {} {{", input.name);
    for field in &input.fields {
        push_doc(out, "  ", field.description.as_deref());
        let parsed = parse_input_type(&field.field_type);
        if parsed.required {
            let _ = writeln!(out, "  {}: {};", field.name, parsed.ts);
        } else {
            let _ = writeln!(out, "  {}?: {} | null;", field.name, parsed.ts);
        }
    }
    out.push_str("}\n");
}

// =============================================================================
// queries.ts
// =============================================================================

pub(super) fn queries(ctx: &Ctx) -> String {
    let schema = ctx.schema;
    let mut refs: BTreeSet<&str> = BTreeSet::new();
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
    }

    let mut out = String::new();
    out.push_str("import { FraiseqlClient } from \"./client\";\n");
    push_imports(&mut out, &render_imports(ctx, &refs, "queries"));

    for q in &schema.queries {
        emit_query(&mut out, ctx, q);
        out.push('\n');
    }
    finish(out)
}

fn emit_query(out: &mut String, ctx: &Ctx, q: &QueryDefinition) {
    let op = build_operation(&q.arguments, q.relay);
    let selection = selection_for_return(ctx, &q.return_type, q.relay);
    let document = render_document("query", &q.name, &op, &selection);
    let result = query_result_ts(ctx, q);

    push_doc(out, "", q.description.as_deref());
    let _ = writeln!(out, "const {} = {document};\n", const_name(&q.name));
    emit_operation_fn(out, &q.name, &op, &result);
}

fn query_result_ts(_ctx: &Ctx, q: &QueryDefinition) -> String {
    let node = type_name_to_ts(&q.return_type);
    if q.relay {
        return format!("Connection<{node}>");
    }
    let base = if q.returns_list {
        format!("{node}[]")
    } else {
        node
    };
    if q.nullable {
        format!("{base} | null")
    } else {
        base
    }
}

// =============================================================================
// mutations.ts
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
    out.push_str("import { FraiseqlClient } from \"./client\";\n");
    push_imports(&mut out, &render_imports(ctx, &refs, "mutations"));

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
        .join(" | ");
    let set_items = ctx
        .error_typenames
        .iter()
        .map(|n| format!("\"{n}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(out, "export type ErrorTypename = {literals};\n");
    let _ = writeln!(
        out,
        "const ERROR_TYPENAMES: ReadonlySet<string> = new Set<string>([{set_items}]);\n"
    );
    out.push_str("/** Narrow a mutation result union to its typed-error members. */\n");
    out.push_str("export function isErrorResult<T extends { __typename: string }>(\n");
    out.push_str("  value: T,\n");
    out.push_str("): value is Extract<T, { __typename: ErrorTypename }> {\n");
    out.push_str("  return ERROR_TYPENAMES.has(value.__typename);\n");
    out.push_str("}\n\n");
}

fn emit_mutation(out: &mut String, ctx: &Ctx, m: &MutationDefinition) {
    let op = build_operation(&m.arguments, false);
    let selection = selection_for_return(ctx, &m.return_type, false);
    let document = render_document("mutation", &m.name, &op, &selection);
    let result = type_name_to_ts(&m.return_type);

    push_doc(out, "", m.description.as_deref());
    let _ = writeln!(out, "const {} = {document};\n", const_name(&m.name));
    emit_operation_fn(out, &m.name, &op, &result);
}

// =============================================================================
// relationships.ts
// =============================================================================

pub(super) fn relationships(schema: &CompiledSchema) -> String {
    let mut out = String::new();
    out.push_str(
        "export type RelationshipCardinality = \"oneToMany\" | \"manyToOne\" | \"oneToOne\";\n\n",
    );
    out.push_str("export interface RelationshipMeta {\n");
    out.push_str("  targetType: string;\n");
    out.push_str("  cardinality: RelationshipCardinality;\n");
    out.push_str("  foreignKey: string;\n");
    out.push_str("  referencedKey: string;\n");
    out.push_str("}\n\n");
    out.push_str("export const relationships = {\n");
    for ty in &schema.types {
        if ty.relationships.is_empty() {
            continue;
        }
        let _ = writeln!(out, "  {}: {{", ty.name);
        for rel in &ty.relationships {
            let card = cardinality_ts(rel.cardinality);
            let _ = writeln!(
                out,
                "    {}: {{ targetType: \"{}\", cardinality: \"{card}\", foreignKey: \"{}\", referencedKey: \"{}\" }},",
                rel.name, rel.target_type, rel.foreign_key, rel.referenced_key
            );
        }
        out.push_str("  },\n");
    }
    out.push_str("} as const;\n\n");
    out.push_str("export type EntityRelationships = typeof relationships;\n");
    finish(out)
}

const fn cardinality_ts(card: fraiseql_core::schema::Cardinality) -> &'static str {
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
// index.ts
// =============================================================================

pub(super) fn index(modules: &[&str]) -> String {
    let mut out = String::new();
    for module in modules {
        let _ = writeln!(out, "export * from \"./{module}\";");
    }
    out
}

// =============================================================================
// Operation building (shared by queries & mutations)
// =============================================================================

/// A built operation: `GraphQL` variable declarations, the field-call arguments,
/// and the `TypeScript` `variables` object fields.
struct Operation {
    var_decls:    Vec<String>, // e.g. "$id: ID!"
    call_args:    Vec<String>, // e.g. "id: $id"
    ts_fields:    Vec<String>, // e.g. "id: string" / "first?: number"
    all_optional: bool,
}

fn build_operation(arguments: &[ArgumentDefinition], relay: bool) -> Operation {
    let mut var_decls = Vec::new();
    let mut call_args = Vec::new();
    let mut ts_fields = Vec::new();
    let mut all_optional = true;

    for arg in arguments {
        let name = &arg.name;
        var_decls.push(format!("${name}: {}", arg_graphql_type(&arg.arg_type, arg.nullable)));
        call_args.push(format!("{name}: ${name}"));
        let ts = field_type_ts(&arg.arg_type);
        if arg.nullable {
            ts_fields.push(format!("{name}?: {ts} | null"));
        } else {
            ts_fields.push(format!("{name}: {ts}"));
            all_optional = false;
        }
    }

    if relay {
        // Spec-standard forward pagination; both optional.
        var_decls.push("$first: Int".to_string());
        var_decls.push("$after: String".to_string());
        call_args.push("first: $first".to_string());
        call_args.push("after: $after".to_string());
        ts_fields.push("first?: number".to_string());
        ts_fields.push("after?: string".to_string());
    }

    Operation {
        var_decls,
        call_args,
        ts_fields,
        all_optional,
    }
}

/// Build the `query`/`mutation` document string (a backtick template literal).
fn render_document(kind: &str, name: &str, op: &Operation, selection: &str) -> String {
    let var_sig = if op.var_decls.is_empty() {
        String::new()
    } else {
        format!("({})", op.var_decls.join(", "))
    };
    let call_sig = if op.call_args.is_empty() {
        String::new()
    } else {
        format!("({})", op.call_args.join(", "))
    };

    let mut doc = format!("`{kind} {name}{var_sig} {{\n  {name}{call_sig} {{\n");
    doc.push_str(selection);
    doc.push_str("  }\n}`");
    doc
}

/// Emit the `export async function ...` wrapper that calls `client.request`.
fn emit_operation_fn(out: &mut String, name: &str, op: &Operation, result: &str) {
    let has_vars = !op.ts_fields.is_empty();
    let const_doc = const_name(name);

    out.push_str("export async function ");
    out.push_str(name);
    out.push_str("(\n  client: FraiseqlClient,\n");
    if has_vars {
        let fields = op.ts_fields.join("; ");
        if op.all_optional {
            let _ = writeln!(out, "  variables: {{ {fields} }} = {{}},");
        } else {
            let _ = writeln!(out, "  variables: {{ {fields} }},");
        }
    }
    let _ = writeln!(out, "): Promise<{result}> {{");

    let call = if has_vars {
        format!("{const_doc}, variables")
    } else {
        const_doc
    };
    let _ = writeln!(out, "  const data = await client.request<{{ {name}: {result} }}>({call});");
    let _ = writeln!(out, "  return data.{name};");
    out.push_str("}\n");
}

// =============================================================================
// Selection sets
// =============================================================================

/// Build the indented selection-set lines for an operation's return type.
///
/// For relay queries the node selection is wrapped in the connection shape; for
/// union return types inline fragments are emitted per member.
fn selection_for_return(ctx: &Ctx, return_type: &str, relay: bool) -> String {
    if relay {
        let mut sel = String::new();
        sel.push_str("    edges {\n      cursor\n      node {\n");
        sel.push_str(&type_selection(ctx, return_type, "        "));
        sel.push_str("      }\n    }\n");
        sel.push_str("    pageInfo {\n      hasNextPage\n      hasPreviousPage\n      startCursor\n      endCursor\n    }\n");
        return sel;
    }
    type_selection(ctx, return_type, "    ")
}

/// Selection-set lines for a type name (object, union, or — degenerate — scalar).
fn type_selection(ctx: &Ctx, type_name: &str, indent: &str) -> String {
    let mut sel = String::new();
    let _ = writeln!(sel, "{indent}__typename");

    if let Some(union) = ctx.unions.get(type_name) {
        for member in &union.member_types {
            let _ = writeln!(sel, "{indent}... on {member} {{");
            sel.push_str(&leaf_name_lines(ctx, member, &format!("{indent}  ")));
            let _ = writeln!(sel, "{indent}}}");
        }
    } else {
        sel.push_str(&leaf_name_lines(ctx, type_name, indent));
    }
    sel
}

/// The leaf field names of an object type, one indented line each.
fn leaf_name_lines(ctx: &Ctx, type_name: &str, indent: &str) -> String {
    let mut out = String::new();
    if let Some(ty) = ctx.object_types.get(type_name) {
        for field in leaf_fields(&ty.fields) {
            let _ = writeln!(out, "{indent}{}", field.name);
        }
    }
    out
}

// =============================================================================
// Small shared helpers
// =============================================================================

/// Resolve a schema type name to its `TypeScript` type (scalars mapped, else name).
fn type_name_to_ts(name: &str) -> String {
    named_scalar_ts(name).map_or_else(|| name.to_string(), str::to_string)
}

/// `getUser` → `GET_USER`, `postsConnection` → `POSTS_CONNECTION`.
fn const_name(name: &str) -> String {
    let mut out = String::new();
    let mut prev_lower = false;
    for ch in name.chars() {
        if ch == '_' {
            out.push('_');
            prev_lower = false;
        } else if ch.is_ascii_uppercase() {
            if prev_lower {
                out.push('_');
            }
            out.push(ch);
            prev_lower = false;
        } else {
            out.push(ch.to_ascii_uppercase());
            prev_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        }
    }
    out
}

fn push_doc(out: &mut String, indent: &str, description: Option<&str>) {
    if let Some(desc) = description {
        let one_line = desc.replace('\n', " ");
        let _ = writeln!(out, "{indent}/** {one_line} */");
    }
}

fn push_imports(out: &mut String, imports: &str) {
    if !imports.is_empty() {
        out.push_str(imports);
        out.push('\n');
    }
}

/// Normalize trailing whitespace: collapse to a single trailing newline.
fn finish(mut out: String) -> String {
    while out.ends_with("\n\n") {
        out.pop();
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

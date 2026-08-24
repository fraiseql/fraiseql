//! Language-independent client-generation machinery shared by every target
//! language: schema lookups, `GraphQL` document building, and selection sets.
//!
//! The generated **documents** (operation signatures, field calls, selection
//! sets) are pure `GraphQL` and must be byte-identical across target languages —
//! centralising them here is what prevents a second language from drifting into
//! subtly different queries. Only the *type* rendering (TS `string` vs Python
//! `str`) belongs in the per-language emitters.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
};

use fraiseql_core::schema::{
    ArgumentDefinition, CompiledSchema, FieldDefinition, FieldType, InterfaceDefinition,
    TypeDefinition, UnionDefinition,
};

/// Resolved lookups over a [`CompiledSchema`], shared by all language emitters.
pub(super) struct SchemaCtx<'a> {
    pub(super) schema:            &'a CompiledSchema,
    pub(super) object_types:      BTreeMap<&'a str, &'a TypeDefinition>,
    pub(super) interfaces:        BTreeMap<&'a str, &'a InterfaceDefinition>,
    pub(super) unions:            BTreeMap<&'a str, &'a UnionDefinition>,
    pub(super) enum_names:        BTreeSet<&'a str>,
    pub(super) input_names:       BTreeSet<&'a str>,
    pub(super) error_typenames:   BTreeSet<&'a str>,
    pub(super) has_relay:         bool,
    pub(super) has_relationships: bool,
}

impl<'a> SchemaCtx<'a> {
    pub(super) fn new(schema: &'a CompiledSchema) -> Self {
        let object_types = schema.types.iter().map(|t| (t.name.as_str(), t)).collect();
        let interfaces = schema.interfaces.iter().map(|i| (i.name.as_str(), i)).collect();
        let unions = schema.unions.iter().map(|u| (u.name.as_str(), u)).collect();
        let enum_names = schema.enums.iter().map(|e| e.name.as_str()).collect();
        let input_names = schema.input_types.iter().map(|i| i.name.as_str()).collect();
        let error_typenames =
            schema.types.iter().filter(|t| t.is_error).map(|t| t.name.as_str()).collect();
        let has_relay = schema.queries.iter().any(|q| q.relay);
        let has_relationships = schema.types.iter().any(|t| !t.relationships.is_empty());

        Self {
            schema,
            object_types,
            interfaces,
            unions,
            enum_names,
            input_names,
            error_typenames,
            has_relay,
            has_relationships,
        }
    }

    /// Which generated module defines a referenced type name, if any.
    ///
    /// Module names are language-neutral (`types`, `enums`, `inputs`); each
    /// language maps them onto its own file/import syntax.
    pub(super) fn module_of(&self, name: &str) -> Option<&'static str> {
        if self.enum_names.contains(name) {
            Some("enums")
        } else if self.input_names.contains(name) {
            Some("inputs")
        } else if matches!(name, "Connection" | "Edge" | "PageInfo")
            || self.object_types.contains_key(name)
            || self.unions.contains_key(name)
            || self.interfaces.contains_key(name)
        {
            Some("types")
        } else {
            None
        }
    }
}

/// Leaf fields of a type, in declaration order (those the default document fetches).
pub(super) fn leaf_fields(fields: &[FieldDefinition]) -> Vec<&FieldDefinition> {
    fields.iter().filter(|f| is_leaf(&f.field_type)).collect()
}

/// Whether a field is a `GraphQL` **leaf** (selectable without a sub-selection).
///
/// Scalars, enums, and lists-of-leaf are leaves and are fetched by the default
/// document; object/interface/union references (and lists thereof) are composite
/// and are omitted in v1 (see `DESIGN-CLIENT-TS.md` §2).
pub(super) fn is_leaf(ft: &FieldType) -> bool {
    match ft {
        FieldType::Object(_) | FieldType::Interface(_) | FieldType::Union(_) => false,
        FieldType::List(inner) => is_leaf(inner),
        _ => true,
    }
}

/// The innermost named type a [`FieldType`] references (recursing into lists).
pub(super) fn referenced_named_type(ft: &FieldType) -> Option<&str> {
    match ft {
        FieldType::Enum(n)
        | FieldType::Object(n)
        | FieldType::Input(n)
        | FieldType::Interface(n)
        | FieldType::Union(n) => Some(n),
        FieldType::List(inner) => referenced_named_type(inner),
        _ => None,
    }
}

/// The base named type of an input-field `GraphQL` type **string** (strips `[]!`).
pub(super) fn input_base_name(type_str: &str) -> String {
    type_str.chars().filter(|c| !matches!(c, '[' | ']' | '!' | ' ')).collect()
}

/// Render a [`FieldType`] argument as a `GraphQL` type reference for a variable
/// declaration, e.g. `ID!`, `UserFilter`, `[String]`.
///
/// Requiredness is applied to the **undecorated** base rather than appended to
/// whatever [`FieldType::to_graphql_string`] returned, because a few types come
/// back already decorated: `Vector` and `HalfVector` both render as the
/// fully-formed `[Float!]!`. Appending to that produced `[Float!]!!`, which no
/// `GraphQL` parser accepts, so every call to a vector-argument operation died
/// at the server's parse step; the nullable branch was wrong the other way,
/// declaring `[Float!]!` for a variable the generated wrapper left optional
/// (#1066).
pub(super) fn arg_graphql_type(ft: &FieldType, nullable: bool) -> String {
    let base = ft.to_graphql_string();
    let undecorated = base.strip_suffix('!').unwrap_or(base.as_str());
    if nullable { undecorated.to_string() } else { format!("{undecorated}!") }
}

/// The `GraphQL` half of a built operation: variable declarations and the
/// field-call arguments. Language emitters derive their own typed parameter
/// lists from the same [`ArgumentDefinition`]s.
pub(super) struct GqlOperation {
    /// e.g. `$id: ID!`
    pub(super) var_decls: Vec<String>,
    /// e.g. `id: $id`
    pub(super) call_args: Vec<String>,
}

/// Build the `GraphQL` variable/call lists for an operation's arguments.
///
/// With `relay`, spec-standard forward pagination (`$first: Int`,
/// `$after: String`) is appended — both optional.
pub(super) fn build_gql_operation(arguments: &[ArgumentDefinition], relay: bool) -> GqlOperation {
    let mut var_decls = Vec::new();
    let mut call_args = Vec::new();

    for arg in arguments {
        let name = &arg.name;
        var_decls.push(format!("${name}: {}", arg_graphql_type(&arg.arg_type, arg.nullable)));
        call_args.push(format!("{name}: ${name}"));
    }
    if relay {
        var_decls.push("$first: Int".to_string());
        var_decls.push("$after: String".to_string());
        call_args.push("first: $first".to_string());
        call_args.push("after: $after".to_string());
    }

    GqlOperation {
        var_decls,
        call_args,
    }
}

/// Build the raw `query`/`mutation` document text (no language quoting).
pub(super) fn render_document(
    kind: &str,
    name: &str,
    op: &GqlOperation,
    selection: &str,
) -> String {
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

    let mut doc = format!("{kind} {name}{var_sig} {{\n  {name}{call_sig} {{\n");
    doc.push_str(selection);
    doc.push_str("  }\n}");
    doc
}

/// Build the indented selection-set lines for an operation's return type.
///
/// For relay queries the node selection is wrapped in the connection shape; for
/// union return types inline fragments are emitted per member.
pub(super) fn selection_for_return(ctx: &SchemaCtx, return_type: &str, relay: bool) -> String {
    if relay {
        let mut sel = String::new();
        sel.push_str("    edges {\n      cursor\n      node {\n");
        sel.push_str(&type_selection(ctx, return_type, "        "));
        sel.push_str("      }\n    }\n");
        sel.push_str(
            "    pageInfo {\n      hasNextPage\n      hasPreviousPage\n      startCursor\n      endCursor\n    }\n",
        );
        return sel;
    }
    type_selection(ctx, return_type, "    ")
}

/// Selection-set lines for a type name (object, union, or — degenerate — scalar).
pub(super) fn type_selection(ctx: &SchemaCtx, type_name: &str, indent: &str) -> String {
    let mut sel = String::new();
    let _ = writeln!(sel, "{indent}__typename");

    if let Some(union) = ctx.unions.get(type_name) {
        for member in &union.member_types {
            let inner = format!("{indent}  ");
            let _ = writeln!(sel, "{indent}... on {member} {{");

            // A member can contribute no leaf lines at all — its fields may be
            // composite-only, or the name may resolve to no type in the schema
            // (a typo, an interface, or a type never registered; nothing
            // validates union members). Writing the braces around nothing gave
            // `... on X {}`, and an empty selection set is a parse error, so
            // every call to the operation failed (#1032). `__typename` is the
            // one field every object type has, and it is what the generated
            // client narrows on regardless.
            let leaves = leaf_name_lines(ctx, member, &inner);
            if leaves.is_empty() {
                let _ = writeln!(sel, "{inner}__typename");
            } else {
                sel.push_str(&leaves);
            }

            let _ = writeln!(sel, "{indent}}}");
        }
    } else {
        sel.push_str(&leaf_name_lines(ctx, type_name, indent));
    }
    sel
}

/// The leaf field names of an object type, one indented line each.
fn leaf_name_lines(ctx: &SchemaCtx, type_name: &str, indent: &str) -> String {
    let mut out = String::new();
    if let Some(ty) = ctx.object_types.get(type_name) {
        for field in leaf_fields(&ty.fields) {
            let _ = writeln!(out, "{indent}{}", field.name);
        }
    }
    out
}

/// `getUser` → `GET_USER`, `postsConnection` → `POSTS_CONNECTION`.
pub(super) fn const_name(name: &str) -> String {
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

/// Normalize trailing whitespace: collapse to a single trailing newline.
pub(super) fn finish(mut out: String) -> String {
    while out.ends_with("\n\n") {
        out.pop();
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

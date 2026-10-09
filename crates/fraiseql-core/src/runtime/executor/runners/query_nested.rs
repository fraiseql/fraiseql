//! A GraphQL selection into a nested type, classified as a read of that type.
//!
//! `{ users { orders { margin } } }` serves `Order` documents out of the `users` view's
//! materialised `data`. Field-level RBAC used to classify the root's projection against
//! the root type and nothing else, so what a read of `Order` would have masked or refused
//! reached the response through any type that embeds it. Every level is classified here,
//! against its own type, before the read — so a `Reject` at any depth never reaches the
//! database — and masked afterwards at the keys the response carries it under.
//!
//! # By field, not by response key
//!
//! A selection's alias is the client's choice of output key, not another field:
//! `{ orders { m: margin } }` reads `margin`. The planner lists the root's projection by
//! response key, which is what the projector needs, and the classifier used to be handed
//! that list — so an aliased field matched no declared field and passed through
//! unclassified. Classification here takes the field name; masking takes the key.

use std::collections::{HashMap, HashSet};

use super::{super::context::ExecutorContext, query::QueryRunner};
use crate::{
    backend::{
        COMPOSED_DOCUMENT_KEY, COMPOSED_EMBEDS_KEY, ComposedEmbed, ComposedLevel, EmbedShape,
        EmbedSource, LevelKeys, WhereClause,
    },
    error::{FraiseQLError, Result},
    graphql::FieldSelection,
    runtime::{field_filter::FieldAccessResult, projection::effective_selections},
    schema::{
        CompiledSchema, FieldDefinition, FieldType, QueryDefinition, Relationship, TypeDefinition,
    },
    security::{ConstrainedPaths, RLSPolicy, SecurityContext, rls_policy::RlsTarget},
};

/// The #422 authorizer and the input it is asked with, for the nested levels of a
/// selection: each level is put to it as a read of its own type, once per request per path.
#[derive(Clone, Copy)]
pub(in super::super) struct LevelAuthz<'a> {
    authorizer: &'a dyn crate::security::Authorizer,
    input:      Option<&'a serde_json::Value>,
}

impl<'a> LevelAuthz<'a> {
    /// The configured authorizer, if there is one, asked with `input`.
    pub(in super::super) fn from_config(
        config: &'a crate::runtime::RuntimeConfig,
        input: Option<&'a serde_json::Value>,
    ) -> Option<Self> {
        config.authorizer.as_deref().map(|authorizer| Self { authorizer, input })
    }
}

/// What field-level RBAC decided for a whole selection tree.
pub(in super::super) struct SelectionAccess {
    /// The root level, in the shape the projector takes: the projection's response keys,
    /// and the masked ones among them.
    pub(super) root: FieldAccessResult,
    /// Every `(type, field)` the caller may not read and that masks, at any level.
    masked:          HashSet<(String, String)>,
}

impl SelectionAccess {
    /// Classify every level of `root_fields` against the type it selects from.
    ///
    /// `projection_keys` is the planner's projection — the root's response keys, in
    /// order — returned unchanged as [`Self::root`]'s `projected`.
    ///
    /// # Errors
    ///
    /// `FraiseQLError::Authorization` for a selected field, at any depth, that requires a
    /// scope the caller lacks and whose `on_deny` is `Reject`; for a nested level of a
    /// type whose read requires a role or an actor type the caller lacks; and for a nested
    /// level the #422 authorizer (`authz`) denies.
    pub(in super::super) fn classify(
        schema: &CompiledSchema,
        root_type: &str,
        root_fields: &[FieldSelection],
        projection_keys: Vec<String>,
        security_context: Option<&SecurityContext>,
        authz: Option<LevelAuthz<'_>>,
    ) -> Result<Self> {
        let mut masked = HashSet::new();
        let mut level = Level {
            schema,
            security_context,
            authz,
            asked: HashSet::new(),
        };
        classify_level(&mut level, root_type, root_fields, "", &mut masked)?;

        let masked_keys = effective_selections(root_fields, root_type, schema)
            .into_iter()
            .filter(|sel| masked.contains(&(root_type.to_string(), sel.name.clone())))
            .map(|sel| sel.response_key().to_string())
            .collect();
        Ok(Self {
            root: FieldAccessResult {
                projected: projection_keys,
                masked:    masked_keys,
            },
            masked,
        })
    }

    /// Classify every `(type, selections)` in `roots` as [`Self::classify`] classifies one,
    /// as one classification: a nested path shared by two roots is put to the authorizer
    /// once. [`Self::root`] is empty — a caller of this reads each root's masks through
    /// [`Self::masked_fields`].
    ///
    /// # Errors
    ///
    /// As [`Self::classify`], for any root.
    pub(in super::super) fn classify_roots(
        schema: &CompiledSchema,
        roots: &[(&str, &[FieldSelection])],
        security_context: Option<&SecurityContext>,
        authz: Option<LevelAuthz<'_>>,
    ) -> Result<Self> {
        let mut masked = HashSet::new();
        let mut level = Level {
            schema,
            security_context,
            authz,
            asked: HashSet::new(),
        };
        for (root_type, root_fields) in roots {
            classify_level(&mut level, root_type, root_fields, "", &mut masked)?;
        }
        Ok(Self {
            root: FieldAccessResult {
                projected: Vec::new(),
                masked:    Vec::new(),
            },
            masked,
        })
    }

    /// The fields of `type_name` this classification masks, by field name.
    pub(in super::super) fn masked_fields(&self, type_name: &str) -> Vec<String> {
        self.masked
            .iter()
            .filter(|(t, _)| t == type_name)
            .map(|(_, field)| field.clone())
            .collect()
    }

    /// Null every masked field of a projected result, at every level, under the key the
    /// response carries it.
    pub(in super::super) fn null_masked(
        &self,
        value: &mut serde_json::Value,
        root_type: &str,
        root_fields: &[FieldSelection],
        schema: &CompiledSchema,
    ) {
        if !self.masked.is_empty() {
            null_masked_at(value, root_type, root_fields, &self.masked, schema);
        }
    }
}

/// The object type a field holds — its element type for a list. An enum is not one: it is
/// a value of its field, with no fields or gates of its own.
pub(super) fn object_type_of(field_type: &FieldType) -> Option<&str> {
    match field_type.inner_type().unwrap_or(field_type) {
        FieldType::Object(name) | FieldType::Interface(name) | FieldType::Union(name) => Some(name),
        _ => None,
    }
}

/// What every level of one classification shares.
struct Level<'a> {
    schema:           &'a CompiledSchema,
    security_context: Option<&'a SecurityContext>,
    authz:            Option<LevelAuthz<'a>>,
    /// The levels already put to the authorizer, by parent type and path: `a: orders { id }
    /// b: orders { total }` is one read of `orders`, asked once. The parent type is part of
    /// the key because one classification can hold several roots (a write's payload), and
    /// the same path under two of them is two reads.
    asked:            HashSet<(String, String)>,
}

/// Classify one level's selections against `type_name`, then every level beneath it.
/// `path` is the field names from the root to this level, dot-separated (empty at the root).
fn classify_level(
    level_ctx: &mut Level<'_>,
    type_name: &str,
    selections: &[FieldSelection],
    path: &str,
    masked: &mut HashSet<(String, String)>,
) -> Result<()> {
    let schema = level_ctx.schema;
    let security_context = level_ctx.security_context;
    let level: Vec<&FieldSelection> = effective_selections(selections, type_name, schema)
        .into_iter()
        .filter(|sel| sel.name != "__typename")
        .collect();
    let names = level.iter().map(|sel| sel.name.clone()).collect();
    let access = super::super::support::security::classify_fields_for_read(
        schema,
        type_name,
        names,
        security_context,
    )?;
    masked.extend(access.masked.into_iter().map(|field| (type_name.to_string(), field)));

    let Some(type_def) = schema.find_type(type_name) else {
        return Ok(());
    };
    for sel in level {
        let child = type_def
            .fields
            .iter()
            .find(|f| f.name == sel.name)
            .and_then(|f| object_type_of(&f.field_type));
        if let Some(child) = child {
            enforce_level_read_gates(schema, type_name, &sel.name, child, security_context)?;
            let child_path = if path.is_empty() {
                sel.name.clone()
            } else {
                format!("{path}.{}", sel.name)
            };
            if let Some(authz) = level_ctx.authz {
                if level_ctx.asked.insert((type_name.to_string(), child_path.clone())) {
                    ask_level_authorizer(
                        authz,
                        schema,
                        type_name,
                        child,
                        &child_path,
                        security_context,
                    )?;
                }
            }
            classify_level(level_ctx, child, &sel.nested_fields, &child_path, masked)?;
        }
    }
    Ok(())
}

/// Put a nested level of `target` to the #422 authorizer, as a read of its own type: named
/// by the type's canonical list query — REST's spelling for the same level — or by the type
/// when it has none, with `nesting` saying where it sits.
fn ask_level_authorizer(
    authz: LevelAuthz<'_>,
    schema: &CompiledSchema,
    parent_type: &str,
    target: &str,
    path: &str,
    security_context: Option<&SecurityContext>,
) -> Result<()> {
    let name = super::query_composed::list_query_for_type(schema, target)
        .map_or(target, |q| q.name.as_str());
    let op = crate::security::AuthzOperation::nested(
        name,
        target,
        crate::security::AuthzNesting::new(parent_type, path),
    );
    crate::security::authorizer::enforce_authz(
        authz.authorizer,
        security_context,
        &[op],
        authz.input,
    )
}

/// Refuse a nested level of `target` unless the caller may read `target` at all: the
/// `requires_role` and `requires_actor` of the read it is gated as ([`own_read`]), and the
/// type's own `requires_role` — which #677 lowers onto every read of the type, and which a
/// type no query returns still declares.
///
/// A refusal, not a masked `null`: these gate reading the type, not one of its fields. And
/// a `403` rather than the root's enumeration-hiding "not found" — the caller named a field
/// of a type it may read, so the nested type's existence is not what the answer discloses.
fn enforce_level_read_gates(
    schema: &CompiledSchema,
    parent_type: &str,
    field: &str,
    target: &str,
    security_context: Option<&SecurityContext>,
) -> Result<()> {
    match type_read_refusal(schema, target, security_context) {
        None => Ok(()),
        Some(why) => Err(FraiseQLError::Authorization {
            message:  format!("'{parent_type}.{field}' reads '{target}', {why}"),
            action:   Some("read".to_string()),
            resource: Some(target.to_string()),
        }),
    }
}

/// Why reading `type_name` anywhere but its own root query is refused to
/// `security_context`, if it is: its own read's `requires_role` (else the type's) and its own
/// read's `requires_actor`. One rule for a nested level, a subscription or stream root, and a
/// payload served as a type other than its mutation's declared return — so a deployment that
/// gates a type's list query rather than the type is gated on every path that reads it.
pub(in super::super) fn type_read_refusal(
    schema: &CompiledSchema,
    type_name: &str,
    security_context: Option<&SecurityContext>,
) -> Option<&'static str> {
    let read = own_read(schema, type_name);
    let role = read
        .and_then(|q| q.requires_role.as_deref())
        .or_else(|| schema.find_type(type_name).and_then(|t| t.requires_role.as_deref()));
    if let Some(role) = role {
        if !security_context.is_some_and(|ctx| ctx.roles.iter().any(|r| r == role)) {
            return Some("whose read requires a role the request does not hold");
        }
    }
    let actor_refused = read.is_some_and(|read| {
        crate::security::actor_type::enforce_requires_actor(
            "Query",
            &read.name,
            &read.requires_actor,
            security_context,
        )
        .is_err()
    });
    actor_refused.then_some("whose read is restricted to actor types the request is not")
}

/// What a masked field reads as: `null`, or `[]` for a translations sibling, whose
/// `[LocalizedString!]!` has no `null` (#1523).
pub(in super::super) fn masked_value(
    type_def: Option<&crate::schema::TypeDefinition>,
    field: &str,
) -> serde_json::Value {
    if type_def.and_then(|t| t.translations_of(field)).is_some() {
        serde_json::Value::Array(Vec::new())
    } else {
        serde_json::Value::Null
    }
}

fn null_masked_at(
    value: &mut serde_json::Value,
    type_name: &str,
    selections: &[FieldSelection],
    masked: &HashSet<(String, String)>,
    schema: &CompiledSchema,
) {
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                null_masked_at(item, type_name, selections, masked, schema);
            }
        },
        serde_json::Value::Object(object) => {
            let type_def = schema.find_type(type_name);
            for sel in effective_selections(selections, type_name, schema) {
                let key = sel.response_key();
                if masked.contains(&(type_name.to_string(), sel.name.clone())) {
                    if let Some(slot) = object.get_mut(key) {
                        *slot = masked_value(type_def, &sel.name);
                    }
                    continue;
                }
                let child = type_def
                    .and_then(|t| t.fields.iter().find(|f| f.name == sel.name))
                    .and_then(|f| object_type_of(&f.field_type));
                if let (Some(child), Some(nested)) = (child, object.get_mut(key)) {
                    null_masked_at(nested, child, &sel.nested_fields, masked, schema);
                }
            }
        },
        _ => {},
    }
}

// ── Row security of a nested level ───────────────────────────────────────────
//
// A read of `Order` is filtered by `Order`'s RLS predicate and `inject_params`. The
// `Order` documents `v_user` embeds were filtered by `User`'s, which says nothing about
// which orders the principal may read: a principal-scoped policy is invisible to a view
// composed for no principal, and a tenant policy is only implied where the view's join
// carries the tenant. So a nested level whose type scopes its rows is read as a level
// of a composed statement (`fraiseql_db::ComposedLevel`) carrying that type's predicate,
// either over the documents the parent's view materialised or over the type's own view.
//
// Which of the two is decided per `(parent type, field)` when the executor is built, from
// the schema and the policy's declared paths (`RLSPolicy::constrained_paths`): evaluating
// a predicate over an embedded document is sound only when the document carries every key
// the predicate reads. A policy that does not say is joined through a declared
// relationship, and refused where there is none.

/// How a nested object field's rows are gated by its type's row security.
#[derive(Debug, Clone)]
pub(in super::super) enum NestedRowGate {
    /// The type's predicate reads only `paths`, every one a key the type declares: it is
    /// evaluated over the documents the parent's view embedded.
    Project {
        /// The type's read whose policy target and `inject_params` apply.
        query: String,
        /// The stored keys the predicate may read.
        paths: Vec<String>,
    },
    /// The type's rows are read from its own view, through the parent's relationship.
    Join {
        /// The type's list query: its view, policy target and `inject_params`.
        query:        String,
        /// That query's view.
        view:         String,
        /// The parent's relationship the field is declared as.
        relationship: Relationship,
    },
    /// Neither is possible: a predicate for the type refuses the selection.
    Refuse {
        /// The type's read whose policy target and `inject_params` apply.
        query: String,
    },
}

impl NestedRowGate {
    /// The type's read whose policy target and `inject_params` apply.
    fn query(&self) -> &str {
        match self {
            Self::Project { query, .. } | Self::Join { query, .. } | Self::Refuse { query } => {
                query
            },
        }
    }
}

/// Every nested object field whose rows its type scopes, and how — built once, with the
/// executor. A field absent from it is ungated: its type has no SQL-backed read of its
/// own (a value object, part of its parent's row and gated by its parent's predicate), or
/// no policy and no `inject_params` scope that read.
#[derive(Debug, Default)]
pub(in super::super) struct NestedRowGates {
    by_field: HashMap<(String, String), NestedRowGate>,
}

impl NestedRowGates {
    /// Classify every object field of every type in `schema`.
    pub(in super::super) fn build(schema: &CompiledSchema, policy: Option<&dyn RLSPolicy>) -> Self {
        let mut by_field = HashMap::new();
        for parent in &schema.types {
            for field in &parent.fields {
                let Some(target) = object_type_of(&field.field_type) else {
                    continue;
                };
                if let Some(gate) = row_gate(schema, policy, parent, field, target) {
                    by_field.insert((parent.name.to_string(), field.name.to_string()), gate);
                }
            }
        }
        Self { by_field }
    }

    fn get(&self, parent: &str, field: &str) -> Option<&NestedRowGate> {
        self.by_field.get(&(parent.to_string(), field.to_string()))
    }
}

/// The SQL-backed read of `type_name` a nested level of it is gated as: its list query
/// when it has one — the read REST embeds it through — otherwise any.
pub(in super::super) fn own_read<'a>(
    schema: &'a CompiledSchema,
    type_name: &str,
) -> Option<&'a QueryDefinition> {
    let reads = |q: &&QueryDefinition| {
        q.return_type == type_name
            && q.function.is_none()
            && q.sql_source.is_some()
            && !q.returns_count
    };
    schema
        .queries
        .iter()
        .filter(reads)
        .find(|q| q.returns_list)
        .or_else(|| schema.queries.iter().find(reads))
}

fn row_gate(
    schema: &CompiledSchema,
    policy: Option<&dyn RLSPolicy>,
    parent: &TypeDefinition,
    field: &FieldDefinition,
    target: &str,
) -> Option<NestedRowGate> {
    let read = own_read(schema, target)?;
    if policy.is_none() && read.inject_params.is_empty() {
        return None;
    }

    // The keys the type's predicate may read: the policy's, and each `inject_params`
    // column held in the document. A native column is not in an embedded document.
    let mut paths = match policy.map(|p| p.constrained_paths(&RlsTarget::query(&read.name, target)))
    {
        None => Some(Vec::new()),
        Some(ConstrainedPaths::Declared(paths)) => Some(paths),
        Some(ConstrainedPaths::Opaque) => None,
    };
    for column in read.inject_params.keys() {
        if read.native_columns.contains_key(column) {
            paths = None;
        } else if let Some(paths) = paths.as_mut() {
            paths.push(crate::utils::to_snake_case(column));
        }
    }
    let declared = schema.find_type(target);
    let carried = paths.filter(|paths| {
        paths.iter().all(|path| {
            declared.is_some_and(|t| {
                t.fields.iter().any(|f| crate::utils::to_snake_case(f.name.as_str()) == *path)
            })
        })
    });
    if let Some(paths) = carried {
        return Some(NestedRowGate::Project {
            query: read.name.clone(),
            paths,
        });
    }

    let is_list = field.field_type.is_list();
    let relationship = parent.relationships.iter().find(|r| {
        r.name == field.name && r.target_type == target && r.cardinality.is_to_one() != is_list
    });
    let list = schema.queries.iter().find(|q| {
        q.return_type == target && q.returns_list && q.function.is_none() && !q.returns_count
    });
    Some(match (relationship, list.and_then(|q| q.sql_source.as_ref().map(|v| (q, v)))) {
        (Some(relationship), Some((list, view))) => NestedRowGate::Join {
            query:        list.name.clone(),
            view:         view.clone(),
            relationship: relationship.clone(),
        },
        _ => NestedRowGate::Refuse {
            query: read.name.clone(),
        },
    })
}

impl SelectionAccess {
    fn masks(&self, type_name: &str, field: &str) -> bool {
        self.masked.contains(&(type_name.to_string(), field.to_string()))
    }
}

impl QueryRunner {
    /// The nested levels of `root_fields` read as levels of a composed statement: every one
    /// whose type's predicate applies, and every one above such a level, which has to be a
    /// level to carry it. Empty when no nested level is scoped — the read stays flat.
    ///
    /// # Errors
    ///
    /// `FraiseQLError::Authorization` when a nested type's predicate cannot be applied
    /// ([`NestedRowGate::Refuse`], or a declared-path predicate that reads an undeclared
    /// key); the policy's own refusals; `FraiseQLError::Validation` for an `inject_params`
    /// read with no principal to resolve it from.
    pub(super) fn plan_nested_reads(
        &self,
        root_type: &str,
        root_fields: &[FieldSelection],
        security_context: Option<&SecurityContext>,
        access: &SelectionAccess,
    ) -> Result<Vec<ComposedEmbed>> {
        self.plan_nested_level(root_type, root_fields, security_context, access)
    }

    fn plan_nested_level(
        &self,
        parent_type: &str,
        selections: &[FieldSelection],
        security_context: Option<&SecurityContext>,
        access: &SelectionAccess,
    ) -> Result<Vec<ComposedEmbed>> {
        let schema = &self.ctx.schema;
        let Some(parent_def) = schema.find_type(parent_type) else {
            return Ok(Vec::new());
        };

        // One level per field, over every selection of it: `a: orders { id }` and
        // `b: orders { total }` read the same rows.
        let mut fields: Vec<(&FieldDefinition, Vec<FieldSelection>)> = Vec::new();
        for sel in effective_selections(selections, parent_type, schema) {
            let Some(field) = parent_def.fields.iter().find(|f| f.name == sel.name) else {
                continue;
            };
            if object_type_of(&field.field_type).is_none() {
                continue;
            }
            match fields.iter_mut().find(|(f, _)| f.name == field.name) {
                Some((_, subs)) => subs.extend(sel.nested_fields.iter().cloned()),
                None => fields.push((field, sel.nested_fields.clone())),
            }
        }

        let mut embeds = Vec::new();
        for (field, subs) in fields {
            let Some(target) = object_type_of(&field.field_type) else {
                continue;
            };
            let children = self.plan_nested_level(target, &subs, security_context, access)?;
            let gate = self.ctx.nested_row_gates.get(parent_type, field.name.as_str());

            let (predicate, source, view) = match gate {
                None => (None, None, None),
                Some(NestedRowGate::Project { query, paths }) => {
                    let predicate = self.nested_predicate(query, target, security_context)?;
                    if let Some(clause) = predicate.as_ref() {
                        if !reads_only(clause, paths) {
                            return Err(FraiseQLError::Authorization {
                                message:  format!(
                                    "the row-security predicate for '{target}' reads a key its \
                                     policy does not declare, so it cannot be evaluated over the \
                                     '{target}' documents '{parent_type}.{}' embeds; refusing \
                                     rather than serving them unfiltered",
                                    field.name
                                ),
                                action:   Some("read".to_string()),
                                resource: Some(target.to_string()),
                            });
                        }
                    }
                    (predicate, None, None)
                },
                Some(NestedRowGate::Join {
                    query,
                    view,
                    relationship,
                }) => {
                    let predicate = self.nested_predicate(query, target, security_context)?;
                    let source =
                        super::query_composed::correlated_source(schema, relationship, parent_type);
                    (predicate, Some(source), Some(view.clone()))
                },
                Some(NestedRowGate::Refuse { query }) => {
                    let predicate = self.nested_predicate(query, target, security_context)?;
                    if predicate.is_some() {
                        return Err(FraiseQLError::Authorization {
                            message:  format!(
                                "'{parent_type}.{}' embeds '{target}', whose row-security policy \
                                 does not declare the keys it reads, and '{parent_type}' declares \
                                 no relationship '{}' to read '{target}' through; refusing rather \
                                 than serving '{target}' rows unfiltered (declare the \
                                 relationship, or implement `RLSPolicy::constrained_paths`)",
                                field.name, field.name
                            ),
                            action:   Some("read".to_string()),
                            resource: Some(target.to_string()),
                        });
                    }
                    (None, None, None)
                },
            };

            // A joined level is always joined, so which rows it holds does not depend on
            // who asks. A materialised one is only a level when it has to be.
            if source.is_none() && predicate.is_none() && children.is_empty() {
                continue;
            }
            let (stored, fallback) = crate::runtime::stored_key_candidates(field.name.as_str());
            let source = source.unwrap_or_else(|| EmbedSource::Materialised {
                keys: std::iter::once(stored.clone()).chain(fallback).collect(),
            });
            let view = view.unwrap_or_else(|| {
                schema.find_type(target).map_or_else(String::new, |t| t.sql_source.to_string())
            });
            let keys = level_keys(schema, target, &subs, &children, access);
            embeds.push(ComposedEmbed {
                output_key: stored,
                shape: if field.field_type.is_list() {
                    EmbedShape::Many
                } else {
                    EmbedShape::One
                },
                source,
                level: ComposedLevel {
                    view,
                    projection: None,
                    where_clause: predicate,
                    order_by: None,
                    limit: None,
                    offset: None,
                    keyset: None,
                    keys,
                    embeds: children,
                },
            });
        }
        Ok(embeds)
    }

    /// Refuse a selection, for a read that cannot carry a composed level, when any nested
    /// level of it reaches a type whose row predicate applies to this caller: the
    /// federation resolver's entity lookup, built in `fraiseql-federation`. (A Relay
    /// connection's keyset page is a composed root now.) `surface` names the read in the
    /// refusal.
    ///
    /// A level whose type's predicate is empty for this caller is served as embedded,
    /// as [`Self::plan_nested_reads`] would leave it flat.
    ///
    /// # Errors
    ///
    /// `FraiseQLError::Authorization` for a nested level whose type's predicate applies;
    /// what [`Self::plan_nested_reads`] returns for a predicate it cannot evaluate.
    #[cfg(feature = "federation")]
    pub(in super::super) fn refuse_row_gated_levels(
        &self,
        parent_type: &str,
        selections: &[FieldSelection],
        security_context: Option<&SecurityContext>,
        surface: &str,
    ) -> Result<()> {
        let schema = &self.ctx.schema;
        let Some(parent_def) = schema.find_type(parent_type) else {
            return Ok(());
        };
        for sel in effective_selections(selections, parent_type, schema) {
            let Some(field) = parent_def.fields.iter().find(|f| f.name == sel.name) else {
                continue;
            };
            let Some(target) = object_type_of(&field.field_type) else {
                continue;
            };
            if let Some(gate) = self.ctx.nested_row_gates.get(parent_type, field.name.as_str()) {
                if self.nested_predicate(gate.query(), target, security_context)?.is_some() {
                    return Err(FraiseQLError::Authorization {
                        message:  format!(
                            "{surface} cannot apply the row security of '{target}' to the \
                             '{target}' rows '{parent_type}.{}' embeds; refusing rather than \
                             serving them unfiltered",
                            field.name
                        ),
                        action:   Some("read".to_string()),
                        resource: Some(target.to_string()),
                    });
                }
            }
            self.refuse_row_gated_levels(target, &sel.nested_fields, security_context, surface)?;
        }
        Ok(())
    }

    /// The row predicate a read of `target` through `query` carries: see [`level_predicate`].
    fn nested_predicate(
        &self,
        query: &str,
        target: &str,
        security_context: Option<&SecurityContext>,
    ) -> Result<Option<WhereClause>> {
        level_predicate(&self.ctx, query, target, security_context)
    }
}

/// The row predicate a read of `target` through `query` carries: the policy's, AND-ed with
/// the query's `inject_params` — what a root read of that query composes.
fn level_predicate(
    ctx: &ExecutorContext,
    query: &str,
    target: &str,
    security_context: Option<&SecurityContext>,
) -> Result<Option<WhereClause>> {
    row_predicate(&ctx.schema, ctx.config.rls_policy.as_deref(), query, target, security_context)
}

/// [`level_predicate`] over a schema and a policy, for a caller that holds no executor
/// context (the client-filter chokepoint).
fn row_predicate(
    schema: &CompiledSchema,
    policy: Option<&dyn RLSPolicy>,
    query: &str,
    target: &str,
    security_context: Option<&SecurityContext>,
) -> Result<Option<WhereClause>> {
    let Some(read) = schema.queries.iter().find(|q| q.name == query) else {
        return Ok(None);
    };
    let mut conditions = Vec::new();
    match (policy, security_context) {
        (Some(policy), Some(principal)) => {
            if let Some(clause) =
                policy.evaluate(principal, &RlsTarget::query(&read.name, target))?
            {
                conditions.push(clause.into_where_clause());
            }
        },
        // Fail closed (#784), as a root read with a policy and no principal does.
        (Some(_), None) => {
            return Err(FraiseQLError::Validation {
                message: format!("Query '{}' not found in schema", read.name),
                path:    None,
            });
        },
        (None, _) => {},
    }
    if !read.inject_params.is_empty() {
        let Some(principal) = security_context else {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Type '{target}' is scoped by the inject params of query '{}', which a \
                     request without a security context cannot resolve",
                    read.name
                ),
                path:    None,
            });
        };
        for (column, source) in &read.inject_params {
            let value = super::super::resolve_inject_value(
                column,
                source,
                principal,
                schema.tenant_claim(),
            )?;
            conditions.push(super::query_params::inject_param_where_clause(
                column,
                value,
                &read.native_columns,
            ));
        }
    }
    Ok(match conditions.len() {
        0 => None,
        1 => conditions.pop(),
        _ => Some(WhereClause::And(conditions)),
    })
}

/// A client filter over `root_type`, rewritten so that every path through a to-one relation
/// into a row-gated type reads its value only where the caller may read the related row
/// (ruling AH): a [`WhereClause::Guarded`] per crossing, carrying the predicate a read of
/// that type composes — path (a)'s own derivation, over the embedded document. To the
/// filter, a related row the caller may not read is as absent as it is to the response. A
/// clause that crosses no row-gated relation for this caller comes back unchanged.
///
/// # Errors
///
/// `FraiseQLError::Authorization` for a relation into a type whose predicate applies to
/// this caller and cannot be read off the embedded document (a policy that does not declare
/// its keys, a native `inject_params` column); what [`row_predicate`] returns (a policy with
/// no principal, #784).
pub(in super::super) fn guard_relation_filters(
    schema: &CompiledSchema,
    policy: Option<&dyn RLSPolicy>,
    root_type: &str,
    clause: &WhereClause,
    security_context: Option<&SecurityContext>,
) -> Result<WhereClause> {
    let guard =
        |c: &WhereClause| guard_relation_filters(schema, policy, root_type, c, security_context);
    Ok(match clause {
        WhereClause::And(all) => WhereClause::And(all.iter().map(guard).collect::<Result<_>>()?),
        WhereClause::Or(all) => WhereClause::Or(all.iter().map(guard).collect::<Result<_>>()?),
        WhereClause::Not(inner) => WhereClause::Not(Box::new(guard(inner)?)),
        WhereClause::Typed { types, inner } => WhereClause::Typed {
            types: types.clone(),
            inner: Box::new(guard(inner)?),
        },
        WhereClause::Field { path, .. } => {
            relation_guards(schema, policy, root_type, path, security_context)?
                .into_iter()
                .rev()
                .fold(clause.clone(), |inner, (under, guard)| WhereClause::Guarded {
                    under,
                    guard: Box::new(guard),
                    inner: Box::new(inner),
                })
        },
        // A native column is the root row's own; an already guarded subtree is final; and a
        // shape this does not know is left to the generators, which refuse what they cannot
        // render.
        _ => clause.clone(),
    })
}

/// The guard of every to-one relation `path` crosses into a type whose row predicate applies
/// to this caller: the relation's path, and the predicate over absolute paths.
fn relation_guards(
    schema: &CompiledSchema,
    policy: Option<&dyn RLSPolicy>,
    root_type: &str,
    path: &[String],
    security_context: Option<&SecurityContext>,
) -> Result<Vec<(Vec<String>, WhereClause)>> {
    let mut guards = Vec::new();
    let mut current = root_type.to_string();
    // The last segment is the compared field; every one before it steps into a type.
    for depth in 0..path.len().saturating_sub(1) {
        let Some(parent) = schema.find_type(&current) else {
            break;
        };
        let Some(field) = parent
            .fields
            .iter()
            .find(|f| crate::utils::to_snake_case(f.name.as_str()) == path[depth])
        else {
            break;
        };
        let Some(target) = object_type_of(&field.field_type) else {
            break;
        };
        // A list relation cannot be filtered through (`where` refuses it as a scalar);
        // were it made filterable, it would need the per-element form (ruling AH 5).
        if field.field_type.is_list() {
            break;
        }
        // The predicate is the one path (a) applies to this level: its gate's read's.
        if let Some(gate) = row_gate(schema, policy, parent, field, target) {
            if let Some(predicate) =
                row_predicate(schema, policy, gate.query(), target, security_context)?
            {
                let under = path[..=depth].to_vec();
                let guard = match &gate {
                    NestedRowGate::Project { paths, .. } if reads_only(&predicate, paths) => {
                        under_path(&predicate, &under)
                    },
                    // Ruling AL: the level path (a) reads from the target's own view through
                    // the declared relationship — the filter asks the same question there.
                    NestedRowGate::Join {
                        view, relationship, ..
                    } => {
                        let (target_key, parent_key, key_type) =
                            super::query_composed::correlation_keys(
                                schema,
                                relationship,
                                parent.name.as_str(),
                            );
                        WhereClause::KeyIn {
                            path: path[..depth].iter().chain(&parent_key).cloned().collect(),
                            key_type,
                            view: view.clone(),
                            target_key,
                            predicate: Box::new(predicate),
                        }
                    },
                    _ => {
                        return Err(FraiseQLError::Authorization {
                            message:  format!(
                                "Access denied: '{}.{}' cannot be used to filter by: which \
                                 '{target}' rows the request may read cannot be decided over the \
                                 document the view embeds (declare the policy's keys with \
                                 `RLSPolicy::constrained_paths`, or declare '{}' as a \
                                 relationship of '{}')",
                                parent.name, field.name, field.name, parent.name
                            ),
                            action:   Some("read".to_string()),
                            resource: Some(target.to_string()),
                        });
                    },
                };
                guards.push((under, guard));
            }
        }
        current = target.to_string();
    }
    Ok(guards)
}

/// `clause` with every path prefixed by `under`: a predicate over a type's own document,
/// read off the document embedded at `under`. Only called on a clause [`reads_only`]
/// accepted, which holds nothing but `Field`s under `And` / `Or` / `Not` / `Typed`.
fn under_path(clause: &WhereClause, under: &[String]) -> WhereClause {
    match clause {
        WhereClause::Field {
            path,
            operator,
            value,
        } => WhereClause::Field {
            path:     under.iter().chain(path).cloned().collect(),
            operator: operator.clone(),
            value:    value.clone(),
        },
        WhereClause::And(all) => {
            WhereClause::And(all.iter().map(|c| under_path(c, under)).collect())
        },
        WhereClause::Or(all) => WhereClause::Or(all.iter().map(|c| under_path(c, under)).collect()),
        WhereClause::Not(inner) => WhereClause::Not(Box::new(under_path(inner, under))),
        WhereClause::Typed { types, inner } => WhereClause::Typed {
            types: types.clone(),
            inner: Box::new(under_path(inner, under)),
        },
        other => other.clone(),
    }
}

/// Whether `clause` reads only the top-level stored keys in `paths`.
fn reads_only(clause: &WhereClause, paths: &[String]) -> bool {
    match clause {
        WhereClause::Field { path, .. } => path.first().is_some_and(|key| paths.contains(key)),
        WhereClause::And(all) | WhereClause::Or(all) => all.iter().all(|c| reads_only(c, paths)),
        WhereClause::Not(inner) | WhereClause::Typed { inner, .. } => reads_only(inner, paths),
        // A native column is not in a document, and a shape this does not know reads
        // something it cannot vouch for.
        _ => false,
    }
}

/// The keys a nested level returns: every selected field's stored spellings, masked ones
/// as `NULL`, and none of those its own embedded levels replace.
fn level_keys(
    schema: &CompiledSchema,
    type_name: &str,
    selections: &[FieldSelection],
    embedded: &[ComposedEmbed],
    access: &SelectionAccess,
) -> LevelKeys {
    let mut kept = Vec::new();
    let mut masked = Vec::new();
    for sel in effective_selections(selections, type_name, schema) {
        if sel.name == "__typename" {
            continue;
        }
        let (stored, fallback) = crate::runtime::stored_key_candidates(&sel.name);
        if embedded.iter().any(|e| e.output_key == stored) {
            continue;
        }
        let into = if access.masks(type_name, &sel.name) {
            &mut masked
        } else {
            &mut kept
        };
        for key in std::iter::once(stored).chain(fallback) {
            if !into.contains(&key) {
                into.push(key);
            }
        }
    }
    LevelKeys::Only { kept, masked }
}

/// The root's keys when `embedded` replace some of them: its whole document, less every
/// stored spelling of each — the embedded value is the gated one.
pub(super) fn root_keys(embedded: &[ComposedEmbed]) -> LevelKeys {
    LevelKeys::Without(
        embedded
            .iter()
            .flat_map(|e| match &e.source {
                EmbedSource::Materialised { keys } => keys.clone(),
                EmbedSource::Correlated { .. } => {
                    let (stored, fallback) = crate::runtime::stored_key_candidates(&e.output_key);
                    std::iter::once(stored).chain(fallback).collect()
                },
            })
            .collect(),
    )
}

/// The stored document of one composed row, with each embedded value written under its
/// stored key in place of what the parent's view stored there.
pub(super) fn merge_composed_row(
    row: &serde_json::Value,
    embeds: &[ComposedEmbed],
) -> serde_json::Value {
    let mut document = row.get(COMPOSED_DOCUMENT_KEY).cloned().unwrap_or(serde_json::Value::Null);
    let embedded = row.get(COMPOSED_EMBEDS_KEY);
    if let serde_json::Value::Object(object) = &mut document {
        for embed in embeds {
            let raw = embedded.and_then(|e| e.get(&embed.output_key));
            let value = match (embed.shape, raw) {
                (EmbedShape::Many, Some(serde_json::Value::Array(rows))) => {
                    serde_json::Value::Array(
                        rows.iter().map(|r| merge_composed_row(r, &embed.level.embeds)).collect(),
                    )
                },
                (EmbedShape::Many, _) => serde_json::Value::Array(Vec::new()),
                (EmbedShape::One, Some(row @ serde_json::Value::Object(_))) => {
                    merge_composed_row(row, &embed.level.embeds)
                },
                _ => serde_json::Value::Null,
            };
            object.insert(embed.output_key.clone(), value);
        }
    }
    document
}

impl QueryRunner {
    /// Read `root` as one composed statement and return its rows as stored documents, each
    /// with its gated nested levels merged in — the shape a flat read of the root returns.
    ///
    /// # Errors
    ///
    /// `FraiseQLError::Unsupported` from an adapter that cannot compose, refused from its
    /// capability flag as REST's composed read is; the adapter's own errors.
    pub(super) async fn execute_composed_document_read(
        &self,
        root: ComposedLevel,
        session_pairs: &[(&str, &str)],
        routing: crate::backend::types::ReadRouting,
    ) -> Result<std::sync::Arc<Vec<crate::backend::JsonbValue>>> {
        if !self.ctx.adapter.supports_composed_reads() {
            return Err(crate::backend::composed_read_unsupported(&root.view));
        }
        let rows = self
            .ctx
            .adapter
            .execute_composed_with_session(&root, session_pairs, routing)
            .await?;
        Ok(std::sync::Arc::new(
            rows.iter()
                .map(|row| {
                    crate::backend::JsonbValue::new(merge_composed_row(&row.data, &root.embeds))
                })
                .collect(),
        ))
    }
}

/// Project stored documents through `root_fields` at `root_type`, every level included.
pub(super) fn project_documents(
    documents: &[crate::backend::JsonbValue],
    root_type: &str,
    root_fields: &[FieldSelection],
    schema: &CompiledSchema,
    returns_list: bool,
) -> serde_json::Value {
    let project = |document: &crate::backend::JsonbValue| {
        crate::runtime::project_entity(&document.data, root_type, root_fields, schema)
    };
    if returns_list {
        serde_json::Value::Array(documents.iter().map(project).collect())
    } else {
        documents.first().map_or(serde_json::Value::Null, project)
    }
}

// ── A REST leaf selection of an object field ─────────────────────────────────
//
// `members?select=id,team` names `team`, a field of `Member` rather than an embedded
// relationship, and REST's selections are leaves: the field used to be projected with no
// sub-selection, so a to-one came back as the stored sub-object — none of `Team`'s gates
// applied — and a list as one `{}` per stored element. A leaf object field means the
// whole object: every field its type declares, each classified, masked and row-gated as
// the GraphQL selection of the same fields is.

/// How deep a leaf object selection is expanded. The expansion follows the schema, not a
/// selection, so a type that reaches itself would expand forever; an object field below
/// this depth **refuses** the request. It is not dropped, which would answer a different
/// question, and never served as stored, which is the leak the expansion exists to close.
const MAX_LEAF_OBJECT_DEPTH: usize = 4;

/// `query_match` with each leaf selection of an object field expanded to every field its
/// type declares, recursively. Borrowed unchanged when there is none.
///
/// # Errors
///
/// `FraiseQLError::Validation` when the whole object nests an object field deeper than
/// [`MAX_LEAF_OBJECT_DEPTH`].
pub(super) fn expand_leaf_objects<'a>(
    schema: &CompiledSchema,
    query_match: &'a crate::runtime::QueryMatch,
) -> Result<std::borrow::Cow<'a, crate::runtime::QueryMatch>> {
    let root_type = &query_match.query_def.return_type;
    let Some(type_def) = schema.find_type(root_type) else {
        return Ok(std::borrow::Cow::Borrowed(query_match));
    };
    let root_fields =
        query_match.selections.first().map_or(&[][..], |r| r.nested_fields.as_slice());
    let is_leaf_object = |sel: &FieldSelection| {
        sel.nested_fields.is_empty()
            && type_def
                .fields
                .iter()
                .find(|f| f.name == sel.name)
                .is_some_and(|f| object_type_of(&f.field_type).is_some())
    };
    if !root_fields.iter().any(is_leaf_object) {
        return Ok(std::borrow::Cow::Borrowed(query_match));
    }

    let mut expanded = query_match.clone();
    if let Some(root) = expanded.selections.first_mut() {
        for sel in &mut root.nested_fields {
            if is_leaf_object(sel) {
                let child = type_def
                    .fields
                    .iter()
                    .find(|f| f.name == sel.name)
                    .and_then(|f| object_type_of(&f.field_type));
                if let Some(child) = child {
                    sel.nested_fields = whole_object(schema, child, 1)?;
                }
            }
        }
    }
    Ok(std::borrow::Cow::Owned(expanded))
}

/// Every field `type_name` declares, as a selection, object fields expanded in turn.
pub(in super::super) fn whole_object(
    schema: &CompiledSchema,
    type_name: &str,
    depth: usize,
) -> Result<Vec<FieldSelection>> {
    let Some(type_def) = schema.find_type(type_name) else {
        return Ok(Vec::new());
    };
    type_def
        .fields
        .iter()
        .map(|field| {
            let nested_fields = match object_type_of(&field.field_type) {
                None => Vec::new(),
                Some(child) if depth >= MAX_LEAF_OBJECT_DEPTH => {
                    return Err(FraiseQLError::Validation {
                        message: format!(
                            "The whole object selected reaches '{type_name}.{}', a '{child}' \
                             {depth} levels down; a leaf object selection expands at most \
                             {MAX_LEAF_OBJECT_DEPTH} levels",
                            field.name
                        ),
                        path:    None,
                    });
                },
                Some(child) => whole_object(schema, child, depth + 1)?,
            };
            Ok(FieldSelection {
                name: field.name.to_string(),
                alias: None,
                arguments: Vec::new(),
                nested_fields,
                directives: Vec::new(),
            })
        })
        .collect()
}

/// Re-project each root object field that carries a sub-selection from the stored row, at
/// its own type: the flat projector returns a nested object verbatim, keys and all.
pub(super) fn project_object_fields(
    projected: &mut serde_json::Value,
    rows: &[crate::backend::JsonbValue],
    root_type: &str,
    root_fields: &[FieldSelection],
    schema: &CompiledSchema,
) {
    let Some(type_def) = schema.find_type(root_type) else {
        return;
    };
    let objects: Vec<(&FieldSelection, &str)> =
        effective_selections(root_fields, root_type, schema)
            .into_iter()
            .filter(|sel| !sel.nested_fields.is_empty())
            .filter_map(|sel| {
                type_def
                    .fields
                    .iter()
                    .find(|f| f.name == sel.name)
                    .and_then(|f| object_type_of(&f.field_type))
                    .map(|child| (sel, child))
            })
            .collect();
    if objects.is_empty() {
        return;
    }
    let reproject = |item: &mut serde_json::Value, row: &crate::backend::JsonbValue| {
        let Some(object) = item.as_object_mut() else {
            return;
        };
        for (sel, child) in &objects {
            let (stored, fallback) = crate::runtime::stored_key_candidates(&sel.name);
            let raw = row.data.get(&stored).or_else(|| fallback.and_then(|k| row.data.get(&k)));
            let value = match raw {
                Some(serde_json::Value::Array(elements)) => serde_json::Value::Array(
                    elements
                        .iter()
                        .map(|el| {
                            crate::runtime::project_entity(el, child, &sel.nested_fields, schema)
                        })
                        .collect(),
                ),
                Some(value) => {
                    crate::runtime::project_entity(value, child, &sel.nested_fields, schema)
                },
                None => continue,
            };
            object.insert(sel.response_key().to_string(), value);
        }
    };
    match projected {
        serde_json::Value::Array(items) => {
            for (item, row) in items.iter_mut().zip(rows) {
                reproject(item, row);
            }
        },
        item @ serde_json::Value::Object(_) => {
            if let Some(row) = rows.first() {
                reproject(item, row);
            }
        },
        _ => {},
    }
}

// ── Row security over a document a write returned ────────────────────────────
//
// A mutation's payload is the document its function returned, not a read of a view: there
// is no statement to carry a composed level, and the root entity is the function's to
// report. A nested level of it is still a read of its own type, so the type's predicate
// applies to the documents embedded there — evaluated in memory, over the returned
// document, where the policy declares the keys it reads (`NestedRowGate::Project`) and the
// predicate is a conjunction of equalities. Anything else refuses, before the write:
// joining through a relationship would be a read after the write (not done yet), and a
// predicate the evaluator below cannot decide is not one to guess at.
//
// The evaluator is stricter than SQL, never looser: two JSON values are equal only when
// they are the same value, and `null` equals nothing. A document whose key PostgreSQL would
// cast to match — a numeric string against a number, an upper-cased UUID — is dropped, and
// no document SQL would drop is kept.

/// One equality a nested level's documents must meet: the stored path, and the value.
type DocumentCondition = (Vec<String>, serde_json::Value);

/// The conditions each row-gated nested level of a returned document must meet, by
/// `(parent type, field)` — built before the write, applied after it.
#[derive(Debug, Default)]
pub(in super::super) struct DocumentRowFilter {
    by_field: HashMap<(String, String), Vec<DocumentCondition>>,
}

impl DocumentRowFilter {
    /// Plan the row security of every nested level of `selections` at `root_type`, merged
    /// into this filter. The root is not filtered.
    ///
    /// # Errors
    ///
    /// `FraiseQLError::Authorization` for a nested level whose type's predicate applies to
    /// this caller and cannot be evaluated over the returned document: a policy that does
    /// not declare its keys, a predicate that reads a key it did not declare, or one that is
    /// not a conjunction of equalities. What [`level_predicate`] returns.
    pub(in super::super) fn plan(
        &mut self,
        ctx: &ExecutorContext,
        root_type: &str,
        selections: &[FieldSelection],
        security_context: Option<&SecurityContext>,
    ) -> Result<()> {
        let schema = &ctx.schema;
        let Some(parent_def) = schema.find_type(root_type) else {
            return Ok(());
        };
        for sel in effective_selections(selections, root_type, schema) {
            let Some(field) = parent_def.fields.iter().find(|f| f.name == sel.name) else {
                continue;
            };
            let Some(target) = object_type_of(&field.field_type) else {
                continue;
            };
            let key = (root_type.to_string(), field.name.to_string());
            if let std::collections::hash_map::Entry::Vacant(slot) = self.by_field.entry(key) {
                if let Some(gate) = ctx.nested_row_gates.get(root_type, field.name.as_str()) {
                    let predicate = level_predicate(ctx, gate.query(), target, security_context)?;
                    if let Some(clause) = predicate {
                        let refuse = |why: &str| FraiseQLError::Authorization {
                            message:  format!(
                                "'{root_type}.{}' embeds '{target}' in the document the write \
                                 returned, and {why}; refusing before the write rather than \
                                 serving '{target}' rows unfiltered",
                                field.name
                            ),
                            action:   Some("read".to_string()),
                            resource: Some(target.to_string()),
                        };
                        let NestedRowGate::Project { paths, .. } = gate else {
                            return Err(refuse(
                                "the row-security policy of that type does not declare the \
                                 keys it reads, so it cannot be evaluated over that document",
                            ));
                        };
                        if !reads_only(&clause, paths) {
                            return Err(refuse(
                                "its row-security predicate reads a key its policy does not \
                                 declare",
                            ));
                        }
                        let mut conditions = Vec::new();
                        if !equalities(&clause, &mut conditions) {
                            return Err(refuse(
                                "its row-security predicate is not a conjunction of \
                                 equalities, the only shape evaluated over a returned document",
                            ));
                        }
                        slot.insert(conditions);
                    }
                }
            }
            self.plan(ctx, target, &sel.nested_fields, security_context)?;
        }
        Ok(())
    }

    /// Whether no nested level is filtered.
    pub(in super::super) fn is_empty(&self) -> bool {
        self.by_field.is_empty()
    }

    /// Drop, from the stored `document` of a `root_type`, every embedded document a nested
    /// level of `selections` reaches that its type's predicate excludes: from a list, the
    /// element; a to-one, `null`.
    pub(in super::super) fn apply(
        &self,
        document: &mut serde_json::Value,
        root_type: &str,
        selections: &[FieldSelection],
        schema: &CompiledSchema,
    ) {
        if self.by_field.is_empty() {
            return;
        }
        let Some(parent_def) = schema.find_type(root_type) else {
            return;
        };
        let serde_json::Value::Object(object) = document else {
            return;
        };
        for sel in effective_selections(selections, root_type, schema) {
            let Some(field) = parent_def.fields.iter().find(|f| f.name == sel.name) else {
                continue;
            };
            let Some(target) = object_type_of(&field.field_type) else {
                continue;
            };
            let (stored, fallback) = crate::runtime::stored_key_candidates(field.name.as_str());
            let key = if object.contains_key(&stored) {
                stored
            } else if let Some(fallback) = fallback.filter(|k| object.contains_key(k)) {
                fallback
            } else {
                continue;
            };
            let conditions = self.by_field.get(&(root_type.to_string(), field.name.to_string()));
            let Some(value) = object.get_mut(&key) else {
                continue;
            };
            match value {
                serde_json::Value::Array(elements) => {
                    if let Some(conditions) = conditions {
                        elements.retain(|el| meets(el, conditions));
                    }
                    for element in elements {
                        self.apply(element, target, &sel.nested_fields, schema);
                    }
                },
                serde_json::Value::Null => {},
                embedded => {
                    if conditions.is_some_and(|c| !meets(embedded, c)) {
                        *embedded = serde_json::Value::Null;
                    } else {
                        self.apply(embedded, target, &sel.nested_fields, schema);
                    }
                },
            }
        }
    }
}

/// The row predicate of a type read as the root of a pushed document — a subscription's
/// after-image (ruling AC 4): the type's policy AND its own read's `inject_params`, as a
/// root read of the type composes, evaluated over the document in memory.
#[derive(Debug)]
pub(in super::super) struct RootRowFilter {
    conditions: Vec<DocumentCondition>,
}

impl RootRowFilter {
    /// Plan the predicate `principal` reads `type_name` under, as the root of
    /// `subscription`'s documents. `None`: no predicate applies to this principal.
    ///
    /// # Errors
    ///
    /// `Validation` with no principal under a policy or `inject_params` (#784: refused as
    /// a query is); `Authorization` for a predicate the document cannot answer — a policy
    /// that does not declare its keys, one reading a key the type does not declare or a
    /// native column, or one that is not a conjunction of equalities.
    pub(in super::super) fn plan(
        ctx: &ExecutorContext,
        subscription: &str,
        type_name: &str,
        principal: Option<&SecurityContext>,
    ) -> Result<Option<Self>> {
        let schema = &ctx.schema;
        let policy = ctx.config.rls_policy.as_deref();
        let read = own_read(schema, type_name);
        let target = read.map_or_else(
            || RlsTarget::query(subscription, type_name),
            |read| RlsTarget::query(&read.name, type_name),
        );
        let clause = match read {
            Some(read) => level_predicate(ctx, &read.name, type_name, principal)?,
            None => match (policy, principal) {
                (Some(policy), Some(principal)) => policy
                    .evaluate(principal, &target)?
                    .map(crate::security::RlsWhereClause::into_where_clause),
                (Some(_), None) => {
                    return Err(FraiseQLError::Validation {
                        message: format!("Subscription '{subscription}' not found in schema"),
                        path:    None,
                    });
                },
                (None, _) => None,
            },
        };
        let Some(clause) = clause else {
            return Ok(None);
        };
        let refuse = |why: &str| FraiseQLError::Authorization {
            message:  format!(
                "Subscription '{subscription}' delivers '{type_name}', whose row-security \
                 predicate {why}; refusing the subscription rather than delivering rows \
                 unfiltered"
            ),
            action:   Some("read".to_string()),
            resource: Some(type_name.to_string()),
        };
        // The keys the predicate may read: the policy's, and each `inject_params` column —
        // every one a key the type declares, since the after-image holds the type's keys.
        let mut paths = match policy.map(|p| p.constrained_paths(&target)) {
            None => Vec::new(),
            Some(ConstrainedPaths::Declared(paths)) => paths,
            Some(ConstrainedPaths::Opaque) => {
                return Err(refuse("does not declare the keys it reads"));
            },
        };
        for column in read.map(|r| r.inject_params.keys()).into_iter().flatten() {
            if read.is_some_and(|r| r.native_columns.contains_key(column)) {
                return Err(refuse("reads a native column, which a document does not hold"));
            }
            paths.push(crate::utils::to_snake_case(column));
        }
        let declared = schema.find_type(type_name);
        let carried = paths.iter().all(|path| {
            declared.is_some_and(|t| {
                t.fields.iter().any(|f| crate::utils::to_snake_case(f.name.as_str()) == *path)
            })
        });
        if !carried || !reads_only(&clause, &paths) {
            return Err(refuse("reads a key the type does not declare"));
        }
        let mut conditions = Vec::new();
        if !equalities(&clause, &mut conditions) {
            return Err(refuse(
                "is not a conjunction of equalities, the only shape evaluated over a document",
            ));
        }
        Ok(Some(Self { conditions }))
    }

    /// Whether `document` is a row the predicate admits.
    pub(in super::super) fn admits(&self, document: &serde_json::Value) -> bool {
        meets(document, &self.conditions)
    }
}

/// Collect `clause` as a conjunction of equalities into `out`; `false` when it is not one.
fn equalities(clause: &WhereClause, out: &mut Vec<DocumentCondition>) -> bool {
    match clause {
        WhereClause::Field {
            path,
            operator: crate::backend::WhereOperator::Eq,
            value,
        } => {
            out.push((path.clone(), value.clone()));
            true
        },
        WhereClause::And(all) => all.iter().all(|c| equalities(c, out)),
        // The declared types steer SQL's casts; the comparison below casts nothing.
        WhereClause::Typed { inner, .. } => equalities(inner, out),
        _ => false,
    }
}

/// Whether `document` holds every condition's value at its path. `null` meets nothing.
fn meets(document: &serde_json::Value, conditions: &[DocumentCondition]) -> bool {
    conditions.iter().all(|(path, expected)| {
        let actual = path.iter().try_fold(document, |at, key| at.get(key));
        !expected.is_null() && actual == Some(expected)
    })
}

#[cfg(test)]
#[path = "query_nested_document_tests.rs"]
mod document_tests;

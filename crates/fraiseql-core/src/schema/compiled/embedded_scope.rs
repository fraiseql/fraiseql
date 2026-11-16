//! A type that carries its own scope key, embedded in another type's served document
//! (#1737).
//!
//! # What goes wrong
//!
//! A read of a scoped type is filtered by its `inject_params`: a column compared with a
//! value taken from the caller's token. Embedded in another type's document, the same rows
//! are stored inside that document and served to every reader of it. The engine filters the
//! nested level on the scope key, read from the embedded documents; where they do not carry
//! it, the level comes back empty or refused, on every request, and nothing said so when the
//! schema was written.
//!
//! # The rule
//!
//! A **scoped type** is one whose [`scoping_read`](CompiledSchema::scoping_read) carries
//! `inject_params`. A **served document** is the document of a type the server reads
//! directly: a query that reads a relation returns it, a subscription streams it, or
//! federation resolves it as an entity. The document holds the type's fields and the fields
//! of every type beneath them, to any depth: what a view embeds is served with it.
//!
//! A field of a served document that holds a scoped type is refused, unless it declares
//! [`scoped_like_parent`](crate::schema::FieldDefinition::scoped_like_parent). The
//! declaration is a claim, accepted only where the schema bears it out: every read of every
//! document that reaches the field carries each scope column of the embedded type, from the
//! same source. A declaration on a field that holds no scoped type is refused too, so it
//! cannot outlive the scope it was written for.
//!
//! A mutation's payload is not a served document: its function builds it for the caller.
//!
//! # What the rule cannot see
//!
//! Only `inject_params` is in the schema. A row-security policy installed on the server at
//! run time scopes a type without the schema saying so; such a type is not covered here, and
//! the nested level is gated when it is read.

use std::collections::{BTreeMap, BTreeSet};

use indexmap::IndexMap;

use super::{CompiledSchema, QueryDefinition, type_inject::describe_source};
use crate::schema::{FieldDefinition, FieldType, InjectedParamSource, TypeDefinition};

/// One field holding one scoped type, and the served documents that reach it.
struct Embedding<'a> {
    field:    &'a FieldDefinition,
    embedded: &'a str,
    read:     &'a QueryDefinition,
    roots:    BTreeSet<&'a str>,
}

impl CompiledSchema {
    /// The relation-reading query a level of `type_name` is scoped as: its list query when
    /// it has one, otherwise any. `None` for a type no query reads.
    ///
    /// One answer for the nested row gate and for
    /// [`embedded_scope_violations`](Self::embedded_scope_violations), so the schema is
    /// refused for exactly the levels the runtime would gate.
    #[must_use]
    pub fn scoping_read(&self, type_name: &str) -> Option<&QueryDefinition> {
        let reads = |q: &&QueryDefinition| {
            q.return_type == type_name && q.reads_relation().is_some() && !q.returns_count
        };
        self.queries
            .iter()
            .filter(reads)
            .find(|q| q.returns_list)
            .or_else(|| self.queries.iter().find(reads))
    }

    /// Scoped types embedded in a served document without a declaration the schema bears
    /// out, and declarations with nothing to declare.
    ///
    /// One message per violation, empty when the schema is enforceable as written. Called
    /// when a schema loads, in the compiler and at server boot.
    #[must_use]
    pub fn embedded_scope_violations(&self) -> Vec<String> {
        let mut embeddings: BTreeMap<(&str, &str, &str), Embedding<'_>> = BTreeMap::new();
        for root in self.types.iter().filter(|t| self.is_served(t.name.as_str())) {
            let mut seen = BTreeSet::new();
            self.collect_embeddings(root.name.as_str(), root, &mut seen, &mut embeddings);
        }

        let mut violations = Vec::new();
        for ((owner, field, _), embedding) in &embeddings {
            let columns = embedding.read.inject_params.keys().cloned().collect::<Vec<_>>();
            if !embedding.field.scoped_like_parent {
                violations.push(format!(
                    "`{owner}.{field}` embeds `{}`, whose read `{}` is scoped by `{}`: every \
                     reader of the document that holds it would be served those rows. Read \
                     them through a query of their own, or declare `scoped_like_parent` on \
                     the field when the document is scoped by the same key",
                    embedding.embedded,
                    embedding.read.name,
                    columns.join("`, `"),
                ));
                continue;
            }
            for root in &embedding.roots {
                violations.extend(self.unborne(owner, field, embedding, root));
            }
        }

        for owner in &self.types {
            for field in owner.fields.iter().filter(|f| f.scoped_like_parent) {
                if self.scoped_targets(&field.field_type).is_empty() {
                    violations.push(format!(
                        "`{}.{}` declares `scoped_like_parent`, and holds no type whose read \
                         is scoped by `inject_params`; remove the declaration",
                        owner.name, field.name
                    ));
                }
            }
        }
        violations
    }

    /// Whether the server reads `type_name` directly, as a document of its own.
    fn is_served(&self, type_name: &str) -> bool {
        !self.reads_of(type_name).is_empty()
            || self.subscriptions.iter().any(|s| s.return_type == type_name)
            || self
                .federation
                .as_ref()
                .is_some_and(|f| f.entities.iter().any(|entity| entity.name == type_name))
    }

    /// Every query that reads a relation and returns `type_name`, by name or as a member of
    /// the union or interface it returns.
    fn reads_of(&self, type_name: &str) -> Vec<&QueryDefinition> {
        self.queries
            .iter()
            .filter(|q| q.reads_relation().is_some() && !q.returns_count)
            .filter(|q| {
                q.return_type == type_name
                    || (self.find_type(&q.return_type).is_none()
                        && self
                            .possible_types(&q.return_type)
                            .is_some_and(|members| members.contains(&type_name)))
            })
            .collect()
    }

    /// The scoped types a field of `field_type` can hold, each with its scoping read.
    fn scoped_targets(&self, field_type: &FieldType) -> Vec<(&str, &QueryDefinition)> {
        self.object_types_of(field_type)
            .into_iter()
            .filter_map(|name| {
                let read = self.scoping_read(name)?;
                (!read.inject_params.is_empty()).then_some((name, read))
            })
            .collect()
    }

    /// The object types a field of `field_type` can hold: through lists, and every member of
    /// a union or an interface.
    fn object_types_of(&self, field_type: &FieldType) -> Vec<&str> {
        let mut base = field_type;
        while let FieldType::List { item, .. } = base {
            base = item;
        }
        let (FieldType::Object(name) | FieldType::Interface(name) | FieldType::Union(name)) = base
        else {
            return Vec::new();
        };
        match self.find_type(name) {
            Some(found) => vec![found.name.as_str()],
            None => self.possible_types(name).unwrap_or_default(),
        }
    }

    /// Walk the document of `root` from `holder`, recording each field that holds a scoped
    /// type and descending into every type beneath. A type served on its own is descended
    /// into as well: its documents are in this one, and where no query reads it nothing else
    /// holds this document to the scope its fields declare.
    fn collect_embeddings<'a>(
        &'a self,
        root: &'a str,
        holder: &'a TypeDefinition,
        seen: &mut BTreeSet<&'a str>,
        embeddings: &mut BTreeMap<(&'a str, &'a str, &'a str), Embedding<'a>>,
    ) {
        if !seen.insert(holder.name.as_str()) {
            return;
        }
        for field in &holder.fields {
            let scoped = self.scoped_targets(&field.field_type);
            for (embedded, read) in &scoped {
                embeddings
                    .entry((holder.name.as_str(), field.name.as_str(), embedded))
                    .or_insert_with(|| Embedding {
                        field,
                        embedded,
                        read,
                        roots: BTreeSet::new(),
                    })
                    .roots
                    .insert(root);
            }
            for name in self.object_types_of(&field.field_type) {
                if let Some(beneath) = self.find_type(name) {
                    self.collect_embeddings(root, beneath, seen, embeddings);
                }
            }
        }
    }

    /// Why the declaration on `owner.field` is not borne out by the document of `root`: one
    /// message per read of it, and per scope column, that falls short.
    fn unborne(
        &self,
        owner: &str,
        field: &str,
        embedding: &Embedding<'_>,
        root: &str,
    ) -> Vec<String> {
        let through = if owner == root {
            String::new()
        } else {
            format!(", reached through the document of `{root}`")
        };
        let reads = self.reads_of(root);
        // A document no query returns is served by a subscription or as a federation
        // entity, scoped by what the type itself declares.
        let scopes: Vec<(String, &IndexMap<String, InjectedParamSource>)> = if reads.is_empty() {
            self.find_type(root)
                .map(|t| (format!("`{root}`, which no query returns,"), &t.inject_params))
                .into_iter()
                .collect()
        } else {
            reads
                .iter()
                .map(|q| (format!("the read `{}`", q.name), &q.inject_params))
                .collect()
        };

        let mut violations = Vec::new();
        for (served_by, scope) in scopes {
            for (column, source) in &embedding.read.inject_params {
                let falls_short = match scope.get(column) {
                    None => format!("{served_by} is not scoped by `{column}`"),
                    Some(theirs) if theirs != source => format!(
                        "{served_by} takes `{column}` from {}, and `{}` takes it from {}",
                        describe_source(theirs),
                        embedding.read.name,
                        describe_source(source),
                    ),
                    Some(_) => continue,
                };
                violations.push(format!(
                    "`{owner}.{field}` declares `scoped_like_parent` for `{}`{through}, but \
                     {falls_short}",
                    embedding.embedded
                ));
            }
        }
        violations
    }
}

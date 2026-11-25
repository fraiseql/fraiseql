//! Selections of one response key can merge into one field (GraphQL § 5.3.2, Field
//! Selection Merging).
//!
//! Execution answers every selection of one response key as a single field (§ 6.3.2,
//! `CollectFields`), so a document is valid only if they can be one: under one parent type
//! they must name the same field with the same arguments, and wherever they stand they must
//! answer the same shape. `{ users { a: id a: name } }` gives `a` two fields; `posts(limit:
//! 1)` and `posts(limit: 2)` under one key give it two argument sets. Both are refused.
//!
//! The rule is per parent type. Two fragments on distinct object types can never both apply
//! to one value, so they may alias different fields to one key: `... on Dog { x: name } ...
//! on Cat { x: alias }` on a `Pet` union. That is why this runs on the document as written,
//! before named spreads are expanded: expansion keeps a spread's fields and loses its type
//! condition, and over the expanded set those two fragments would read as one parent. Even
//! there the key must answer one shape (`SameResponseShape`): the same leaf type, with the
//! same nullability and list wrapping, so a client knows the key's type without knowing
//! which member answered.
//!
//! Only what the schema proves is refused. A set at a position whose type the schema does
//! not have is not judged, and a field it does not declare is not judged on its shape: the
//! selection rules next door refuse those documents for what they are.

use std::collections::{HashMap, HashSet};

use super::{FieldSelection, FragmentDefinition};
use crate::{
    error::{FraiseQLError, Result},
    schema::{CompiledSchema, FieldType},
};

/// `__typename`, which every composite type has: a non-null `String`.
static TYPENAME: FieldType = FieldType::String;

/// Refuse the first response key, in the operation of `kind` whose root fields are
/// `roots`, whose selections cannot merge into one field.
///
/// # Errors
///
/// [`FraiseQLError::Validation`] at the key's path, naming it and why its selections
/// conflict.
pub fn check(
    roots: &[FieldSelection],
    fragments: &[FragmentDefinition],
    kind: &str,
    schema: &CompiledSchema,
) -> Result<()> {
    let walk = Walk { schema, fragments };
    walk.level(
        &[Source {
            lineage:    Vec::new(),
            position:   Position::Root(kind),
            selections: roots,
        }],
        "",
    )
}

/// What a selection set stands on.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Position<'a> {
    /// The root fields of an operation of this kind.
    Root(&'a str),
    /// A value of this object, interface or union type.
    Type(&'a str),
    /// A Relay connection over this node type: `edges`, `nodes`, `pageInfo`.
    Connection(&'a str),
    /// One edge of a Relay connection over this node type.
    Edge(&'a str),
    /// Nothing the schema describes.
    Unknown,
}

/// A selection set to collect, at `position`, below the fields of `lineage`.
struct Source<'a> {
    /// At each level above, the object type the enclosing field was selected on, if any.
    lineage:    Vec<Option<&'a str>>,
    position:   Position<'a>,
    selections: &'a [FieldSelection],
}

/// The selections of one field, with one set of arguments, on one parent, at one level.
struct Group<'a> {
    /// [`Source::lineage`], with the object type the field is selected on at this level.
    lineage: Vec<Option<&'a str>>,
    field:   &'a FieldSelection,
    /// Its declared type and whether it is nullable, when the schema declares it.
    shape:   Option<(&'a FieldType, bool)>,
    /// What its sub-selections stand on.
    child:   Position<'a>,
    /// The sub-selections of every occurrence.
    nested:  Vec<&'a [FieldSelection]>,
}

struct Walk<'a> {
    schema:    &'a CompiledSchema,
    fragments: &'a [FragmentDefinition],
}

impl<'a> Walk<'a> {
    /// Check the selections `sources` make at one level, under `path`, and then each key's
    /// sub-selections together.
    fn level(&self, sources: &[Source<'a>], path: &str) -> Result<()> {
        let mut keys: Vec<(&'a str, Vec<Group<'a>>)> = Vec::new();
        let mut index: HashMap<&'a str, usize> = HashMap::new();
        for source in sources {
            if self.known(source.position) {
                let mut spreading = HashSet::new();
                self.collect(
                    source,
                    source.selections,
                    source.position,
                    &mut spreading,
                    &mut |g| {
                        let key = g.field.response_key();
                        let at = *index.entry(key).or_insert_with(|| {
                            keys.push((key, Vec::new()));
                            keys.len() - 1
                        });
                        let groups = &mut keys[at].1;
                        match groups.iter_mut().find(|o| {
                            o.lineage == g.lineage
                                && o.field.name == g.field.name
                                && same_arguments(o.field, g.field)
                        }) {
                            Some(same) => same.nested.extend(g.nested),
                            None => groups.push(g),
                        }
                    },
                );
            }
        }
        for (key, groups) in &keys {
            let below = if path.is_empty() {
                (*key).to_string()
            } else {
                format!("{path}.{key}")
            };
            let mut rest = groups.as_slice();
            while let [first, others @ ..] = rest {
                if let Some(why) = others.iter().find_map(|second| conflict(first, second)) {
                    return Err(FraiseQLError::Validation {
                        message: format!(
                            "Fields \"{key}\" conflict because {why} (GraphQL § 5.3.2): use \
                             different aliases to select both"
                        ),
                        path:    Some(below),
                    });
                }
                rest = others;
            }
            let children: Vec<Source<'a>> = groups
                .iter()
                .flat_map(|g| {
                    g.nested.iter().map(|selections| Source {
                        lineage: g.lineage.clone(),
                        position: g.child,
                        selections,
                    })
                })
                .collect();
            if !children.is_empty() {
                self.level(&children, &below)?;
            }
        }
        Ok(())
    }

    /// Every field `selections` selects at `position`, through its fragments, as a group of
    /// its own, handed to `out`.
    fn collect(
        &self,
        source: &Source<'a>,
        selections: &'a [FieldSelection],
        position: Position<'a>,
        spreading: &mut HashSet<&'a str>,
        out: &mut dyn FnMut(Group<'a>),
    ) {
        for selection in selections {
            if let Some(condition) = selection.name.strip_prefix("...on ") {
                let at = Self::fragment_position(position, condition);
                self.collect(source, &selection.nested_fields, at, spreading, out);
            } else if let Some(name) = selection.name.strip_prefix("...") {
                // An undefined or cyclic spread is refused where spreads are expanded.
                let Some(fragment) = self.fragments.iter().find(|f| f.name == name) else {
                    continue;
                };
                if spreading.insert(fragment.name.as_str()) {
                    let at = Self::fragment_position(position, &fragment.type_condition);
                    self.collect(source, &fragment.selections, at, spreading, out);
                    spreading.remove(fragment.name.as_str());
                }
            } else {
                let mut lineage = source.lineage.clone();
                lineage.push(self.object_type(position));
                out(Group {
                    lineage,
                    field: selection,
                    shape: self.shape(position, &selection.name),
                    child: self.child(position, &selection.name),
                    nested: vec![selection.nested_fields.as_slice()],
                });
            }
        }
    }

    /// Where a fragment on `condition` (empty for an untyped inline fragment) at `position`
    /// selects. A root's fragments were read into the root as the document was parsed.
    const fn fragment_position(position: Position<'a>, condition: &'a str) -> Position<'a> {
        if condition.is_empty() {
            position
        } else {
            Position::Type(condition)
        }
    }

    /// Whether the schema describes `position`.
    fn known(&self, position: Position<'a>) -> bool {
        match position {
            Position::Root(_) | Position::Connection(_) | Position::Edge(_) => true,
            Position::Type(name) => {
                self.schema.find_type(name).is_some()
                    || self.schema.find_interface(name).is_some()
                    || self.schema.find_union(name).is_some()
            },
            Position::Unknown => false,
        }
    }

    /// The object type `position` holds, if it holds exactly one.
    fn object_type(&self, position: Position<'a>) -> Option<&'a str> {
        match position {
            Position::Type(name) => self.schema.find_type(name).map(|t| t.name.as_str()),
            _ => None,
        }
    }

    /// The declared type of the field `name` at `position`, and whether it is nullable.
    fn shape(&self, position: Position<'a>, name: &str) -> Option<(&'a FieldType, bool)> {
        let Position::Type(parent) = position else {
            return None;
        };
        if name == "__typename" {
            return Some((&TYPENAME, false));
        }
        let declared =
            self.schema.find_type(parent).and_then(|t| t.find_field(name)).or_else(|| {
                self.schema
                    .find_interface(parent)
                    .and_then(|i| i.fields.iter().find(|f| f.name == name))
            })?;
        Some((&declared.field_type, declared.nullable))
    }

    /// What the sub-selections of the field `name` at `position` stand on.
    fn child(&self, position: Position<'a>, name: &str) -> Position<'a> {
        let schema = self.schema;
        match position {
            Position::Root("mutation") => schema
                .find_mutation(name)
                .map_or(Position::Unknown, |m| Position::Type(&m.return_type)),
            Position::Root("subscription") => schema
                .find_subscription(name)
                .map_or(Position::Unknown, |s| Position::Type(&s.return_type)),
            Position::Root(_) if name == "node" && schema.publishes_node() => {
                Position::Type("Node")
            },
            Position::Root(_) => match schema.find_query(name) {
                Some(q) if q.returns_count => Position::Unknown,
                Some(q) if q.relay => Position::Connection(&q.return_type),
                Some(q) => Position::Type(&q.return_type),
                None => Position::Unknown,
            },
            Position::Connection(node) => match name {
                "nodes" => Position::Type(node),
                "edges" => Position::Edge(node),
                _ => Position::Unknown,
            },
            Position::Edge(node) if name == "node" => Position::Type(node),
            Position::Type(_) => match self.shape(position, name) {
                Some((declared, _)) => match declared.inner_type().unwrap_or(declared) {
                    FieldType::Object(t) | FieldType::Interface(t) | FieldType::Union(t) => {
                        Position::Type(t)
                    },
                    _ => Position::Unknown,
                },
                None => Position::Unknown,
            },
            Position::Edge(_) | Position::Unknown => Position::Unknown,
        }
    }
}

/// Why `first` and `second`, two groups under one key, cannot be one field, or `None`.
fn conflict(first: &Group<'_>, second: &Group<'_>) -> Option<String> {
    // Selected at some level on two distinct object types, they never answer for one value.
    let exclusive = first
        .lineage
        .iter()
        .zip(&second.lineage)
        .any(|(a, b)| matches!((a, b), (Some(a), Some(b)) if a != b));
    if !exclusive {
        if first.field.name != second.field.name {
            return Some(format!(
                "\"{}\" and \"{}\" are different fields",
                first.field.name, second.field.name
            ));
        }
        if !same_arguments(first.field, second.field) {
            return Some("they have different arguments".to_string());
        }
    }
    let ((a, a_nullable), (b, b_nullable)) = (first.shape?, second.shape?);
    shapes_conflict(a, a_nullable, b, b_nullable).then(|| {
        format!(
            "they return conflicting types \"{}\" and \"{}\"",
            rendered(a, a_nullable),
            rendered(b, b_nullable)
        )
    })
}

/// Whether two declared types answer differently: in nullability, in list wrapping, or as
/// two different leaf types. Two composite types answer through their sub-selections.
fn shapes_conflict(a: &FieldType, a_nullable: bool, b: &FieldType, b_nullable: bool) -> bool {
    if a_nullable != b_nullable {
        return true;
    }
    match (a, b) {
        (
            FieldType::List {
                item: a_item,
                nullable_items: a_items_nullable,
            },
            FieldType::List {
                item: b_item,
                nullable_items: b_items_nullable,
            },
        ) => shapes_conflict(a_item, *a_items_nullable, b_item, *b_items_nullable),
        (FieldType::List { .. }, _) | (_, FieldType::List { .. }) => true,
        _ if is_composite(a) && is_composite(b) => false,
        _ => a.to_graphql_string() != b.to_graphql_string(),
    }
}

const fn is_composite(t: &FieldType) -> bool {
    matches!(t, FieldType::Object(_) | FieldType::Interface(_) | FieldType::Union(_))
}

/// A declared type as GraphQL writes it.
fn rendered(t: &FieldType, nullable: bool) -> String {
    let mut s = t.to_graphql_string();
    if !nullable {
        s.push('!');
    }
    s
}

/// Whether two selections pass the same arguments, in any order.
fn same_arguments(a: &FieldSelection, b: &FieldSelection) -> bool {
    a.arguments.len() == b.arguments.len() && sorted_arguments(a) == sorted_arguments(b)
}

/// A selection's arguments as `(name, value)`, by name.
fn sorted_arguments(f: &FieldSelection) -> Vec<(&str, &str)> {
    let mut args: Vec<(&str, &str)> =
        f.arguments.iter().map(|a| (a.name.as_str(), a.value_json.as_str())).collect();
    args.sort_unstable();
    args
}

#[cfg(test)]
#[path = "field_merging_tests.rs"]
mod tests;

//! Federation entities: refusal of an `@key` the router would reject (#1395).
//!
//! An entity's `key_fields` reach the served SDL verbatim — `generate_service_sdl` joins
//! them with spaces into `@key(fields: "…")` — and the router composes the supergraph
//! from that SDL. A key naming a field the type does not publish compiled clean, booted
//! clean, and was then refused by the router with `KEY_INVALID_FIELDS`, far from the
//! declaration that caused it. Nothing in the subgraph checked the key at all:
//! `fraiseql federation check` read a `federation.types` shape that no producer writes,
//! so on every compiled schema it validated nothing and reported success.
//!
//! This is the one decision. `fraiseql compile` reports it with the rest of the schema's
//! problems, and [`CompiledSchema`]'s load path refuses a document that carries one, so
//! a hand-edited artifact cannot reach a server either. `fraiseql federation check`
//! loads through the same path.
//!
//! # What a key is checked against
//!
//! The key is parsed as a GraphQL field set (names and braces; commas are insignificant,
//! as everywhere in GraphQL) and walked through the entity's type — or interface, for an
//! entity interface. Each name must be a field of the type it is selected on, spelled
//! exactly as published: the SDL prints [`FieldDefinition::name`] verbatim, and the router
//! compares exactly, so a case- or style-folding match here would accept the
//! `organization_id` key Apollo refuses when the field is published as `organizationId`.
//! A near miss earns a hint instead.
//!
//! An object field must carry a nested selection and a leaf must not. A list, interface,
//! union or input-object field cannot be part of a key (`KEY_FIELDS_SELECT_INVALID_TYPE`). Aliases,
//! arguments, directives and fragments have no meaning in a key and are refused as
//! malformed.
//!
//! An `embedded` type (#687) is a value object with no identity of its own and no
//! `sql_source`, so `_entities` could never resolve it: it is refused as an entity
//! whatever its key says.
//!
//! [`FieldDefinition::name`]: crate::schema::FieldDefinition::name

use std::{collections::HashSet, fmt};

use super::CompiledSchema;
use crate::{
    runtime::suggest_similar,
    schema::{FieldDefinition, FieldType},
    utils::casing::to_camel_case,
};

/// One reason a federation entity's `@key` cannot be served as declared.
///
/// Every variant names the entity, so a report over many entities stays attributable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FederationKeyProblem {
    /// The entity names neither an object type nor an interface the schema declares.
    UnknownType {
        /// The entity as declared.
        entity:      String,
        /// Declared object types and interfaces spelled like it.
        suggestions: Vec<String>,
    },
    /// The entity is declared twice; only one declaration can reach the SDL.
    DuplicateEntity {
        /// The entity as declared.
        entity: String,
    },
    /// The entity is an `embedded` value type, which has no identity to key on.
    EmbeddedType {
        /// The entity as declared.
        entity: String,
    },
    /// The key selects no field at all.
    EmptyKey {
        /// The entity as declared.
        entity: String,
    },
    /// The key is not a field set: an alias, an argument, a directive, a fragment, an
    /// unbalanced brace or an empty nested selection.
    MalformedFieldSet {
        /// The entity as declared.
        entity:    String,
        /// The key, joined as the SDL renders it.
        field_set: String,
        /// What is wrong with it.
        reason:    String,
    },
    /// The key selects a field the type it is selected on does not declare.
    MissingField {
        /// The entity as declared.
        entity: String,
        /// Dotted path of the missing field from the entity (`org.region`).
        path:   String,
        /// The type the field was looked up on.
        owner:  String,
        /// The published spelling of a near miss (`organizationId` for `organization_id`).
        hint:   Option<String>,
    },
    /// The key gives a nested selection to a field that has no sub-fields.
    SelectionOnLeaf {
        /// The entity as declared.
        entity: String,
        /// Dotted path of the leaf field.
        path:   String,
    },
    /// The key names an object field without selecting any of its fields.
    ObjectWithoutSelection {
        /// The entity as declared.
        entity: String,
        /// Dotted path of the object field.
        path:   String,
    },
    /// The key selects a list, interface, union or input-object field, which no key may
    /// contain.
    InvalidKeyFieldType {
        /// The entity as declared.
        entity: String,
        /// Dotted path of the field.
        path:   String,
        /// `"a list"`, `"an interface"`, `"a union"` or `"an input object"`.
        kind:   &'static str,
    },
    /// A nested selection reaches an object type the schema does not declare.
    UnknownNestedType {
        /// The entity as declared.
        entity:    String,
        /// Dotted path of the field whose type is missing.
        path:      String,
        /// The undeclared type name.
        type_name: String,
    },
}

impl fmt::Display for FederationKeyProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownType {
                entity,
                suggestions,
            } => {
                write!(
                    f,
                    "federation entity '{entity}' names no object type or interface this \
                     schema declares"
                )?;
                if !suggestions.is_empty() {
                    write!(f, " (did you mean: {}?)", suggestions.join(", "))?;
                }
                Ok(())
            },
            Self::DuplicateEntity { entity } => write!(
                f,
                "federation entity '{entity}' is declared more than once; one entity carries \
                 one `@key`, so every declaration after the first would be dropped from the SDL"
            ),
            Self::EmbeddedType { entity } => write!(
                f,
                "federation entity '{entity}' is an embedded value type: it has no identity of \
                 its own and no `sql_source`, so `_entities` could never resolve it. Drop it \
                 from `federation.entities` (it is served as part of its parent)"
            ),
            Self::EmptyKey { entity } => {
                write!(f, "federation entity '{entity}' has an `@key` that selects no field")
            },
            Self::MalformedFieldSet {
                entity,
                field_set,
                reason,
            } => write!(
                f,
                "federation entity '{entity}' has `@key(fields: \"{field_set}\")`, which is not \
                 a field set: {reason}"
            ),
            Self::MissingField {
                entity,
                path,
                owner,
                hint,
            } => {
                write!(
                    f,
                    "federation entity '{entity}' is keyed on '{path}', but type '{owner}' has \
                     no field '{}'; the router would refuse the supergraph with \
                     KEY_INVALID_FIELDS",
                    path.rsplit('.').next().unwrap_or(path)
                )?;
                if let Some(hint) = hint {
                    write!(f, " (did you mean '{hint}'? keys name the published field)")?;
                }
                Ok(())
            },
            Self::SelectionOnLeaf { entity, path } => write!(
                f,
                "federation entity '{entity}' selects sub-fields of '{path}', which has none"
            ),
            Self::ObjectWithoutSelection { entity, path } => write!(
                f,
                "federation entity '{entity}' is keyed on '{path}', an object; a key must \
                 select the object's fields, e.g. '{path} {{ id }}'"
            ),
            Self::InvalidKeyFieldType { entity, path, kind } => write!(
                f,
                "federation entity '{entity}' is keyed on '{path}', which is {kind}; a key \
                 cannot contain a list, interface, union or input-object field \
                 (KEY_FIELDS_SELECT_INVALID_TYPE)"
            ),
            Self::UnknownNestedType {
                entity,
                path,
                type_name,
            } => write!(
                f,
                "federation entity '{entity}' selects into '{path}', whose type '{type_name}' \
                 this schema does not declare"
            ),
        }
    }
}

/// A parsed field-set selection: a field name and its (possibly empty) sub-selection.
#[derive(Debug)]
struct Selection {
    name:     String,
    children: Vec<Selection>,
}

/// Parse a federation field set. Names and braces only; commas and whitespace separate.
fn parse_field_set(input: &str) -> Result<Vec<Selection>, String> {
    let mut stack: Vec<Vec<Selection>> = vec![Vec::new()];
    let mut chars = input.char_indices().peekable();

    while let Some(&(start, c)) = chars.peek() {
        if c.is_whitespace() || c == ',' {
            chars.next();
        } else if c == '_' || c.is_ascii_alphabetic() {
            let mut end = start;
            while let Some(&(i, c)) = chars.peek() {
                if c == '_' || c.is_ascii_alphanumeric() {
                    end = i + c.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }
            let level = stack.last_mut().ok_or("unbalanced braces")?;
            level.push(Selection {
                name:     input[start..end].to_string(),
                children: Vec::new(),
            });
        } else if c == '{' {
            chars.next();
            let has_parent = stack
                .last()
                .is_some_and(|level| level.last().is_some_and(|s| s.children.is_empty()));
            if !has_parent {
                return Err("'{' must follow the field it selects into".to_string());
            }
            stack.push(Vec::new());
        } else if c == '}' {
            chars.next();
            if stack.len() < 2 {
                return Err("unbalanced braces".to_string());
            }
            let children = stack.pop().unwrap_or_default();
            if children.is_empty() {
                return Err("an empty selection '{}'".to_string());
            }
            let parent =
                stack.last_mut().and_then(|level| level.last_mut()).ok_or("unbalanced braces")?;
            parent.children = children;
        } else {
            return Err(format!(
                "unexpected '{c}' (aliases, arguments, directives and fragments have no \
                 meaning in a key)"
            ));
        }
    }

    if stack.len() != 1 {
        return Err("unbalanced braces".to_string());
    }
    Ok(stack.pop().unwrap_or_default())
}

/// The field a key selects on `fields`, or the near miss the author probably meant.
fn lookup<'a>(
    fields: &'a [FieldDefinition],
    name: &str,
) -> Result<&'a FieldDefinition, Option<String>> {
    if let Some(field) = fields.iter().find(|f| f.name == name) {
        return Ok(field);
    }
    let wanted = to_camel_case(name);
    Err(fields
        .iter()
        .find(|f| to_camel_case(f.name.as_str()) == wanted)
        .map(|f| f.name.to_string()))
}

impl CompiledSchema {
    /// Every reason a federation entity's `@key` cannot be served as declared (#1395).
    ///
    /// Empty when the schema declares no federation block, or every entity is a
    /// declared, non-embedded type or interface whose key selects published fields.
    /// Called from `finish_load` and from `fraiseql compile`, so neither path can accept a
    /// key the other would refuse.
    #[must_use]
    pub fn federation_key_problems(&self) -> Vec<FederationKeyProblem> {
        let Some(federation) = &self.federation else {
            return Vec::new();
        };
        let mut problems = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();

        for entity in &federation.entities {
            let name = entity.name.as_str();
            if !seen.insert(name) {
                problems.push(FederationKeyProblem::DuplicateEntity {
                    entity: name.to_string(),
                });
                continue;
            }

            let fields = if let Some(type_def) = self.find_type(name) {
                if type_def.embedded {
                    problems.push(FederationKeyProblem::EmbeddedType {
                        entity: name.to_string(),
                    });
                    continue;
                }
                &type_def.fields
            } else if let Some(interface) = self.find_interface(name) {
                &interface.fields
            } else {
                let known: Vec<&str> = self
                    .types
                    .iter()
                    .map(|t| t.name.as_str())
                    .chain(self.interfaces.iter().map(|i| i.name.as_str()))
                    .collect();
                problems.push(FederationKeyProblem::UnknownType {
                    entity:      name.to_string(),
                    suggestions: suggest_similar(name, &known)
                        .into_iter()
                        .map(ToString::to_string)
                        .collect(),
                });
                continue;
            };

            let field_set = entity.key_fields.join(" ");
            match parse_field_set(&field_set) {
                Err(reason) => problems.push(FederationKeyProblem::MalformedFieldSet {
                    entity: name.to_string(),
                    field_set,
                    reason,
                }),
                Ok(selections) if selections.is_empty() => {
                    problems.push(FederationKeyProblem::EmptyKey {
                        entity: name.to_string(),
                    });
                },
                Ok(selections) => {
                    self.check_selections(name, name, "", fields, &selections, &mut problems);
                },
            }
        }
        problems
    }

    /// [`Self::federation_key_problems`], one sentence each.
    #[must_use]
    pub fn federation_key_violations(&self) -> Vec<String> {
        self.federation_key_problems().iter().map(ToString::to_string).collect()
    }

    fn check_selections(
        &self,
        entity: &str,
        owner: &str,
        prefix: &str,
        fields: &[FieldDefinition],
        selections: &[Selection],
        problems: &mut Vec<FederationKeyProblem>,
    ) {
        for selection in selections {
            let path = if prefix.is_empty() {
                selection.name.clone()
            } else {
                format!("{prefix}.{}", selection.name)
            };
            let field = match lookup(fields, &selection.name) {
                Ok(field) => field,
                Err(hint) => {
                    problems.push(FederationKeyProblem::MissingField {
                        entity: entity.to_string(),
                        path,
                        owner: owner.to_string(),
                        hint,
                    });
                    continue;
                },
            };

            let invalid = |kind| FederationKeyProblem::InvalidKeyFieldType {
                entity: entity.to_string(),
                path: path.clone(),
                kind,
            };
            match &field.field_type {
                FieldType::List(_) => problems.push(invalid("a list")),
                FieldType::Interface(_) => problems.push(invalid("an interface")),
                FieldType::Union(_) => problems.push(invalid("a union")),
                FieldType::Input(_) => problems.push(invalid("an input object")),
                FieldType::Object(type_name) => {
                    if selection.children.is_empty() {
                        problems.push(FederationKeyProblem::ObjectWithoutSelection {
                            entity: entity.to_string(),
                            path,
                        });
                    } else if let Some(nested) = self.find_type(type_name) {
                        self.check_selections(
                            entity,
                            type_name,
                            &path,
                            &nested.fields,
                            &selection.children,
                            problems,
                        );
                    } else {
                        problems.push(FederationKeyProblem::UnknownNestedType {
                            entity: entity.to_string(),
                            path,
                            type_name: type_name.clone(),
                        });
                    }
                },
                FieldType::String
                | FieldType::Int
                | FieldType::Float
                | FieldType::Boolean
                | FieldType::Id
                | FieldType::DateTime
                | FieldType::Date
                | FieldType::Time
                | FieldType::Json
                | FieldType::Uuid
                | FieldType::Decimal
                | FieldType::Vector
                | FieldType::BitVector
                | FieldType::HalfVector
                | FieldType::SparseVector
                | FieldType::Scalar(_)
                | FieldType::Enum(_) => {
                    if !selection.children.is_empty() {
                        problems.push(FederationKeyProblem::SelectionOnLeaf {
                            entity: entity.to_string(),
                            path,
                        });
                    }
                },
            }
        }
    }
}

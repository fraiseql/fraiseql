//! Declared scalar types for the fields a WHERE clause filters on.

use std::{collections::HashMap, sync::Arc};

use serde::{Deserialize, Serialize};

use crate::types::sql_hints::ScalarFieldType;

/// The declared scalar type of every field a WHERE clause may reference,
/// keyed by the dotted snake_case storage path (`"created_at"`,
/// `"machine.serial_number"`).
///
/// A JSON extraction is always `text`, so the generator has to cast before the
/// database can compare it as a number, an instant or a boolean. Which cast is
/// correct is a property of the *field*, not of the operator — deciding it from
/// the operator is what made every date, UUID and string range filter a hard
/// SQL error (#798) and made `in: [19.9]` miss rows `eq: 19.9` matched (#800).
///
/// A path that is absent from the map has no declared type; the generator then
/// falls back to the shape of the supplied JSON value.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldTypeMap(HashMap<String, ScalarFieldType>);

impl FieldTypeMap {
    /// Build a map from `(dotted snake_case path, type)` pairs.
    #[must_use]
    pub fn from_pairs<I, S>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (S, ScalarFieldType)>,
        S: Into<String>,
    {
        Self(pairs.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }

    /// Declared type of `path`, or `None` when the field is not declared.
    #[must_use]
    pub fn get(&self, path: &[String]) -> Option<ScalarFieldType> {
        if path.len() == 1 {
            // Reason: length checked on the line above.
            return self.0.get(path.first()?).copied();
        }
        self.0.get(&path.join(".")).copied()
    }

    /// `true` when no field carries a declared type.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Number of declared fields.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl FromIterator<(String, ScalarFieldType)> for FieldTypeMap {
    fn from_iter<I: IntoIterator<Item = (String, ScalarFieldType)>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

/// Shared handle to a [`FieldTypeMap`], as carried by [`WhereClause::Typed`].
///
/// [`WhereClause::Typed`]: crate::WhereClause::Typed
pub type SharedFieldTypes = Arc<FieldTypeMap>;

/// What the schema knows about one `where` key.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WhereFieldInfo {
    /// The name as the schema declares it, used for "did you mean" hints
    /// because that is the spelling a client writes.
    pub declared_name: String,
    /// Whether `{field: {sub: {eq: …}}}` is a legitimate nested relation filter
    /// on this field rather than a scalar handed a bogus operator.
    ///
    /// This is the same question the published `{Entity}WhereInput` answers by
    /// giving the field a nested filter type instead of an operator bag, and it
    /// must be answered the same way: a field is a relation exactly when the
    /// derived surface publishes a nested `WhereInput` for it.
    pub is_relation:   bool,
    /// The declared type a nested predicate on this field filters, when the
    /// schema can name it.
    ///
    /// `None` on a scalar, and on a relation whose target the caller could not
    /// resolve — the nested level is then unadjudicated rather than refused.
    pub relation_type: Option<String>,
}

/// The declared `where` keys of every type a nested predicate can descend into,
/// by declared type name.
///
/// Prebuilt once per compiled schema rather than per request: a nested level is
/// adjudicated against the *target* type's keys, and rebuilding the reachable
/// closure on every query would put a map-per-type allocation in front of every
/// filter.
pub type RelationFieldMaps = Arc<HashMap<String, Arc<HashMap<String, WhereFieldInfo>>>>;

/// The `where` keys a type declares, alongside their casts.
///
/// # Why this is not just [`FieldTypeMap`]
///
/// `FieldTypeMap` cannot distinguish *"this type has no fields"* from *"this
/// type was not found"* — both produce an empty map, and the generator reads
/// empty as *"no type information, skip the casts"*. Reusing it as an allowlist
/// would therefore fail **open** on a missing type, or — if inverted — fail
/// **closed** on every schema with no field metadata.
///
/// So the "cannot adjudicate" state is a distinct value here (`known: None`)
/// rather than a property of an empty collection. Every consumer has to name
/// that branch explicitly.
#[derive(Debug, Clone, Default)]
pub struct WhereFieldSchema {
    /// Declared casts, keyed by dotted snake_case path.
    casts:     SharedFieldTypes,
    /// Declared top-level keys, by snake_case name.
    ///
    /// `None` means the schema could not adjudicate — the type was not found,
    /// or it carries no field metadata. It does **not** mean "no keys".
    known:     Option<Arc<HashMap<String, WhereFieldInfo>>>,
    /// Keys of every type a nested predicate can descend into.
    ///
    /// Empty is the honest default for a caller that knows the entry type but
    /// nothing beyond it — the nested levels then pass unadjudicated, which is
    /// what this did at every depth before the derived filter surface made the
    /// target type nameable.
    relations: RelationFieldMaps,
}

impl WhereFieldSchema {
    /// A schema that cannot adjudicate field names: casts only, no allowlist.
    ///
    /// This is the honest constructor for a caller that has type information but
    /// no type *definition* — fuzzers, benchmarks, and the wire path.
    #[must_use]
    pub fn casts_only(casts: SharedFieldTypes) -> Self {
        Self {
            casts,
            known: None,
            relations: RelationFieldMaps::default(),
        }
    }

    /// A schema that can adjudicate its **top level** and nothing below it.
    ///
    /// For callers that know the entry type's keys but cannot name what a
    /// relation points at. See [`with_relations`](Self::with_relations) for the
    /// compiled-schema path.
    #[must_use]
    pub fn with_known_keys(
        casts: SharedFieldTypes,
        known: HashMap<String, WhereFieldInfo>,
    ) -> Self {
        Self {
            casts,
            known: Some(Arc::new(known)),
            relations: RelationFieldMaps::default(),
        }
    }

    /// A schema that can adjudicate every level a nested predicate reaches.
    ///
    /// `relations` maps a declared type name to that type's `where` keys, so a
    /// key at depth *n* is scored against the type the path actually arrived at
    /// rather than passed through unchecked.
    #[must_use]
    pub fn with_relations(
        casts: SharedFieldTypes,
        known: HashMap<String, WhereFieldInfo>,
        relations: RelationFieldMaps,
    ) -> Self {
        Self {
            casts,
            known: Some(Arc::new(known)),
            relations,
        }
    }

    /// The declared casts.
    #[must_use]
    pub const fn casts(&self) -> &SharedFieldTypes {
        &self.casts
    }

    /// What the schema knows about `snake_key`, or `None` when it cannot
    /// adjudicate **or** the key is undeclared — use [`Self::can_adjudicate`] to
    /// tell those apart.
    #[must_use]
    pub fn lookup(&self, snake_key: &str) -> Option<&WhereFieldInfo> {
        self.known.as_ref()?.get(snake_key)
    }

    /// Whether the schema carries enough information to reject an unknown key.
    #[must_use]
    pub const fn can_adjudicate(&self) -> bool {
        self.known.is_some()
    }

    /// Declared names, for "did you mean" hints.
    #[must_use]
    pub fn declared_names(&self) -> Vec<&str> {
        self.known
            .as_ref()
            .map_or_else(Vec::new, |k| k.values().map(|i| i.declared_name.as_str()).collect())
    }

    /// The entry level a `where` object is parsed against.
    #[must_use]
    pub fn root_level(&self) -> Option<&HashMap<String, WhereFieldInfo>> {
        self.known.as_deref()
    }

    /// The level a nested predicate on a field of type `type_name` descends
    /// into, or `None` when this schema carries no keys for it.
    #[must_use]
    pub fn level_of(&self, type_name: &str) -> Option<&HashMap<String, WhereFieldInfo>> {
        self.relations.get(type_name).map(Arc::as_ref)
    }
}

/// Resolve the SQL type a comparison against `path` must be made in.
///
/// The declared type wins whenever the compiled schema names the field: it is
/// the only source that knows a JSON string is an instant rather than text.
/// When the field is not declared — a nested relation path, an RLS-composed
/// condition, a library embedder with no schema — the shape of the supplied
/// JSON value is the best available signal.
///
/// For a containment operand (`in` / `nin`) the array's *elements* carry the
/// shape; a mixed array falls back to text, which keeps the comparison
/// well-typed rather than erroring on the first element.
///
/// Both WHERE generators call this, so the parameterised path and the
/// raw-SQL wire path cannot disagree about how a field is typed.
#[must_use]
pub fn operand_type(
    types: Option<&FieldTypeMap>,
    path: &[String],
    value: &serde_json::Value,
) -> ScalarFieldType {
    if let Some(declared) = types.and_then(|t| t.get(path)) {
        return declared;
    }
    match value.as_array() {
        Some(items) => {
            let first = items.first().map_or(ScalarFieldType::Text, value_shape);
            if items.iter().all(|v| value_shape(v) == first) {
                first
            } else {
                ScalarFieldType::Text
            }
        },
        None => value_shape(value),
    }
}

/// Best-effort scalar type for an operand that has no declared type.
///
/// Only the JSON shape is available, so a date is indistinguishable from any
/// other string and compares as text. A declared type always wins over this.
#[must_use]
pub fn value_shape(value: &serde_json::Value) -> ScalarFieldType {
    if value.is_number() {
        ScalarFieldType::Numeric
    } else if value.is_boolean() {
        ScalarFieldType::Boolean
    } else {
        ScalarFieldType::Text
    }
}

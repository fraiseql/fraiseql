//! Whether an entity's stored document holds a dotted path.
//!
//! A webhook `body_template` names values of the changed row by dotted path. The row is
//! read as the entity document, the type's fields at the top level, each under the key the
//! document holds it by ([`FieldDefinition::stored_key`]). One definition, asked by the
//! compiler for a declared observer and by the admin API for a stored one, so the two
//! cannot disagree on what a template may name.

use super::CompiledSchema;
use crate::schema::{FieldDefinition, FieldType, TypeDefinition};

/// A dotted path no document of the entity can fill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnfillablePath {
    /// The key the document holds, when the segment that failed is a field's published
    /// name: `review_status` for a path that named `reviewStatus`.
    pub held: Option<String>,
}

impl CompiledSchema {
    /// Whether a document of `entity` can fill the dotted `path`: every segment names a
    /// field by the key the document holds for it.
    ///
    /// The document is what a mutation stores, and what the capture trigger stores for a
    /// table declared to keep its document in a column (`[subscriptions.capture]`). A table
    /// of plain columns is captured as its row, which holds a field only where a column
    /// carries its stored key.
    ///
    /// A path that descends into JSON, a custom scalar, an interface, a union or a type the
    /// schema does not declare is accepted: the schema cannot say what they hold.
    ///
    /// # Errors
    ///
    /// Returns [`UnfillablePath`] for the first segment that names no field, or that reads
    /// a key out of a list or a scalar.
    pub fn document_path(&self, entity: &TypeDefinition, path: &str) -> Result<(), UnfillablePath> {
        let mut current = entity;
        let mut segments = path.split('.').peekable();
        while let Some(segment) = segments.next() {
            let Some(field) = current.fields.iter().find(|f| f.stored_key() == segment) else {
                let held = current
                    .fields
                    .iter()
                    .find(|f| f.name.as_str() == segment)
                    .map(FieldDefinition::stored_key);
                return Err(UnfillablePath { held });
            };
            if segments.peek().is_none() {
                return Ok(());
            }
            match &field.field_type {
                FieldType::Object(name) => match self.find_type(name) {
                    Some(next) => current = next,
                    None => return Ok(()),
                },
                FieldType::Json
                | FieldType::Scalar(_)
                | FieldType::Interface(_)
                | FieldType::Union(_) => return Ok(()),
                // A list or a scalar has no key to read the next segment from.
                _ => return Err(UnfillablePath { held: None }),
            }
        }
        Ok(())
    }

    /// What to tell the author of a `body_template` whose `path` a captured row of
    /// `entity` cannot fill, though its document can.
    ///
    /// A type's change is stored as its document by a mutation and, for a table with no
    /// `[subscriptions.capture]` entry, as the table's row by the capture trigger. The row
    /// holds columns: a field the type embeds (an object, an interface, a union, or a list
    /// of one) is in the document and not in the row. `None` when the path starts at no
    /// such field, or when no table of the type is captured as its row.
    ///
    /// The schema does not hold a table's columns, so this says nothing of a scalar field:
    /// a column usually carries its stored key.
    #[must_use]
    pub fn captured_row_warning(&self, entity: &TypeDefinition, path: &str) -> Option<String> {
        let first = path.split('.').next()?;
        let table = self.row_captured_embedded(entity, first)?;
        Some(format!(
            "`body_template` placeholder `{{{{ {path} }}}}` reads `{first}`, which `{}` \
             embeds. The table `{table}` has no `[subscriptions.capture.{table}]` entry, so \
             a write to it made outside a mutation is stored as the table's row, which holds \
             no embedded field: that event would be dead-lettered at delivery. A mutation's \
             event fills the placeholder.",
            entity.name
        ))
    }
}

impl CompiledSchema {
    /// The table a captured row of `entity` comes from, when `key` is a field the type
    /// embeds and such a row cannot hold.
    fn row_captured_embedded(&self, entity: &TypeDefinition, key: &str) -> Option<&str> {
        let field = entity.fields.iter().find(|f| f.stored_key() == key)?;
        if !embeds(&field.field_type) {
            return None;
        }
        self.row_captured_table(entity.name.as_str())
    }

    /// Whether an observer's condition on `entity` can read `key` from the events it is
    /// evaluated against.
    ///
    /// A condition reads one top-level key of the event, literally. A mutation's event is
    /// the entity document, its fields under their stored keys. For a table of the type
    /// with no `[subscriptions.capture]` entry, a write made outside a mutation is stored
    /// as the table's row, whose keys are its columns: the schema does not hold them.
    ///
    /// Returns `Ok(None)` for a key every event holds, and `Ok(Some(warning))` for one
    /// only some events hold: a field the type embeds, which a captured row lacks, or, on
    /// a type with a row-captured table, a key no field holds, which only such a row can
    /// supply as a column.
    ///
    /// # Errors
    ///
    /// Returns the refusal for a key no event can hold: one with a dot in it, or one no
    /// field holds on a type whose document is its only image. The refusal names the
    /// field's stored key when `key` is its published name.
    pub fn condition_key(
        &self,
        entity: &TypeDefinition,
        key: &str,
        read: ConditionRead,
    ) -> Result<Option<String>, String> {
        let on_absent = read.on_absent();
        let type_name = &entity.name;
        if key.contains('.') {
            return Err(format!(
                "`condition` reads `{key}`: a condition reads one top-level key of the \
                 event, and no event of `{type_name}` holds a key with a dot in it, so \
                 {on_absent} for every event. A condition cannot read a nested field."
            ));
        }
        let table = self.row_captured_table(type_name.as_str());
        if let Err(unknown) = self.document_path(entity, key) {
            let held = unknown.held.map_or_else(String::new, |stored| {
                format!(" The document holds that field as `{stored}`.")
            });
            return match table {
                None => Err(format!(
                    "`condition` reads `{key}`, which names no field of `{type_name}`: \
                     {on_absent} for every event.{held}"
                )),
                Some(table) => Ok(Some(format!(
                    "`condition` reads `{key}`, which is not a field of `{type_name}`. Only \
                     a captured row of the table `{table}` can supply it, as a column: for \
                     a mutation's event, {on_absent}.{held}"
                ))),
            };
        }
        Ok(self.row_captured_embedded(entity, key).map(|table| {
            format!(
                "`condition` reads `{key}`, which `{type_name}` embeds. The table `{table}` \
                 has no `[subscriptions.capture.{table}]` entry, so a write to it made \
                 outside a mutation is stored as the table's row, which holds no embedded \
                 field: for that event, {on_absent}. A mutation's event holds it."
            )
        }))
    }
}

/// How an observer's condition reads a key of the event: a comparison reads its value,
/// `has_field` its presence, `field_changed*` its change.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditionRead {
    /// A comparison, which fails to evaluate on an event without the key.
    Value,
    /// `has_field`, false on an event without the key.
    Presence,
    /// `field_changed`, `field_changed_to` or `field_changed_from`, which see no change of
    /// a key the event lacks.
    Change,
}

impl ConditionRead {
    /// What the read comes to on an event without the key.
    const fn on_absent(self) -> &'static str {
        match self {
            Self::Value => "the condition fails to evaluate and the observer's actions do not run",
            Self::Presence => "`has_field` is false",
            Self::Change => "no change of the key is seen",
        }
    }
}

/// Whether a field of this type is built by the type's read from another relation, and so
/// is no column of the type's table.
fn embeds(field_type: &FieldType) -> bool {
    match field_type {
        FieldType::Object(_) | FieldType::Interface(_) | FieldType::Union(_) => true,
        FieldType::List { item, .. } => embeds(item),
        _ => false,
    }
}

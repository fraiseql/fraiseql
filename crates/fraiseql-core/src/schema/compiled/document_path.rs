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
        let field = entity.fields.iter().find(|f| f.stored_key() == first)?;
        if !embeds(&field.field_type) {
            return None;
        }
        let table = self.row_captured_table(entity.name.as_str())?;
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

/// Whether a field of this type is built by the type's read from another relation, and so
/// is no column of the type's table.
fn embeds(field_type: &FieldType) -> bool {
    match field_type {
        FieldType::Object(_) | FieldType::Interface(_) | FieldType::Union(_) => true,
        FieldType::List { item, .. } => embeds(item),
        _ => false,
    }
}

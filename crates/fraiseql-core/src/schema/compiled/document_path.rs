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
}

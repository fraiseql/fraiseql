//! Result schema validation

use crate::protocol::{BackendMessage, FieldDescription};
use crate::util::oid::is_json_oid;
use crate::{Result, WireError};

/// Validate that `RowDescription` matches our requirements
///
/// # Errors
///
/// Returns [`WireError::Protocol`] if the message is not a `RowDescription`.
/// Returns [`WireError::InvalidSchema`] if the description does not have exactly one column
/// named `"data"` with a JSON or JSONB type OID.
pub fn validate_row_description(msg: &BackendMessage) -> Result<()> {
    let fields = match msg {
        BackendMessage::RowDescription(fields) => fields,
        _ => return Err(WireError::Protocol("expected RowDescription".into())),
    };

    // Must have exactly one column
    if fields.len() != 1 {
        return Err(WireError::InvalidSchema(format!(
            "expected 1 column, got {}",
            fields.len()
        )));
    }

    let field = &fields[0];

    // Column must be named "data"
    if field.name != "data" {
        return Err(WireError::InvalidSchema(format!(
            "expected column named 'data', got '{}'",
            field.name
        )));
    }

    // Type must be json or jsonb
    if !is_json_oid(field.type_oid) {
        return Err(WireError::InvalidSchema(format!(
            "expected json/jsonb type, got OID {}",
            field.type_oid
        )));
    }

    Ok(())
}

/// Extract field description from `RowDescription`
///
/// # Errors
///
/// Returns [`WireError::Protocol`] if the message is not a `RowDescription`.
pub fn extract_field_description(msg: &BackendMessage) -> Result<FieldDescription> {
    let fields = match msg {
        BackendMessage::RowDescription(fields) => fields,
        _ => return Err(WireError::Protocol("expected RowDescription".into())),
    };

    Ok(fields[0].clone())
}

#[cfg(test)]
mod tests;

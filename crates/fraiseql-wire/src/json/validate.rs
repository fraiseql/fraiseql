//! Result schema validation

use crate::protocol::{BackendMessage, FieldDescription};
use crate::util::oid::is_json_oid;
use crate::{Error, Result};

/// Validate that RowDescription matches our requirements
pub fn validate_row_description(msg: &BackendMessage) -> Result<()> {
    let fields = match msg {
        BackendMessage::RowDescription(fields) => fields,
        _ => return Err(Error::Protocol("expected RowDescription".into())),
    };

    // Must have exactly one column
    if fields.len() != 1 {
        return Err(Error::InvalidSchema(format!(
            "expected 1 column, got {}",
            fields.len()
        )));
    }

    let field = &fields[0];

    // Column must be named "data"
    if field.name != "data" {
        return Err(Error::InvalidSchema(format!(
            "expected column named 'data', got '{}'",
            field.name
        )));
    }

    // Type must be json or jsonb
    if !is_json_oid(field.type_oid) {
        return Err(Error::InvalidSchema(format!(
            "expected json/jsonb type, got OID {}",
            field.type_oid
        )));
    }

    Ok(())
}

/// Extract field description from RowDescription
pub fn extract_field_description(msg: &BackendMessage) -> Result<FieldDescription> {
    let fields = match msg {
        BackendMessage::RowDescription(fields) => fields,
        _ => return Err(Error::Protocol("expected RowDescription".into())),
    };

    Ok(fields[0].clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::oid::JSON_OID;

    #[test]
    fn test_valid_row_description() {
        let field = FieldDescription {
            name: "data".to_string(),
            table_oid: 0,
            column_attr: 0,
            type_oid: JSON_OID,
            type_size: -1,
            type_modifier: -1,
            format_code: 0,
        };

        let msg = BackendMessage::RowDescription(vec![field]);
        assert!(validate_row_description(&msg).is_ok());
    }

    #[test]
    fn test_wrong_column_name() {
        let field = FieldDescription {
            name: "wrong".to_string(),
            table_oid: 0,
            column_attr: 0,
            type_oid: JSON_OID,
            type_size: -1,
            type_modifier: -1,
            format_code: 0,
        };

        let msg = BackendMessage::RowDescription(vec![field]);
        assert!(validate_row_description(&msg).is_err());
    }

    #[test]
    fn test_wrong_type() {
        let field = FieldDescription {
            name: "data".to_string(),
            table_oid: 0,
            column_attr: 0,
            type_oid: 23, // INT4
            type_size: 4,
            type_modifier: -1,
            format_code: 0,
        };

        let msg = BackendMessage::RowDescription(vec![field]);
        assert!(validate_row_description(&msg).is_err());
    }

    #[test]
    fn test_multiple_columns() {
        let field1 = FieldDescription {
            name: "data".to_string(),
            table_oid: 0,
            column_attr: 0,
            type_oid: JSON_OID,
            type_size: -1,
            type_modifier: -1,
            format_code: 0,
        };
        let field2 = field1.clone();

        let msg = BackendMessage::RowDescription(vec![field1, field2]);
        assert!(validate_row_description(&msg).is_err());
    }
}

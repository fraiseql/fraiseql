//! Postgres Object Identifier (OID) constants
//!
//! OIDs identify data types in the Postgres wire protocol.

/// Postgres type OID
pub type OID = u32;

/// JSON type OID
pub const JSON_OID: OID = 114;

/// JSONB type OID
pub const JSONB_OID: OID = 3802;

/// Check if an OID represents a JSON type
#[inline]
pub const fn is_json_oid(oid: OID) -> bool {
    oid == JSON_OID || oid == JSONB_OID
}

#[cfg(test)]
mod tests;

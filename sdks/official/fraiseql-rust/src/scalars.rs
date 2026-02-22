//! Built-in FraiseQL scalar types.
//!
//! These mirror the scalars available in the Python and TypeScript SDKs.
//! Use them as field types in `#[fraiseql_type]` structs.
//!
//! # Example
//!
//! ```rust,ignore
//! use fraiseql_rust::prelude::*;
//! use fraiseql_rust::scalars::Uuid;
//!
//! #[fraiseql_type]
//! struct User {
//!     id: Uuid,
//!     created_at: DateTime,
//! }
//! ```

use serde::{Deserialize, Serialize};

/// Maps a Rust scalar wrapper to its FraiseQL GraphQL type name.
pub trait FraiseQLScalar {
    fn graphql_type_name() -> &'static str;
}

macro_rules! define_scalar {
    ($name:ident, $inner:ty, $graphql_name:literal, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub $inner);

        impl FraiseQLScalar for $name {
            fn graphql_type_name() -> &'static str {
                $graphql_name
            }
        }

        impl From<$inner> for $name {
            fn from(v: $inner) -> Self { Self(v) }
        }
    };
}

// ── Core scalars ─────────────────────────────────────────────────────────────

define_scalar!(ID,       String,  "ID",       "GraphQL `ID` scalar — opaque identifier.");
define_scalar!(Uuid,     String,  "UUID",     "RFC 4122 UUID string.");
define_scalar!(DateTime, String,  "DateTime", "ISO 8601 datetime with timezone.");
define_scalar!(Date,     String,  "Date",     "ISO 8601 calendar date (YYYY-MM-DD).");
define_scalar!(Time,     String,  "Time",     "ISO 8601 time of day (HH:MM:SS[.sss]).");
define_scalar!(Json,     serde_json::Value, "JSON", "Arbitrary JSON value.");

/// Arbitrary-precision decimal number, serialised as a string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Decimal(pub String);

impl FraiseQLScalar for Decimal {
    fn graphql_type_name() -> &'static str { "Decimal" }
}

// ── Numeric scalars ───────────────────────────────────────────────────────────

define_scalar!(BigInt,   i64, "BigInt",  "64-bit signed integer.");
define_scalar!(Long,     i64, "Long",    "Alias for BigInt.");
define_scalar!(PositiveInt, i32, "PositiveInt", "Integer that must be > 0.");
define_scalar!(NonNegativeInt, i32, "NonNegativeInt", "Integer that must be ≥ 0.");
/// 32-bit IEEE 754 float.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Float32(pub f32);

impl FraiseQLScalar for Float32 {
    fn graphql_type_name() -> &'static str { "Float32" }
}

impl From<f32> for Float32 {
    fn from(v: f32) -> Self { Self(v) }
}

// ── Network / identity scalars ────────────────────────────────────────────────

define_scalar!(Url,         String, "URL",         "RFC 3986 URL string.");
define_scalar!(EmailAddress, String, "EmailAddress", "RFC 5322 email address.");
define_scalar!(IPv4,        String, "IPv4",        "IPv4 address.");
define_scalar!(IPv6,        String, "IPv6",        "IPv6 address.");
define_scalar!(MacAddress,  String, "MACAddress",  "IEEE 802 MAC address.");

// ── Geospatial scalars ────────────────────────────────────────────────────────

/// WGS-84 latitude in degrees (−90 to 90).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Latitude(pub f64);
impl FraiseQLScalar for Latitude {
    fn graphql_type_name() -> &'static str { "Latitude" }
}
impl From<f64> for Latitude { fn from(v: f64) -> Self { Self(v) } }

/// WGS-84 longitude in degrees (−180 to 180).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Longitude(pub f64);
impl FraiseQLScalar for Longitude {
    fn graphql_type_name() -> &'static str { "Longitude" }
}
impl From<f64> for Longitude { fn from(v: f64) -> Self { Self(v) } }

// ── Vector / ML scalars ───────────────────────────────────────────────────────

/// Fixed-dimension float vector for ML embeddings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Vector(pub Vec<f32>);

impl FraiseQLScalar for Vector {
    fn graphql_type_name() -> &'static str { "Vector" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scalar_graphql_names() {
        assert_eq!(ID::graphql_type_name(), "ID");
        assert_eq!(Uuid::graphql_type_name(), "UUID");
        assert_eq!(DateTime::graphql_type_name(), "DateTime");
        assert_eq!(Decimal::graphql_type_name(), "Decimal");
        assert_eq!(Vector::graphql_type_name(), "Vector");
    }

    #[test]
    fn test_scalar_roundtrip_serialization() {
        let id = ID("user-123".to_string());
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, r#""user-123""#);

        let back: ID = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }

    #[test]
    fn test_vector_roundtrip() {
        let v = Vector(vec![0.1, 0.2, 0.3]);
        let json = serde_json::to_string(&v).unwrap();
        let back: Vector = serde_json::from_str(&json).unwrap();
        assert_eq!(back.0.len(), 3);
    }
}

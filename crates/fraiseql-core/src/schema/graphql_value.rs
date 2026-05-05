//! Typed GraphQL literal values.
//!
//! [`GraphQLValue`] represents any valid GraphQL default value per the spec (§2.9).
//! It replaces `serde_json::Value` at default-value sites so that invalid shapes
//! (e.g. `{"__type": "unresolvable"}`) can be detected at schema compile time
//! rather than at query execution time.
//!
//! # Wire format
//!
//! Uses `#[serde(untagged)]` so the JSON representation is identical to a plain
//! `serde_json::Value`: `10` → `Int(10)`, `true` → `Boolean(true)`, etc.
//! Existing compiled schemas require no migration.

use std::fmt;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::error::{FraiseQLError, Result};

/// A typed GraphQL literal value (spec §2.9).
///
/// Used for `default_value` in argument and input-field definitions.  The enum
/// covers every kind that the GraphQL spec permits as a default: null, boolean,
/// integer, float, string, enum value (stored as `String`), list, and object.
///
/// # Serialization
///
/// Serializes to / deserializes from plain JSON (no wrapper object), so
/// `GraphQLValue::Int(42)` round-trips through JSON as `42` and
/// `GraphQLValue::Boolean(true)` as `true`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum GraphQLValue {
    /// `null`
    Null,
    /// `true` / `false`
    Boolean(bool),
    /// Integer literals (e.g. `42`, `-1`).
    Int(i64),
    /// Float literals (e.g. `3.14`). Only reached when the JSON number cannot
    /// be losslessly represented as i64.
    Float(f64),
    /// String or enum-variant literals.  Both are JSON strings; callers that
    /// need to distinguish enum values from string values must check the
    /// argument's declared type.
    String(String),
    /// List literals (`[1, 2, 3]`).
    List(Vec<GraphQLValue>),
    /// Input-object literals (`{key: value}`).
    Object(IndexMap<String, GraphQLValue>),
}

impl GraphQLValue {
    /// Convert to an equivalent `serde_json::Value` for wire serialization.
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Self::Null => serde_json::Value::Null,
            Self::Boolean(b) => serde_json::Value::Bool(*b),
            Self::Int(i) => serde_json::json!(*i),
            Self::Float(f) => serde_json::json!(*f),
            Self::String(s) => serde_json::Value::String(s.clone()),
            Self::List(v) => serde_json::Value::Array(v.iter().map(Self::to_json).collect()),
            Self::Object(m) => {
                serde_json::Value::Object(m.iter().map(|(k, v)| (k.clone(), v.to_json())).collect())
            },
        }
    }

    /// Parse from a `serde_json::Value`.
    ///
    /// Returns `Err` if the shape is not a valid GraphQL literal.  Currently
    /// all JSON shapes are valid (`Object` maps to an input-object literal),
    /// but this is the validation boundary where future restrictions can be
    /// added.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Validation`] if a nested value cannot be
    /// converted (e.g. a number that overflows i64 and f64).
    pub fn from_json(v: &serde_json::Value) -> Result<Self> {
        match v {
            serde_json::Value::Null => Ok(Self::Null),
            serde_json::Value::Bool(b) => Ok(Self::Boolean(*b)),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(Self::Int(i))
                } else if let Some(f) = n.as_f64() {
                    Ok(Self::Float(f))
                } else {
                    Err(FraiseQLError::Validation {
                        message: format!("default value number out of range: {n}"),
                        path:    None,
                    })
                }
            },
            serde_json::Value::String(s) => Ok(Self::String(s.clone())),
            serde_json::Value::Array(arr) => {
                let items = arr.iter().map(Self::from_json).collect::<Result<Vec<_>>>()?;
                Ok(Self::List(items))
            },
            serde_json::Value::Object(obj) => {
                let map = obj
                    .iter()
                    .map(|(k, v)| Self::from_json(v).map(|gv| (k.clone(), gv)))
                    .collect::<Result<IndexMap<_, _>>>()?;
                Ok(Self::Object(map))
            },
        }
    }
}

impl fmt::Display for GraphQLValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_json())
    }
}


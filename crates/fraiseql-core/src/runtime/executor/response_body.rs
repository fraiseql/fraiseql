//! A GraphQL response as the transport sends it: a `Value`, or the bytes already serialized.

/// A response body, in whichever form the executor produced it.
///
/// A read whose rows need no shaping in Rust is answered as the bytes the database
/// returned, spliced into the `data` envelope, and never becomes a [`serde_json::Value`].
/// Every other read is answered as a `Value`. Both serialize to the same bytes; the
/// form is the executor's choice, not the caller's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponseBody {
    /// A response built as a tree, serialized by the transport.
    Value(serde_json::Value),
    /// A response already serialized, compact, its keys in response order.
    Raw(Vec<u8>),
}

impl ResponseBody {
    /// Whether the response was answered without building a `Value`.
    #[must_use]
    pub const fn is_raw(&self) -> bool {
        matches!(self, Self::Raw(_))
    }

    /// The response as a tree, for a step that edits it.
    ///
    /// # Errors
    ///
    /// [`FraiseQLError::Internal`](crate::error::FraiseQLError::Internal) when a raw body is
    /// not valid JSON, which no read the executor answers raw produces.
    pub fn into_value(self) -> crate::error::Result<serde_json::Value> {
        match self {
            Self::Value(value) => Ok(value),
            Self::Raw(bytes) => {
                serde_json::from_slice(&bytes).map_err(|e| crate::error::FraiseQLError::Internal {
                    message: format!("a raw response body is not valid JSON: {e}"),
                    source:  None,
                })
            },
        }
    }

    /// The response as the bytes a transport sends.
    ///
    /// # Errors
    ///
    /// Returns [`serde_json::Error`] when a `Value` body cannot be serialized.
    pub fn into_bytes(self) -> serde_json::Result<Vec<u8>> {
        match self {
            Self::Value(value) => serde_json::to_vec(&value),
            Self::Raw(bytes) => Ok(bytes),
        }
    }
}

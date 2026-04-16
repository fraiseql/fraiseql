//! Cascade-spec error classification.
//!
//! `MutationErrorClass` mirrors the `app.mutation_error_class` PostgreSQL enum
//! emitted by `app.mutation_response` rows. `CascadeErrorCode` is the wire
//! representation used by the graphql-cascade error envelope. The mapping
//! between them is 1:1 — no fallbacks, no HTTP-code tiebreakers.
//!
//! See `docs/architecture/mutation-response.md` (semantics table + mapping).

use serde::Deserialize;

/// Classification of a failed mutation.
///
/// Mirrors `app.mutation_error_class` in PostgreSQL. Variants serialize to the
/// `snake_case` form used in the PG enum so rows containing `error_class` strings
/// deserialize directly. `NULL` in the PG column corresponds to
/// `Option::None` in the parent struct.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum MutationErrorClass {
    /// Input failed schema or business-rule validation.
    Validation,
    /// Uniqueness, optimistic-concurrency, or state conflict.
    Conflict,
    /// Target entity does not exist or the caller cannot see it.
    NotFound,
    /// Caller is unauthenticated.
    Unauthorized,
    /// Caller is authenticated but lacks permission.
    Forbidden,
    /// Unhandled server-side failure. Implementation details must not leak.
    Internal,
    /// Transaction was rolled back (serialization, deadlock, explicit abort).
    TransactionFailed,
    /// Operation exceeded a deadline.
    Timeout,
    /// Caller exceeded quota.
    RateLimited,
    /// Downstream dependency unreachable.
    ServiceUnavailable,
}

/// graphql-cascade wire-level error code.
///
/// Serialized as `SCREAMING_SNAKE_CASE` on the wire; derived 1:1 from a
/// `MutationErrorClass` via [`MutationErrorClass::to_cascade_code`].
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum CascadeErrorCode {
    /// Input validation failed.
    ValidationError,
    /// Uniqueness or concurrency conflict.
    Conflict,
    /// Entity not found.
    NotFound,
    /// Unauthenticated.
    Unauthorized,
    /// Forbidden.
    Forbidden,
    /// Internal server error.
    InternalError,
    /// Transaction rolled back.
    TransactionFailed,
    /// Operation timed out.
    Timeout,
    /// Rate limit exceeded.
    RateLimited,
    /// Downstream service unavailable.
    ServiceUnavailable,
}

impl MutationErrorClass {
    /// The `snake_case` string that identifies this class on the wire.
    ///
    /// Mirrors the `app.mutation_error_class` PostgreSQL enum label and the
    /// `serde(rename_all = "snake_case")` serialisation form used in v2 rows.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Validation => "validation",
            Self::Conflict => "conflict",
            Self::NotFound => "not_found",
            Self::Unauthorized => "unauthorized",
            Self::Forbidden => "forbidden",
            Self::Internal => "internal",
            Self::TransactionFailed => "transaction_failed",
            Self::Timeout => "timeout",
            Self::RateLimited => "rate_limited",
            Self::ServiceUnavailable => "service_unavailable",
        }
    }

    /// Map the error class to its graphql-cascade wire code (1:1, no fallbacks).
    #[must_use]
    pub const fn to_cascade_code(self) -> CascadeErrorCode {
        match self {
            Self::Validation => CascadeErrorCode::ValidationError,
            Self::Conflict => CascadeErrorCode::Conflict,
            Self::NotFound => CascadeErrorCode::NotFound,
            Self::Unauthorized => CascadeErrorCode::Unauthorized,
            Self::Forbidden => CascadeErrorCode::Forbidden,
            Self::Internal => CascadeErrorCode::InternalError,
            Self::TransactionFailed => CascadeErrorCode::TransactionFailed,
            Self::Timeout => CascadeErrorCode::Timeout,
            Self::RateLimited => CascadeErrorCode::RateLimited,
            Self::ServiceUnavailable => CascadeErrorCode::ServiceUnavailable,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use serde_json::json;

    use super::*;

    #[test]
    fn to_cascade_code_is_one_to_one() {
        let pairs = [
            (MutationErrorClass::Validation, CascadeErrorCode::ValidationError),
            (MutationErrorClass::Conflict, CascadeErrorCode::Conflict),
            (MutationErrorClass::NotFound, CascadeErrorCode::NotFound),
            (MutationErrorClass::Unauthorized, CascadeErrorCode::Unauthorized),
            (MutationErrorClass::Forbidden, CascadeErrorCode::Forbidden),
            (MutationErrorClass::Internal, CascadeErrorCode::InternalError),
            (MutationErrorClass::TransactionFailed, CascadeErrorCode::TransactionFailed),
            (MutationErrorClass::Timeout, CascadeErrorCode::Timeout),
            (MutationErrorClass::RateLimited, CascadeErrorCode::RateLimited),
            (MutationErrorClass::ServiceUnavailable, CascadeErrorCode::ServiceUnavailable),
        ];
        for (class, expected) in pairs {
            assert_eq!(class.to_cascade_code(), expected, "class = {class:?}");
        }
    }

    #[test]
    fn deserializes_from_pg_enum_snake_case() {
        let pairs = [
            ("validation", MutationErrorClass::Validation),
            ("conflict", MutationErrorClass::Conflict),
            ("not_found", MutationErrorClass::NotFound),
            ("unauthorized", MutationErrorClass::Unauthorized),
            ("forbidden", MutationErrorClass::Forbidden),
            ("internal", MutationErrorClass::Internal),
            ("transaction_failed", MutationErrorClass::TransactionFailed),
            ("timeout", MutationErrorClass::Timeout),
            ("rate_limited", MutationErrorClass::RateLimited),
            ("service_unavailable", MutationErrorClass::ServiceUnavailable),
        ];
        for (raw, expected) in pairs {
            let got: MutationErrorClass = serde_json::from_value(json!(raw)).unwrap();
            assert_eq!(got, expected, "raw = {raw}");
        }
    }

    #[test]
    fn cascade_code_deserializes_from_screaming_snake_case() {
        let pairs = [
            (CascadeErrorCode::ValidationError, "VALIDATION_ERROR"),
            (CascadeErrorCode::Conflict, "CONFLICT"),
            (CascadeErrorCode::NotFound, "NOT_FOUND"),
            (CascadeErrorCode::Unauthorized, "UNAUTHORIZED"),
            (CascadeErrorCode::Forbidden, "FORBIDDEN"),
            (CascadeErrorCode::InternalError, "INTERNAL_ERROR"),
            (CascadeErrorCode::TransactionFailed, "TRANSACTION_FAILED"),
            (CascadeErrorCode::Timeout, "TIMEOUT"),
            (CascadeErrorCode::RateLimited, "RATE_LIMITED"),
            (CascadeErrorCode::ServiceUnavailable, "SERVICE_UNAVAILABLE"),
        ];
        for (code, raw) in pairs {
            let got: CascadeErrorCode = serde_json::from_value(json!(raw)).unwrap();
            assert_eq!(got, code, "raw = {raw}");
        }
    }
}

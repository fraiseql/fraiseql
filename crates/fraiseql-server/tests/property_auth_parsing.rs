//! Property-based tests for bearer token extraction from Authorization headers.
//!
//! Verifies that `extract_bearer_token` is panic-free on arbitrary inputs and
//! correctly classifies Bearer vs non-Bearer headers.

use fraiseql_server::middleware::auth::extract_bearer_token;
use proptest::prelude::*;

proptest! {
    /// Must never panic regardless of header content.
    #[test]
    fn auth_header_extract_never_panics(header in "\\PC*") {
        let _ = extract_bearer_token(&header);  // intentional
    }

    /// Non-Bearer prefixes must always return None.
    #[test]
    fn non_bearer_prefix_returns_none(prefix in "(Basic |Digest |NTLM |TOKEN |bearer )") {
        // Note: lowercase "bearer" is not the RFC 6750 prefix
        let result = extract_bearer_token(&prefix);
        prop_assert!(result.is_none(), "non-Bearer prefix {prefix:?} must return None");
    }

    /// Empty string must return None (no "Bearer " prefix).
    #[test]
    fn empty_header_returns_none(s in "") {
        prop_assert!(extract_bearer_token(&s).is_none());
    }

    /// A header starting with exactly "Bearer " must return the remainder.
    #[test]
    fn bearer_prefix_extracts_token(token in "[A-Za-z0-9._-]{1,80}") {
        let header = format!("Bearer {token}");
        let extracted = extract_bearer_token(&header);
        prop_assert!(
            extracted == Some(token.as_str()),
            "expected Some({token:?}), got {extracted:?}"
        );
    }

    /// Extraction is deterministic — same input yields same output.
    #[test]
    fn extraction_is_deterministic(header in "\\PC{0,100}") {
        let r1 = extract_bearer_token(&header);
        let r2 = extract_bearer_token(&header);
        prop_assert_eq!(r1, r2);
    }

    /// The extracted token must be a suffix of the original header.
    #[test]
    fn extracted_token_is_suffix_of_header(header in "Bearer [A-Za-z0-9]{5,40}") {
        if let Some(token) = extract_bearer_token(&header) {
            prop_assert!(
                header.ends_with(token),
                "token {token:?} must be a suffix of header {header:?}"
            );
        }
    }
}

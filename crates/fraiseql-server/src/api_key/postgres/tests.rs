//! Tests for `api_key/postgres.rs`.

/// Timestamps are written as clients read them today: the fraction padded to `.120` /
/// `.500`, none at a whole second, and a `Z` suffix.
mod stored_time_format {
    use fraiseql_time::Timestamp;

    use crate::api_key::postgres::ApiKeyRecord;

    fn at(text: &str) -> Timestamp {
        Timestamp::parse_rfc3339_strict(text).expect("a valid instant")
    }

    #[test]
    fn an_api_key_record_writes_its_times_in_the_stored_form() {
        let record = ApiKeyRecord {
            selector:     "s".to_owned(),
            name:         "n".to_owned(),
            scopes:       vec![],
            created_at:   at("2025-11-20T12:00:00.120Z"),
            expires_at:   Some(at("2025-11-20T12:00:01.5Z")),
            revoked_at:   Some(at("2025-11-20T12:00:02Z")),
            last_used_at: None,
        };
        assert_eq!(
            serde_json::to_value(&record).expect("it writes"),
            serde_json::json!({
                "selector": "s",
                "name": "n",
                "scopes": [],
                "created_at": "2025-11-20T12:00:00.120Z",
                "expires_at": "2025-11-20T12:00:01.500Z",
                "revoked_at": "2025-11-20T12:00:02Z",
                "last_used_at": null
            })
        );
    }
}

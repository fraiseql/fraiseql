#![allow(clippy::unwrap_used)] // Reason: test code

use fraiseql_core::schema::{CompiledSchema, LocaleConfig, LocaleSource};

use super::session_variables_supported_check;

fn with_locale() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    schema.locale = Some(
        LocaleConfig::new(
            "en-US",
            vec!["en-US".to_string()],
            std::collections::BTreeMap::new(),
            vec![LocaleSource::Header {
                header: "accept-language".to_string(),
            }],
        )
        .unwrap(),
    );
    schema
}

/// #1115: the wire backend cannot apply session variables, so a schema with `[locale]` is
/// refused when the server is built over it; the same schema boots on an adapter that applies
/// them, and a schema without them boots on the wire backend.
#[test]
fn a_schema_needing_session_variables_is_refused_on_the_wire_backend() {
    let wire = fraiseql_core::db::FraiseWireAdapter::new("postgres://localhost/unused");
    let err = session_variables_supported_check(&with_locale(), &wire).unwrap_err();
    assert!(err.to_string().contains("[locale]"), "{err}");
    session_variables_supported_check(&CompiledSchema::new(), &wire)
        .expect("nothing to apply: the wire backend boots");

    let double = fraiseql_test_utils::failing_adapter::FailingAdapter::new();
    session_variables_supported_check(&with_locale(), &double)
        .expect("an adapter that applies them boots");
}

/// The same refusal, through the constructor a wire-backend deployment boots with.
#[tokio::test]
async fn the_wire_backend_server_refuses_to_boot_with_locale() {
    let wire = std::sync::Arc::new(fraiseql_core::db::FraiseWireAdapter::new(
        "postgres://localhost/unused",
    ));
    let config = crate::server_config::ServerConfig {
        database_url: "postgres://localhost/unused".to_string(),
        ..crate::server_config::ServerConfig::default()
    };
    let err = Box::pin(crate::Server::new_read_only(config, with_locale(), wire, None))
        .await
        .err()
        .expect("a wire-backend server with [locale] must not boot");
    assert!(err.to_string().contains("cannot apply session variables"), "{err}");
}

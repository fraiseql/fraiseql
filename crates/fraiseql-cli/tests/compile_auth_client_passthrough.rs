#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
//! End-to-end coverage for the `[auth]` PKCE OAuth-client group surviving
//! `fraiseql compile` (#621).
//!
//! Before this landed, `OidcServerClient::from_compiled_schema` read
//! `schema_json["auth"]`, but `CompiledSchema` had no `auth` field and the merger
//! never emitted one — so the consumer structurally always returned `None` and a
//! complete `[auth]` PKCE group was rejected at compile time as "not yet
//! functional". These tests prove the wire is now complete: the `[auth]` PKCE
//! group carries through the merger and converter into `CompiledSchema.auth`.

use std::io::Write;

use fraiseql_cli::schema::{converter::SchemaConverter, merger::SchemaMerger};
use tempfile::NamedTempFile;

#[test]
fn toml_auth_pkce_group_carries_through_merger_and_converter() {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(
        br#"
        [schema]
        name = "app"

        [types.User]
        sql_source = "v_user"

        [auth]
        discovery_url       = "https://accounts.google.com"
        client_id           = "my-client-id"
        client_secret_env   = "OIDC_CLIENT_SECRET"
        server_redirect_uri = "https://api.example.com/auth/callback"
    "#,
    )
    .unwrap();
    f.flush().unwrap();

    let intermediate = SchemaMerger::merge_toml_only(f.path().to_str().unwrap()).unwrap();
    assert!(
        intermediate.auth.is_some(),
        "the merger must carry the `[auth]` PKCE group into IntermediateSchema.auth"
    );

    let compiled = SchemaConverter::convert(intermediate).expect("convert to compiled schema");
    let auth = compiled.auth.as_ref().expect("compiled schema must carry `auth` (#621)");
    assert_eq!(auth.discovery_url, "https://accounts.google.com");
    assert_eq!(auth.client_id, "my-client-id");
    assert_eq!(auth.client_secret_env, "OIDC_CLIENT_SECRET");
    assert_eq!(auth.server_redirect_uri, "https://api.example.com/auth/callback");
}

#[test]
fn the_client_secret_is_never_embedded_in_the_compiled_schema() {
    // The compiled `auth` object names the env var, never the secret value.
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(
        br#"
        [schema]
        name = "app"

        [types.User]
        sql_source = "v_user"

        [auth]
        discovery_url       = "https://accounts.google.com"
        client_id           = "my-client-id"
        client_secret_env   = "OIDC_CLIENT_SECRET"
        server_redirect_uri = "https://api.example.com/auth/callback"
    "#,
    )
    .unwrap();
    f.flush().unwrap();

    let intermediate = SchemaMerger::merge_toml_only(f.path().to_str().unwrap()).unwrap();
    let compiled = SchemaConverter::convert(intermediate).unwrap();
    let serialized = serde_json::to_string(&compiled).unwrap();

    assert!(
        serialized.contains("OIDC_CLIENT_SECRET"),
        "the env-var NAME is carried so the runtime can read the secret at boot"
    );
    // There is no secret value in TOML, so there is nothing to leak — this pins the
    // invariant that only the env-var name (not a value) is ever compiled.
    assert!(
        !serialized.contains("client_secret\":"),
        "the compiled schema must never carry a `client_secret` value, only the env name"
    );
}

#[test]
fn a_jwt_only_auth_block_produces_no_compiled_auth_client() {
    // The JWT-validation group (issuer/audience) is consumed by the server's
    // OidcConfig, not the PKCE client — so it must NOT populate `CompiledSchema.auth`.
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(
        br#"
        [schema]
        name = "app"

        [types.User]
        sql_source = "v_user"

        [auth]
        issuer   = "https://accounts.example.com"
        audience = "my-api"
    "#,
    )
    .unwrap();
    f.flush().unwrap();

    let intermediate = SchemaMerger::merge_toml_only(f.path().to_str().unwrap()).unwrap();
    let compiled = SchemaConverter::convert(intermediate).unwrap();
    assert!(
        compiled.auth.is_none(),
        "a JWT-only [auth] block must not populate the PKCE client config"
    );
}

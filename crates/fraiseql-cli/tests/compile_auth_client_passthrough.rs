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
    let pkce = auth.pkce.as_ref().expect("compiled auth must carry the PKCE group");
    assert_eq!(pkce.discovery_url, "https://accounts.google.com");
    assert_eq!(pkce.client_id, "my-client-id");
    assert_eq!(pkce.client_secret_env, "OIDC_CLIENT_SECRET");
    assert_eq!(pkce.server_redirect_uri, "https://api.example.com/auth/callback");
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
fn toml_auth_social_group_carries_through_to_compiled_schema() {
    // #368: [auth.social.google] / [auth.social.github] must survive the
    // merger and converter into `CompiledSchema.auth.social`, with the client
    // secrets carried as env-var NAMES only.
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(
        br#"
        [schema]
        name = "app"

        [types.User]
        sql_source = "v_user"

        [auth.social]
        redirect_uri_allowlist = ["https://app.example.com/cb"]

        [auth.social.google]
        client_id         = "google-client-id"
        client_secret_env = "GOOGLE_CLIENT_SECRET"
        redirect_uri      = "https://api.example.com/auth/v1/callback"

        [auth.social.github]
        client_id         = "github-client-id"
        client_secret_env = "GITHUB_CLIENT_SECRET"
        redirect_uri      = "https://api.example.com/auth/v1/callback"
    "#,
    )
    .unwrap();
    f.flush().unwrap();

    let intermediate = SchemaMerger::merge_toml_only(f.path().to_str().unwrap()).unwrap();
    let compiled = SchemaConverter::convert(intermediate).expect("convert to compiled schema");
    let auth = compiled.auth.as_ref().expect("compiled schema must carry `auth` (#368)");
    assert!(
        auth.pkce.is_none(),
        "no PKCE group was configured — social alone must not mint one"
    );
    let social = auth.social.as_ref().expect("compiled auth must carry the social group");
    assert_eq!(social.redirect_uri_allowlist, vec!["https://app.example.com/cb".to_string()]);
    let google = social.google.as_ref().expect("google provider must be compiled");
    assert_eq!(google.client_id, "google-client-id");
    assert_eq!(google.client_secret_env, "GOOGLE_CLIENT_SECRET");
    assert_eq!(google.redirect_uri, "https://api.example.com/auth/v1/callback");
    assert!(google.discovery_url.is_none(), "no override configured");
    let github = social.github.as_ref().expect("github provider must be compiled");
    assert_eq!(github.client_id, "github-client-id");
    assert_eq!(github.client_secret_env, "GITHUB_CLIENT_SECRET");
    assert!(github.base_url.is_none() && github.api_base_url.is_none());
}

#[test]
fn an_unimplemented_social_provider_is_refused_at_compile_time() {
    // #368: a provider key with no implementation must refuse to compile rather
    // than be silently accepted and never served. (`apple` used to be the case
    // in point; #943 implemented it, so the guarantee is pinned on a key that
    // is not a provider at all.)
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(
        br#"
        [schema]
        name = "app"

        [types.User]
        sql_source = "v_user"

        [auth.social.myspace]
        client_id         = "myspace-client-id"
        client_secret_env = "MYSPACE_CLIENT_SECRET"
        redirect_uri      = "https://api.example.com/auth/v1/callback"
    "#,
    )
    .unwrap();
    f.flush().unwrap();

    let err = SchemaMerger::merge_toml_only(f.path().to_str().unwrap())
        .err()
        .map(|e| format!("{e:#}"))
        .expect("[auth.social.myspace] must be refused, not silently dropped");
    assert!(
        err.contains("myspace"),
        "the refusal must name the unsupported provider key: {err}"
    );
}

/// #943: `[auth.social.apple]` reaches the compiled schema with the fields
/// Apple actually needs — no `client_secret_env`, because Apple's client secret
/// is an assertion the runtime signs rather than a string it reads.
#[test]
fn the_apple_provider_compiles_with_its_assertion_key_material() {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(
        br#"
        [schema]
        name = "app"

        [types.User]
        sql_source = "v_user"

        [auth.social.apple]
        client_id       = "com.example.service"
        team_id         = "TEAM123456"
        key_id          = "KEY7890AB"
        private_key_env = "APPLE_SIGNIN_P8"
        redirect_uri    = "https://api.example.com/auth/v1/callback"
    "#,
    )
    .unwrap();
    f.flush().unwrap();

    let intermediate = SchemaMerger::merge_toml_only(f.path().to_str().unwrap()).unwrap();
    let compiled = SchemaConverter::convert(intermediate).expect("convert to compiled schema");
    let apple = compiled
        .auth
        .as_ref()
        .and_then(|a| a.social.as_ref())
        .and_then(|s| s.apple.as_ref())
        .expect("apple provider must be compiled");
    assert_eq!(apple.client_id, "com.example.service");
    assert_eq!(apple.team_id, "TEAM123456");
    assert_eq!(apple.key_id, "KEY7890AB");
    assert_eq!(apple.private_key_env.as_deref(), Some("APPLE_SIGNIN_P8"));
    assert!(apple.private_key_path.is_none());
    assert!(apple.base_url.is_none(), "no override configured");
}

/// #943: the `.p8` key has exactly one source. Naming both is ambiguous and
/// naming neither is unusable — each is refused where the operator is editing,
/// not at server boot.
#[test]
fn apple_needs_exactly_one_private_key_source() {
    let cases = [
        (
            "private_key_env = \"A\"\n        private_key_path = \"/tmp/k.p8\"",
            "names both",
        ),
        ("", "needs exactly one"),
    ];
    for (key_lines, expected) in cases {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(
            format!(
                r#"
        [schema]
        name = "app"

        [types.User]
        sql_source = "v_user"

        [auth.social.apple]
        client_id    = "com.example.service"
        team_id      = "TEAM123456"
        key_id       = "KEY7890AB"
        redirect_uri = "https://api.example.com/auth/v1/callback"
        {key_lines}
    "#
            )
            .as_bytes(),
        )
        .unwrap();
        f.flush().unwrap();

        let err = SchemaMerger::merge_toml_only(f.path().to_str().unwrap())
            .err()
            .map(|e| format!("{e:#}"))
            .expect("an ambiguous or absent key source must be refused");
        assert!(err.contains(expected), "expected {expected:?} in: {err}");
    }
}

#[test]
fn an_empty_social_block_is_refused_at_compile_time() {
    // [auth.social] with no providers is a typo, not a deployment.
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(
        br#"
        [schema]
        name = "app"

        [types.User]
        sql_source = "v_user"

        [auth.social]
        redirect_uri_allowlist = ["https://app.example.com/cb"]
    "#,
    )
    .unwrap();
    f.flush().unwrap();

    let err = SchemaMerger::merge_toml_only(f.path().to_str().unwrap())
        .err()
        .map(|e| format!("{e:#}"))
        .expect("[auth.social] without providers must be refused");
    assert!(
        err.contains("provider"),
        "the refusal must explain that no provider is configured: {err}"
    );
}

#[test]
fn toml_auth_local_group_carries_through_to_compiled_schema() {
    // #367: [auth.local] must survive into `CompiledSchema.auth.local` — the
    // server reads it there to decide which first-party auth routes to mount.
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(
        br#"
        [schema]
        name = "app"

        [types.User]
        sql_source = "v_user"

        [auth.local]
        password           = true
        otp                = true
        mfa                = true
        anonymous          = false
        mfa_issuer         = "Acme"
        email_from         = "support"
        reset_url_template = "https://app.example.com/reset?token={token}"
    "#,
    )
    .unwrap();
    f.flush().unwrap();

    let intermediate = SchemaMerger::merge_toml_only(f.path().to_str().unwrap()).unwrap();
    let compiled = SchemaConverter::convert(intermediate).expect("convert to compiled schema");
    let local = compiled
        .auth
        .as_ref()
        .and_then(|a| a.local.as_ref())
        .expect("compiled auth must carry the local group (#367)");
    assert!(local.password && local.otp && local.mfa);
    assert!(!local.anonymous, "an unset method stays off");
    assert_eq!(local.mfa_issuer.as_deref(), Some("Acme"));
    assert_eq!(local.email_from.as_deref(), Some("support"));
    assert_eq!(
        local.reset_url_template.as_deref(),
        Some("https://app.example.com/reset?token={token}")
    );
}

#[test]
fn local_auth_refuses_mail_methods_with_no_email_from() {
    // A magic-link login with no mail account configured is a login nobody can
    // complete. Refuse at compile time, where the operator is editing.
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(
        br#"
        [schema]
        name = "app"

        [types.User]
        sql_source = "v_user"

        [auth.local]
        otp = true
    "#,
    )
    .unwrap();
    f.flush().unwrap();

    let err = SchemaMerger::merge_toml_only(f.path().to_str().unwrap())
        .err()
        .map(|e| format!("{e:#}"))
        .expect("[auth.local] otp with no email_from must be refused");
    assert!(err.contains("email_from"), "the refusal must name the missing key: {err}");
}

#[test]
fn local_auth_refuses_password_with_no_reset_template() {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(
        br#"
        [schema]
        name = "app"

        [types.User]
        sql_source = "v_user"

        [auth.local]
        password   = true
        email_from = "support"
    "#,
    )
    .unwrap();
    f.flush().unwrap();

    let err = SchemaMerger::merge_toml_only(f.path().to_str().unwrap())
        .err()
        .map(|e| format!("{e:#}"))
        .expect("password auth with no reset_url_template must be refused");
    assert!(
        err.contains("reset_url_template"),
        "the refusal must name the missing key: {err}"
    );
}

#[test]
fn local_auth_refuses_a_template_without_its_placeholder() {
    // A template with no {token} builds the same dead link for every user.
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(
        br#"
        [schema]
        name = "app"

        [types.User]
        sql_source = "v_user"

        [auth.local]
        password           = true
        email_from         = "support"
        reset_url_template = "https://app.example.com/reset"
    "#,
    )
    .unwrap();
    f.flush().unwrap();

    let err = SchemaMerger::merge_toml_only(f.path().to_str().unwrap())
        .err()
        .map(|e| format!("{e:#}"))
        .expect("a template without {token} must be refused");
    assert!(err.contains("{token}"), "the refusal must name the placeholder: {err}");
}

#[test]
fn local_auth_refuses_a_block_that_enables_nothing() {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(
        br#"
        [schema]
        name = "app"

        [types.User]
        sql_source = "v_user"

        [auth.local]
        mfa_issuer = "Acme"
    "#,
    )
    .unwrap();
    f.flush().unwrap();

    let err = SchemaMerger::merge_toml_only(f.path().to_str().unwrap())
        .err()
        .map(|e| format!("{e:#}"))
        .expect("[auth.local] enabling no method must be refused");
    assert!(err.contains("no method"), "the refusal must say what is wrong: {err}");
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

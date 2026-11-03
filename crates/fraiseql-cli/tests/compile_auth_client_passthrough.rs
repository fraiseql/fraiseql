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

/// #944: Discord and Facebook reach the compiled schema, Facebook carrying the
/// Graph API version that Meta's deprecation schedule makes configuration
/// rather than a constant.
#[test]
fn the_discord_and_facebook_providers_compile_with_their_overrides() {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(
        br#"
        [schema]
        name = "app"

        [types.User]
        sql_source = "v_user"

        [auth.social.discord]
        client_id         = "discord-client-id"
        client_secret_env = "DISCORD_CLIENT_SECRET"
        redirect_uri      = "https://api.example.com/auth/v1/callback"

        [auth.social.facebook]
        client_id         = "facebook-client-id"
        client_secret_env = "FACEBOOK_CLIENT_SECRET"
        redirect_uri      = "https://api.example.com/auth/v1/callback"
        api_version       = "v23.0"
    "#,
    )
    .unwrap();
    f.flush().unwrap();

    let intermediate = SchemaMerger::merge_toml_only(f.path().to_str().unwrap()).unwrap();
    let compiled = SchemaConverter::convert(intermediate).expect("convert to compiled schema");
    let social = compiled
        .auth
        .as_ref()
        .and_then(|a| a.social.as_ref())
        .expect("compiled auth must carry the social group");

    let discord = social.discord.as_ref().expect("discord provider must be compiled");
    assert_eq!(discord.client_id, "discord-client-id");
    assert_eq!(discord.client_secret_env, "DISCORD_CLIENT_SECRET");
    assert!(discord.base_url.is_none(), "no override configured");

    let facebook = social.facebook.as_ref().expect("facebook provider must be compiled");
    assert_eq!(facebook.client_id, "facebook-client-id");
    assert_eq!(facebook.client_secret_env, "FACEBOOK_CLIENT_SECRET");
    assert_eq!(
        facebook.api_version.as_deref(),
        Some("v23.0"),
        "the Graph version must survive the seam — Meta deprecates versions on its own schedule"
    );
    assert!(facebook.base_url.is_none() && facebook.graph_base_url.is_none());
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

/// Compile `[auth.local]` with `extra` added to a valid `otp` block.
fn merge_otp_with(extra: &str) -> anyhow::Result<fraiseql_cli::schema::IntermediateSchema> {
    let mut f = NamedTempFile::new().unwrap();
    write!(
        f,
        r#"
        [schema]
        name = "app"

        [types.User]
        sql_source = "v_user"

        [auth.local]
        email_from = "support"
        {extra}
    "#
    )
    .unwrap();
    f.flush().unwrap();
    SchemaMerger::merge_toml_only(f.path().to_str().unwrap())
}

#[test]
fn otp_allowed_query_reaches_the_compiled_schema() {
    let intermediate = merge_otp_with(
        r#"otp = true
        otp_allowed_query = "SELECT 1 FROM tb_member WHERE email = $email AND active""#,
    )
    .unwrap();
    let compiled = SchemaConverter::convert(intermediate).expect("convert to compiled schema");
    let local = compiled.auth.as_ref().and_then(|a| a.local.as_ref()).expect("auth.local");
    assert_eq!(
        local.otp_allowed_query.as_deref(),
        Some("SELECT 1 FROM tb_member WHERE email = $email AND active")
    );
}

#[test]
fn otp_allowed_query_is_refused_without_otp() {
    let err = merge_otp_with(
        r#"mfa = true
        otp_allowed_query = "SELECT 1 FROM tb_member WHERE email = $email""#,
    )
    .err()
    .map(|e| format!("{e:#}"))
    .expect("the key without otp must be refused");
    assert!(err.contains("otp_allowed_query"), "the refusal must name the key: {err}");
}

#[test]
fn otp_allowed_query_that_could_not_work_is_refused() {
    let err = merge_otp_with(
        r#"otp = true
        otp_allowed_query = "SELECT 1 FROM tb_member""#,
    )
    .err()
    .map(|e| format!("{e:#}"))
    .expect("a query that never reads the address must be refused");
    assert!(err.contains("$email"), "the refusal must name the parameter: {err}");
}

/// What compiling `[auth.local]` with `extra` logs at `WARN` or above.
fn warnings_merging_otp_with(extra: &str, section: &str) -> String {
    #[derive(Clone, Default)]
    struct Captured(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
    impl Write for Captured {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_writer(move || writer.clone())
        .finish();
    let toml = format!(
        r#"otp = true
        otp_allowed_query = "SELECT 1 FROM tb_member WHERE email = $email"
        {extra}

        {section}"#
    );
    fraiseql_test_support::tracing_capture::with_default(subscriber, || {
        merge_otp_with(&toml).expect("a valid [auth.local] block");
    });
    let logged = captured.0.lock().unwrap().clone();
    String::from_utf8(logged).unwrap()
}

/// The list governs OTP alone, so a deployment that believes it is invite-only must hear that
/// another open sign-in method sits beside it.
#[test]
fn otp_allowed_query_warns_when_password_signup_stays_open() {
    let logged = warnings_merging_otp_with(
        r#"password = true
        reset_url_template = "https://app.example.com/reset?token={token}""#,
        "",
    );
    assert!(logged.contains("otp_allowed_query governs OTP only"), "logged: {logged}");
}

#[test]
fn otp_allowed_query_warns_when_social_sign_in_stays_open() {
    let logged = warnings_merging_otp_with(
        "",
        r#"[auth.social.google]
        client_id         = "id"
        client_secret_env = "GOOGLE_SECRET"
        redirect_uri      = "https://app.example.com/callback""#,
    );
    assert!(logged.contains("otp_allowed_query governs OTP only"), "logged: {logged}");
}

#[test]
fn otp_allowed_query_alone_does_not_warn() {
    let logged = warnings_merging_otp_with("", "");
    assert!(!logged.contains("otp_allowed_query"), "logged: {logged}");
}

// ── #1853: [auth.session] ────────────────────────────────────────────────────

/// Compile an `OTP` `[auth.local]` block followed by `[auth.session]` holding `session`.
fn merge_session(session: &str) -> anyhow::Result<fraiseql_cli::schema::IntermediateSchema> {
    let mut f = NamedTempFile::new().unwrap();
    write!(
        f,
        r#"
        [schema]
        name = "app"

        [types.User]
        sql_source = "v_user"

        [auth.local]
        otp = true
        email_from = "support"

        [auth.session]
        {session}
    "#
    )
    .unwrap();
    f.flush().unwrap();
    SchemaMerger::merge_toml_only(f.path().to_str().unwrap())
}

fn session_refusal(session: &str) -> String {
    merge_session(session)
        .err()
        .map(|e| format!("{e:#}"))
        .unwrap_or_else(|| panic!("[auth.session] {session:?} must be refused"))
}

/// A refusal of a value, made by the section's own check rather than by the section
/// being unknown.
fn session_value_refusal(session: &str) -> String {
    let err = session_refusal(session);
    assert!(
        !err.contains("unknown field"),
        "{session:?} must be refused for its value, not as an unknown section: {err}"
    );
    err
}

#[test]
fn auth_session_reaches_the_compiled_schema() {
    let intermediate = merge_session("ttl_secs = 900\nabsolute_ttl_secs = 86400").unwrap();
    let compiled = SchemaConverter::convert(intermediate).expect("convert to compiled schema");
    let session = compiled.auth.as_ref().and_then(|a| a.session.as_ref()).expect("auth.session");
    assert_eq!(session.ttl_secs, Some(900));
    assert_eq!(session.absolute_ttl_secs, Some(86_400));
}

#[test]
fn auth_session_refuses_an_unknown_key() {
    let err = session_refusal("ttl_secs = 900\nrefresh = true");
    assert!(err.contains("refresh"), "the refusal must name the key: {err}");
}

#[test]
fn auth_session_refuses_a_limit_not_above_the_ttl() {
    for session in [
        "ttl_secs = 900\nabsolute_ttl_secs = 900",
        "ttl_secs = 900\nabsolute_ttl_secs = 600",
        // Unset, the access token lasts 3600 s; a limit below that ends a session before
        // its first token does.
        "absolute_ttl_secs = 1800",
    ] {
        let err = session_value_refusal(session);
        assert!(err.contains("absolute_ttl_secs"), "{session:?}: {err}");
    }
}

#[test]
fn auth_session_refuses_a_ttl_under_a_minute() {
    let err = session_value_refusal("ttl_secs = 59");
    assert!(err.contains("ttl_secs"), "{err}");
}

#[test]
fn auth_session_refuses_a_ttl_of_zero_and_a_limit_of_zero() {
    for session in ["ttl_secs = 0", "ttl_secs = 900\nabsolute_ttl_secs = 0"] {
        session_value_refusal(session);
    }
}

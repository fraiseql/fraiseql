//! A SAML sign-in through the shipped server: `GET /auth/saml/login`, a signed
//! `SAMLResponse` posted to `POST /auth/saml/acs`, and the session it mints.
//!
//! The responses are signed the way a production `IdP` signs them: RSA-SHA256 over a
//! SHA-256 digest. samael's own template asks for a SHA-1 digest, which the server's
//! algorithm allow-list refuses, so the fixture sets the digest on that template before
//! signing; nothing in the server is relaxed for the test. One sign-in runs with the
//! engine on a role that cannot read schema `auth`, and auth on a role of its own.
//!
//! Self-skips when no `DATABASE_URL` is set. Runs in the Dagger `saml`
//! integration suite (which has the libxml2/xmlsec1 stack and Postgres).
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL`, optionally
//! `AUTH_DATABASE_URL` (the auth role's credentials) ·
//! **Parallelism:** creates and drops its own `fraiseql_saml_acs_*` databases →
//! run `--test-threads=1`.
#![cfg(feature = "auth-saml")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::{io::Read as _, sync::Arc};

use base64::Engine as _;
use chrono::{Duration, Utc};
use fraiseql_core::{
    db::postgres::PostgresAdapter,
    schema::{AuthClientConfig, CompiledSchema, SessionConfig},
};
use fraiseql_server::{
    Server,
    server_config::{
        ServerConfig,
        hs256::Hs256Config,
        saml::{SamlIdpEntry, SamlServerConfig},
    },
};
use fraiseql_test_support::try_database_url;
use samael::{
    crypto::{CertificateDer, Crypto, CryptoProvider as _},
    idp::{
        CertificateParams, IdentityProvider, KeyType, Rsa,
        response_builder::{ResponseAttribute, build_response_template},
        sp_extractor::RequiredAttribute,
    },
    signature::DigestAlgorithm,
    traits::ToXml as _,
};
use sqlx::PgPool;

/// The variable `[auth.database]` names in this suite (#1965). Auth runs on the scratch
/// database's own URL here; `auth_database_e2e_pg` covers auth on a role of its own.
const AUTH_URL_ENV: &str = "FRAISEQL_TEST_SAML_ACS_AUTH_URL";

fn auth_database() -> fraiseql_core::schema::AuthDatabaseConfig {
    fraiseql_core::schema::AuthDatabaseConfig::new(AUTH_URL_ENV)
}

const IDP_ENTITY: &str = "https://idp.example.com";
const SP_ENTITY: &str = "https://sp.example.com/metadata";
const SP_ACS: &str = "https://sp.example.com/auth/saml/acs";
const ISSUER: &str = "https://sp.example.com";
const AUDIENCE: &str = "fraiseql";
const TENANT: &str = "33333333-3333-4333-8333-333333333333";
/// 32 bytes, base64-safe — the HS256 secret handed over via env.
const HS256_SECRET: &str = "saml-acs-hs256-secret-32-bytes!!";
const SECRET_ENV: &str = "FRAISEQL_TEST_SAML_ACS_HS256_SECRET";

fn with_database(url: &str, db: &str) -> String {
    let (base, _old) = url.rsplit_once('/').expect("database URL has a path component");
    format!("{base}/{db}")
}

async fn scratch_pool(admin_url: &str, db: &str) -> PgPool {
    let admin = PgPool::connect(admin_url).await.expect("connect to admin database");
    sqlx::raw_sql(&format!("DROP DATABASE IF EXISTS {db} WITH (FORCE)"))
        .execute(&admin)
        .await
        .expect("drop scratch database");
    sqlx::raw_sql(&format!("CREATE DATABASE {db}"))
        .execute(&admin)
        .await
        .expect("create scratch database");
    admin.close().await;
    std::env::set_var(AUTH_URL_ENV, with_database(admin_url, db));
    PgPool::connect(&with_database(admin_url, db))
        .await
        .expect("connect to scratch database")
}

async fn drop_scratch(admin_url: &str, db: &str) {
    let Ok(admin) = PgPool::connect(admin_url).await else {
        return;
    };
    let _ = sqlx::raw_sql(&format!("DROP DATABASE IF EXISTS {db} WITH (FORCE)"))
        .execute(&admin)
        .await;
    admin.close().await;
}

fn database_url_or_skip(test: &str) -> Option<String> {
    let url = try_database_url();
    if url.is_none() {
        eprintln!("SKIP {test}: DATABASE_URL not set");
    }
    url
}

/// An `IdP`: a fresh RSA keypair and the self-signed certificate the server is told to trust.
struct SigningIdp {
    idp:  IdentityProvider,
    cert: CertificateDer,
}

impl SigningIdp {
    fn new() -> Self {
        let idp = IdentityProvider::generate_new(KeyType::Rsa(Rsa::Rsa2048)).unwrap();
        let cert = idp
            .create_certificate(&CertificateParams {
                common_name:           IDP_ENTITY,
                issuer_name:           IDP_ENTITY,
                days_until_expiration: 3650,
            })
            .unwrap();
        Self { idp, cert }
    }

    /// The `IdP`'s metadata, naming its signing certificate.
    fn metadata_xml(&self) -> String {
        let cert_b64 = base64::engine::general_purpose::STANDARD.encode(self.cert.der_data());
        format!(
            r#"<EntityDescriptor xmlns="urn:oasis:names:tc:SAML:2.0:metadata" entityID="{IDP_ENTITY}">
  <IDPSSODescriptor protocolSupportEnumeration="urn:oasis:names:tc:SAML:2.0:protocol">
    <KeyDescriptor use="signing">
      <KeyInfo xmlns="http://www.w3.org/2000/09/xmldsig#">
        <X509Data><X509Certificate>{cert_b64}</X509Certificate></X509Data>
      </KeyInfo>
    </KeyDescriptor>
    <SingleSignOnService Binding="urn:oasis:names:tc:SAML:2.0:bindings:HTTP-Redirect" Location="https://idp.example.com/sso"/>
  </IDPSSODescriptor>
</EntityDescriptor>"#
        )
    }

    /// A base64 `SAMLResponse` answering `request_id` for `name_id`, signed over a
    /// `digest` of the response.
    ///
    /// samael's high-level signer leaves out `SubjectConfirmationData/NotOnOrAfter`, which
    /// its own validator requires, so the template is completed and signed here.
    fn response(&self, request_id: &str, name_id: &str, digest: &DigestAlgorithm) -> String {
        let attributes = [ResponseAttribute {
            required_attribute: RequiredAttribute {
                name:   "email".to_string(),
                format: Some("urn:oasis:names:tc:SAML:2.0:attrname-format:uri".to_string()),
            },
            value:              "ada@example.com",
        }];
        let mut response = build_response_template(
            &self.cert,
            name_id,
            SP_ENTITY,
            IDP_ENTITY,
            SP_ACS,
            request_id,
            &attributes,
        );
        if let Some(data) = response
            .assertion
            .as_mut()
            .and_then(|a| a.subject.as_mut())
            .and_then(|s| s.subject_confirmations.as_mut())
            .and_then(|c| c.first_mut())
            .and_then(|c| c.subject_confirmation_data.as_mut())
        {
            data.not_on_or_after = Some(Utc::now() + Duration::minutes(5));
        }
        let signature = response.signature.as_mut().expect("samael's template signs the response");
        for reference in &mut signature.signed_info.reference {
            reference.digest_method.algorithm = digest.clone();
        }

        let xml = response.to_string().unwrap();
        let signed = Crypto::sign_xml(xml, &self.idp.export_private_key_der().unwrap()).unwrap();
        base64::engine::general_purpose::STANDARD.encode(signed)
    }
}

/// `[saml]` with one `IdP`, `test-idp`, bound to [`TENANT`] and trusting `idp`, and the
/// `[auth_hs256]` the sessions are signed with.
fn saml_config(idp: &SigningIdp, database_url: &str) -> ServerConfig {
    let mut idps = std::collections::HashMap::new();
    idps.insert(
        "test-idp".to_string(),
        SamlIdpEntry {
            sp_entity_id:         SP_ENTITY.to_string(),
            acs_url:              SP_ACS.to_string(),
            metadata_xml_path:    None,
            metadata_xml:         Some(idp.metadata_xml()),
            tenant_id:            Some(TENANT.to_string()),
            trust_asserted_email: false,
        },
    );
    ServerConfig {
        // #874: production validate() refuses cors_enabled=true + empty origins
        cors_enabled: false,
        database_url: database_url.to_string(),
        saml: Some(SamlServerConfig {
            idps,
            store_enabled: false,
            refresh_interval_secs: 30,
            certificate_expiry_warning_days: 30,
            sp: None,
        }),
        auth_hs256: Some(Hs256Config {
            secret_env: SECRET_ENV.to_string(),
            issuer:     Some(ISSUER.to_string()),
            audience:   Some(AUDIENCE.to_string()),
        }),
        ..ServerConfig::default()
    }
}

/// A schema with no types, and `[auth.session]` as given.
fn schema(session: Option<SessionConfig>) -> CompiledSchema {
    let mut schema: CompiledSchema = serde_json::from_value(serde_json::json!({
        "fraiseql_version": fraiseql_core::schema::CURRENT_FRAISEQL_VERSION,
        "types": [],
        "queries": [],
        "mutations": [],
    }))
    .expect("compiled schema");
    schema.auth = Some(AuthClientConfig {
        pkce: None,
        social: None,
        local: None,
        session,
        database: Some(auth_database()),
    });
    schema
}

/// The documents this suite serves load, checked with no database.
///
/// Every other test here builds its document only after it has found a database, so in
/// a run without one they skip first, and a refusal of the document would read as a
/// pass. It is deserialised with `from_value`, which ignores a key it does not know, so
/// what can refuse it is a field it does not send becoming required, or a field
/// changing type.
#[test]
fn the_document_loads_without_a_database() {
    schema(None);
    schema(Some(SessionConfig {
        ttl_secs:          Some(900),
        absolute_ttl_secs: None,
    }));
}

/// The server, booted on a scratch database named `db`.
struct Booted {
    base:     String,
    client:   reqwest::Client,
    shutdown: tokio::sync::oneshot::Sender<()>,
    handle:   tokio::task::JoinHandle<()>,
}

async fn boot(url: &str, db: &str, idp: &SigningIdp, schema: CompiledSchema) -> Booted {
    // Env var must exist before construction loads the secret. A fixed valid
    // value, set unconditionally: safe under the parallel runner because every
    // reader wants exactly this value.
    std::env::set_var(SECRET_ENV, HS256_SECRET);
    let pool = scratch_pool(url, db).await;
    let scratch_url = with_database(url, db);
    let adapter = Arc::new(PostgresAdapter::new(&scratch_url).await.expect("PostgresAdapter::new"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    let server = Box::pin(Server::new(saml_config(idp, &scratch_url), schema, adapter, Some(pool)))
        .await
        .expect("[saml] + [auth_hs256] + a pool must boot");
    let (shutdown, rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
            .expect("serve");
    });
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client");
    Booted {
        base: format!("http://127.0.0.1:{port}"),
        client,
        shutdown,
        handle,
    }
}

impl Booted {
    /// Start a sign-in: the `RelayState` the server issued, and the `ID` of the
    /// `AuthnRequest` it sent the `IdP`, which a response must answer.
    async fn login(&self) -> (String, String) {
        let resp = self
            .client
            .get(format!("{}/auth/saml/login?idp=test-idp&tenant={TENANT}", self.base))
            .send()
            .await
            .expect("login request");
        assert!(resp.status().is_redirection(), "login must redirect: {}", resp.status());
        let redirect = resp.headers()["location"].to_str().unwrap().to_string();
        let redirect = reqwest::Url::parse(&redirect).expect("the redirect is a URL");
        let param = |name: &str| {
            redirect.query_pairs().find(|(k, _)| k == name).map_or_else(
                || panic!("the redirect carries no {name}: {redirect}"),
                |(_, v)| v.into_owned(),
            )
        };
        (param("RelayState"), request_id(&param("SAMLRequest")))
    }

    /// Post `response` to the ACS under `relay_state`.
    async fn acs(
        &self,
        relay_state: &str,
        response: &str,
    ) -> (reqwest::StatusCode, serde_json::Value) {
        let resp = self
            .client
            .post(format!("{}/auth/saml/acs", self.base))
            .form(&[("SAMLResponse", response), ("RelayState", relay_state)])
            .send()
            .await
            .expect("ACS request");
        let status = resp.status();
        (status, resp.json().await.unwrap_or(serde_json::Value::Null))
    }

    async fn stop(self) {
        let _ = self.shutdown.send(());
        let _ = self.handle.await;
    }
}

/// The `ID` of the `AuthnRequest` an HTTP-Redirect `SAMLRequest` carries: raw DEFLATE,
/// then base64.
fn request_id(saml_request: &str) -> String {
    let deflated = base64::engine::general_purpose::STANDARD.decode(saml_request).unwrap();
    let mut xml = String::new();
    flate2::read::DeflateDecoder::new(deflated.as_slice())
        .read_to_string(&mut xml)
        .unwrap();
    let start = xml.find(" ID=\"").expect("the AuthnRequest has an ID") + 5;
    let len = xml[start..].find('"').expect("the ID is quoted");
    xml[start..start + len].to_string()
}

/// A JWT's claims, unvalidated.
fn jwt_claims(token: &str) -> serde_json::Value {
    let payload = token.split('.').nth(1).expect("JWT has a payload");
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .expect("payload decodes");
    serde_json::from_slice(&bytes).expect("claims parse")
}

/// Sign in through the ACS, and return the access token's claims.
async fn sign_in(server: &Booted, idp: &SigningIdp) -> serde_json::Value {
    let (relay_state, request_id) = server.login().await;
    let response = idp.response(&request_id, "ada", &DigestAlgorithm::Sha256);
    let (status, body) = server.acs(&relay_state, &response).await;
    assert_eq!(status, reqwest::StatusCode::OK, "the ACS refused a valid response: {body}");
    jwt_claims(body["access_token"].as_str().expect("an access token"))
}

fn lifetime(claims: &serde_json::Value) -> u64 {
    claims["exp"].as_u64().expect("exp") - claims["iat"].as_u64().expect("iat")
}

#[tokio::test]
async fn ttl_secs_sets_the_lifetime_of_a_saml_sign_in() {
    let Some(url) = database_url_or_skip("ttl_secs_sets_the_lifetime_of_a_saml_sign_in") else {
        return;
    };
    let db = "fraiseql_saml_acs_ttl";
    let idp = SigningIdp::new();
    let session = SessionConfig {
        ttl_secs:          Some(900),
        absolute_ttl_secs: None,
    };
    let server = boot(&url, db, &idp, schema(Some(session))).await;

    let claims = sign_in(&server, &idp).await;

    assert_eq!(lifetime(&claims), 900, "{claims}");
    assert_eq!(claims["iss"], ISSUER, "{claims}");
    assert!(
        claims["aud"].as_array().is_some_and(|a| a.iter().any(|v| v == AUDIENCE)),
        "{claims}"
    );
    let tenant = uuid::Uuid::parse_str(TENANT).unwrap().as_simple().to_string();
    assert_eq!(claims["tenant_id"], tenant.as_str(), "{claims}");
    server.stop().await;
    drop_scratch(&url, db).await;
}

#[tokio::test]
async fn an_unset_session_signs_a_saml_user_in_for_seven_days() {
    let Some(url) = database_url_or_skip("an_unset_session_signs_a_saml_user_in_for_seven_days")
    else {
        return;
    };
    let db = "fraiseql_saml_acs_default";
    let idp = SigningIdp::new();
    let server = boot(&url, db, &idp, schema(None)).await;

    let claims = sign_in(&server, &idp).await;

    assert_eq!(lifetime(&claims), 7 * 24 * 3600, "{claims}");
    server.stop().await;
    drop_scratch(&url, db).await;
}

#[tokio::test]
async fn the_server_refuses_a_sha1_digest() {
    let Some(url) = database_url_or_skip("the_server_refuses_a_sha1_digest") else {
        return;
    };
    let db = "fraiseql_saml_acs_sha1";
    let idp = SigningIdp::new();
    let server = boot(&url, db, &idp, schema(None)).await;

    let (relay_state, request_id) = server.login().await;
    let response = idp.response(&request_id, "ada", &DigestAlgorithm::Sha1);
    let (status, body) = server.acs(&relay_state, &response).await;

    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "{body}");
    assert!(body.get("access_token").is_none(), "a SHA-1 digest signed someone in: {body}");
    server.stop().await;
    drop_scratch(&url, db).await;
}

#[tokio::test]
async fn a_response_to_another_request_is_refused() {
    let Some(url) = database_url_or_skip("a_response_to_another_request_is_refused") else {
        return;
    };
    let db = "fraiseql_saml_acs_other_request";
    let idp = SigningIdp::new();
    let server = boot(&url, db, &idp, schema(None)).await;

    let (relay_state, _) = server.login().await;
    let response = idp.response("id-another-request", "ada", &DigestAlgorithm::Sha256);
    let (status, body) = server.acs(&relay_state, &response).await;

    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "{body}");
    assert!(body.get("access_token").is_none(), "{body}");
    server.stop().await;
    drop_scratch(&url, db).await;
}

#[tokio::test]
async fn a_relay_state_signs_in_once() {
    let Some(url) = database_url_or_skip("a_relay_state_signs_in_once") else {
        return;
    };
    let db = "fraiseql_saml_acs_relay_once";
    let idp = SigningIdp::new();
    let server = boot(&url, db, &idp, schema(None)).await;

    let (relay_state, request_id) = server.login().await;
    let first = idp.response(&request_id, "ada", &DigestAlgorithm::Sha256);
    let (status, body) = server.acs(&relay_state, &first).await;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    // A second, distinct response to the same request: the assertion is new, so only the
    // spent RelayState can refuse it.
    let second = idp.response(&request_id, "ada", &DigestAlgorithm::Sha256);
    let (status, body) = server.acs(&relay_state, &second).await;

    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "{body}");
    assert!(body.get("access_token").is_none(), "{body}");
    server.stop().await;
    drop_scratch(&url, db).await;
}

// ── On two roles ────────────────────────────────────────────────────────────

/// The engine role the two-role test boots the engine on: it can connect and create in
/// its scratch database, and has no grant on schema `auth`.
const ENGINE_ROLE: &str = "fraiseql_saml_engine";
const ENGINE_PASSWORD: &str = "fraiseql_saml_engine_password";

/// The auth role: `AUTH_DATABASE_URL`'s user and password when set, else the defaults the
/// test database's init script creates.
fn auth_credentials() -> (String, String) {
    std::env::var("AUTH_DATABASE_URL").ok().filter(|v| !v.is_empty()).map_or_else(
        || ("fraiseql_auth".to_string(), "fraiseql_auth_test_password".to_string()),
        |v| {
            let u = url::Url::parse(&v).expect("AUTH_DATABASE_URL parses");
            (u.username().to_string(), u.password().unwrap_or_default().to_string())
        },
    )
}

fn with_user(url: &str, user: &str, password: &str) -> String {
    let mut out = url::Url::parse(url).expect("database URL parses");
    out.set_username(user).expect("set user");
    out.set_password(Some(password)).expect("set password");
    out.to_string()
}

/// SAML runs on auth's pool: with the engine on a role that cannot read schema `auth`, a
/// sign-in through the ACS completes, and every auth table, the replay cache's included,
/// belongs to the auth role.
#[tokio::test]
async fn a_saml_sign_in_runs_on_the_auth_role() {
    let Some(url) = database_url_or_skip("a_saml_sign_in_runs_on_the_auth_role") else {
        return;
    };
    std::env::set_var(SECRET_ENV, HS256_SECRET);
    let db = "fraiseql_saml_acs_two_roles";
    let pool = scratch_pool(&url, db).await;
    let (auth_role, auth_password) = auth_credentials();
    for (role, password) in [
        (ENGINE_ROLE, ENGINE_PASSWORD),
        (auth_role.as_str(), auth_password.as_str()),
    ] {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = $1)")
                .bind(role)
                .fetch_one(&pool)
                .await
                .expect("read pg_roles");
        if !exists {
            sqlx::raw_sql(&format!(
                "CREATE ROLE {role} LOGIN PASSWORD '{password}' NOSUPERUSER NOBYPASSRLS"
            ))
            .execute(&pool)
            .await
            .expect("create role");
        }
    }
    sqlx::raw_sql(&format!(
        "GRANT CONNECT, CREATE ON DATABASE {db} TO {ENGINE_ROLE}, {auth_role}; \
         GRANT USAGE, CREATE ON SCHEMA public TO {ENGINE_ROLE};"
    ))
    .execute(&pool)
    .await
    .expect("grant on the scratch database");
    let scratch_url = with_database(&url, db);
    let engine_url = with_user(&scratch_url, ENGINE_ROLE, ENGINE_PASSWORD);
    std::env::set_var(AUTH_URL_ENV, with_user(&scratch_url, &auth_role, &auth_password));

    let idp = SigningIdp::new();
    let adapter = Arc::new(PostgresAdapter::new(&engine_url).await.expect("PostgresAdapter::new"));
    let engine_pool = PgPool::connect(&engine_url).await.expect("connect as the engine role");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    let server = Box::pin(Server::new(
        saml_config(&idp, &engine_url),
        schema(None),
        adapter,
        Some(engine_pool),
    ))
    .await
    .expect("[saml] with [auth.database] must construct on two roles");
    let (shutdown, rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
            .expect("serve");
    });
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    let booted = Booted {
        base: format!("http://127.0.0.1:{port}"),
        client: reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("client"),
        shutdown,
        handle,
    };

    let claims = sign_in(&booted, &idp).await;
    booted.stop().await;

    assert!(claims["sub"].is_string(), "the sign-in mints a session: {claims}");
    let foreign: Vec<(String, String)> = sqlx::query_as(
        "SELECT tablename::text, tableowner::text FROM pg_tables \
         WHERE schemaname = 'auth' AND tableowner <> $1 ORDER BY 1",
    )
    .bind(&auth_role)
    .fetch_all(&pool)
    .await
    .expect("read pg_tables");
    assert!(foreign.is_empty(), "every auth table must belong to {auth_role}: {foreign:?}");
    let replay: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_tables WHERE schemaname = 'auth' \
         AND tablename = 'tb_saml_replay')",
    )
    .fetch_one(&pool)
    .await
    .expect("read pg_tables");
    assert!(replay, "the replay cache's table is provisioned in auth");
    pool.close().await;
    drop_scratch(&url, db).await;
}

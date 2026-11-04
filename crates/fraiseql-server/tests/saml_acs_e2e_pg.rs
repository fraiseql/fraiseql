//! A SAML sign-in through the shipped server: `GET /auth/saml/login`, a signed
//! `SAMLResponse` posted to `POST /auth/saml/acs`, and the session it mints.
//!
//! The responses are signed the way a production `IdP` signs them: RSA-SHA256 over a
//! SHA-256 digest. samael's own template asks for a SHA-1 digest, which the server's
//! algorithm allow-list refuses, so the fixture sets the digest on that template before
//! signing; nothing in the server is relaxed for the test.
//!
//! Self-skips when no `DATABASE_URL` is set. Runs in the Dagger `saml`
//! integration suite (which has the libxml2/xmlsec1 stack and Postgres).
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
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
    schema.auth = session.map(|session| AuthClientConfig {
        pkce:    None,
        social:  None,
        local:   None,
        session: Some(session),
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

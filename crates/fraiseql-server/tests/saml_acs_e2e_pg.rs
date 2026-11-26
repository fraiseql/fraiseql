//! A SAML sign-in through the shipped server: `GET /auth/saml/login`, a signed
//! `SAMLResponse` posted to `POST /auth/saml/acs`, and the session it mints.
//!
//! The responses are signed the way a production `IdP` signs them: RSA-SHA256 over a
//! SHA-256 digest. samael's own template asks for a SHA-1 digest, which the server's
//! algorithm allow-list refuses, so the fixture sets the digest on that template before
//! signing; nothing in the server is relaxed for the test. One sign-in runs with the
//! engine on a role that cannot read schema `auth`, and auth on a role of its own. In
//! cookie transport the ACS redirects to `[saml] post_login_redirect_uri` with the session
//! cookies, and the session works on `/graphql`, refreshes and logs out by cookie alone.
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

mod common;

use std::{io::Read as _, sync::Arc};

use base64::Engine as _;
use common::first_party::FirstPartyAuth;
use fraiseql_core::{
    db::postgres::PostgresAdapter,
    schema::{AuthClientConfig, CompiledSchema, SessionConfig, SessionTransport},
};
use fraiseql_server::{
    Server,
    server_config::{
        ServerConfig,
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
/// How this server signs the sessions it mints.
const FIRST_PARTY: FirstPartyAuth = FirstPartyAuth::new(ISSUER, AUDIENCE);

fn auth_database() -> fraiseql_core::schema::AuthDatabaseConfig {
    fraiseql_core::schema::AuthDatabaseConfig::new(AUTH_URL_ENV)
}

const IDP_ENTITY: &str = "https://idp.example.com";
const SP_ENTITY: &str = "https://sp.example.com/metadata";
const SP_ACS: &str = "https://sp.example.com/auth/saml/acs";
const ISSUER: &str = "https://sp.example.com";
const AUDIENCE: &str = "fraiseql";
const TENANT: &str = "33333333-3333-4333-8333-333333333333";

fn with_database(url: &str, db: &str) -> String {
    let (base, _old) = url.rsplit_once('/').expect("database URL has a path component");
    format!("{base}/{db}")
}

async fn scratch_pool(admin_url: &str, db: &str) -> PgPool {
    let admin = PgPool::connect(admin_url).await.expect("connect to admin database");
    // Reason: test SQL built from names and values this test defines.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS {db} WITH (FORCE)")))
        .execute(&admin)
        .await
        .expect("drop scratch database");
    // Reason: test SQL built from names and values this test defines.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE DATABASE {db}")))
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
    // Reason: test SQL built from names and values this test defines.
    let _ =
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS {db} WITH (FORCE)")))
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
            data.not_on_or_after =
                Some((std::time::SystemTime::now() + std::time::Duration::from_mins(5)).into());
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
/// `[auth_issuer]` the sessions are signed with.
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
    FIRST_PARTY.apply(ServerConfig {
        // #874: production validate() refuses cors_enabled=true + empty origins
        cors_enabled: false,
        database_url: database_url.to_string(),
        saml: Some(SamlServerConfig {
            post_login_redirect_uri: None,
            idps,
            store_enabled: false,
            refresh_interval_secs: 30,
            certificate_expiry_warning_days: 30,
            sp: None,
        }),
        ..ServerConfig::default()
    })
}

/// A schema with no types, and `[auth.session]` as given.
fn schema(session: SessionConfig) -> CompiledSchema {
    let mut schema: CompiledSchema = serde_json::from_value(serde_json::json!({
        "fraiseql_version": fraiseql_core::schema::CURRENT_FRAISEQL_VERSION,
        "types": [],
        "queries": [],
        "mutations": [],
    }))
    .expect("compiled schema");
    schema.auth = Some(AuthClientConfig {
        pkce:     None,
        social:   None,
        local:    None,
        session:  Some(session),
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
    schema(FIRST_PARTY.session());
    schema(SessionConfig {
        transport:         fraiseql_core::schema::SessionTransport::Json,
        ttl_secs:          Some(900),
        absolute_ttl_secs: None,
    });
}

/// The server, booted on a scratch database named `db`.
struct Booted {
    base:     String,
    client:   reqwest::Client,
    shutdown: tokio::sync::oneshot::Sender<()>,
    handle:   tokio::task::JoinHandle<()>,
    /// The login cookie each `RelayState` was started with, by `RelayState`: what the
    /// browser that started it sends back to the ACS.
    started:  std::sync::Mutex<std::collections::HashMap<String, String>>,
}

async fn boot(url: &str, db: &str, idp: &SigningIdp, schema: CompiledSchema) -> Booted {
    boot_with(url, db, schema, |scratch_url| saml_config(idp, scratch_url)).await
}

/// [`boot`] with the server configuration `config` builds for the scratch database URL.
async fn boot_with(
    url: &str,
    db: &str,
    schema: CompiledSchema,
    config: impl FnOnce(&str) -> ServerConfig,
) -> Booted {
    // Env var must exist before construction loads the secret. A fixed valid
    // value, set unconditionally: safe under the parallel runner because every
    // reader wants exactly this value.
    FIRST_PARTY.set_env();
    let pool = scratch_pool(url, db).await;
    let scratch_url = with_database(url, db);
    let adapter = Arc::new(PostgresAdapter::new(&scratch_url).await.expect("PostgresAdapter::new"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    let server = Box::pin(Server::new(config(&scratch_url), schema, adapter, Some(pool)))
        .await
        .expect("[saml] + [auth_issuer] + a pool must boot");
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
        started: std::sync::Mutex::default(),
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
        let cookie = login_cookie(&resp);
        let redirect = resp.headers()["location"].to_str().unwrap().to_string();
        let redirect = reqwest::Url::parse(&redirect).expect("the redirect is a URL");
        let param = |name: &str| {
            redirect.query_pairs().find(|(k, _)| k == name).map_or_else(
                || panic!("the redirect carries no {name}: {redirect}"),
                |(_, v)| v.into_owned(),
            )
        };
        let relay_state = param("RelayState");
        self.started.lock().unwrap().insert(relay_state.clone(), cookie);
        (relay_state, request_id(&param("SAMLRequest")))
    }

    /// The login cookie `relay_state` was started with.
    fn cookie_of(&self, relay_state: &str) -> String {
        self.started.lock().unwrap().get(relay_state).cloned().unwrap_or_default()
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
            .header(reqwest::header::COOKIE, self.cookie_of(relay_state))
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

/// The `name=value` of the login cookie the SAML login set on `resp`.
fn login_cookie(resp: &reqwest::Response) -> String {
    resp.headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|v| v.starts_with("__Host-fraiseql_login_"))
        .and_then(|v| v.split(';').next())
        .expect("the login sets the login cookie")
        .to_string()
}

/// The `Set-Cookie` headers of `resp`.
fn set_cookies(resp: &reqwest::Response) -> Vec<String> {
    resp.headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap_or_default().to_string())
        .collect()
}

/// Whether `resp` clears the login cookie whose `name=value` is `cookie`.
fn clears(resp: &reqwest::Response, cookie: &str) -> bool {
    let name = cookie.split('=').next().unwrap_or_default();
    set_cookies(resp)
        .iter()
        .any(|c| c.starts_with(&format!("{name}=;")) && c.contains("Max-Age=0"))
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
        transport:         fraiseql_core::schema::SessionTransport::Json,
        ttl_secs:          Some(900),
        absolute_ttl_secs: None,
    };
    let server = boot(&url, db, &idp, schema(session)).await;

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

/// The ACS answers the session in its `JSON` body: no session cookie, no redirect.
#[tokio::test]
async fn a_saml_sign_in_answers_its_tokens_in_json() {
    let Some(url) = database_url_or_skip("a_saml_sign_in_answers_its_tokens_in_json") else {
        return;
    };
    let db = "fraiseql_saml_acs_json";
    let idp = SigningIdp::new();
    let server = boot(&url, db, &idp, schema(FIRST_PARTY.session())).await;

    let (relay_state, request_id) = server.login().await;
    let response = idp.response(&request_id, "ada", &DigestAlgorithm::Sha256);
    let cookie = server.cookie_of(&relay_state);
    let resp = server
        .client
        .post(format!("{}/auth/saml/acs", server.base))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[
            ("SAMLResponse", response.as_str()),
            ("RelayState", relay_state.as_str()),
        ])
        .send()
        .await
        .expect("ACS request");
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let set = set_cookies(&resp);
    assert!(
        !set.iter().any(|c| c.starts_with(fraiseql_auth::session_cookies::ACCESS_COOKIE)
            || c.starts_with(fraiseql_auth::session_cookies::REFRESH_COOKIE)),
        "no session cookie: {set:?}"
    );
    assert!(clears(&resp, &cookie), "the answer clears the login cookie");
    assert!(resp.headers().get(reqwest::header::LOCATION).is_none(), "no redirect");
    let body: serde_json::Value = resp.json().await.expect("a JSON body");
    assert!(body["access_token"].is_string(), "{body}");
    assert!(body["refresh_token"].is_string(), "{body}");

    server.stop().await;
    drop_scratch(&url, db).await;
}

/// A SAML login completes only from the browser that started it: by the `SameSite=None`
/// cookie the login sets, which the `IdP`'s cross-site form `POST` carries back.
#[tokio::test]
async fn a_saml_login_completes_only_from_the_browser_that_started_it() {
    let Some(url) = database_url_or_skip("a_saml_login_completes_only_from_its_browser") else {
        return;
    };
    let db = "fraiseql_saml_acs_other_client";
    let idp = SigningIdp::new();
    let server = boot(&url, db, &idp, schema(FIRST_PARTY.session())).await;
    let other = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client");
    let acs =
        |client: &reqwest::Client, relay_state: &str, response: &str, cookie: Option<&str>| {
            let mut request = client.post(format!("{}/auth/saml/acs", server.base));
            if let Some(cookie) = cookie {
                request = request.header(reqwest::header::COOKIE, cookie.to_string());
            }
            request
                .form(&[
                    ("SAMLResponse", response.to_string()),
                    ("RelayState", relay_state.to_string()),
                ])
                .send()
        };

    // The cookie the login sets.
    let resp = server
        .client
        .get(format!("{}/auth/saml/login?idp=test-idp&tenant={TENANT}", server.base))
        .send()
        .await
        .expect("login request");
    let set = set_cookies(&resp).join("\n");
    for attribute in [
        "__Host-fraiseql_login_",
        "Path=/",
        "HttpOnly",
        "Secure",
        "SameSite=None",
    ] {
        assert!(set.contains(attribute), "{set}");
    }

    // Another browser, with no cookie: refused, the cookie cleared, and the RelayState spent.
    let (relay_state, request_id) = server.login().await;
    let cookie = server.cookie_of(&relay_state);
    let response = idp.response(&request_id, "ada", &DigestAlgorithm::Sha256);
    let resp = acs(&other, &relay_state, &response, None).await.expect("ACS request");
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);
    assert!(clears(&resp, &cookie), "a refusal clears the login cookie");
    let resp = acs(&server.client, &relay_state, &response, Some(&cookie)).await.expect("ACS");
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST, "a refused RelayState is spent");

    // The cookie's name with another value: refused.
    let (relay_state, request_id) = server.login().await;
    let name = server.cookie_of(&relay_state).split('=').next().expect("a name").to_string();
    let response = idp.response(&request_id, "ada", &DigestAlgorithm::Sha256);
    let mismatched = format!("{name}=0123abcd");
    let resp = acs(&other, &relay_state, &response, Some(&mismatched))
        .await
        .expect("ACS request");
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);

    // The browser that started it: completes.
    let (relay_state, request_id) = server.login().await;
    let response = idp.response(&request_id, "ada", &DigestAlgorithm::Sha256);
    let (status, body) = server.acs(&relay_state, &response).await;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");

    server.stop().await;
    drop_scratch(&url, db).await;
}

// ── Cookie transport ────────────────────────────────────────────────────────────────

/// The front-end page the ACS sends the browser to in cookie transport.
const POST_LOGIN: &str = "https://app.example.com/signed-in";

/// A refreshable `[auth.session]` as `transport`.
const fn session_as(transport: SessionTransport) -> SessionConfig {
    SessionConfig {
        ttl_secs: Some(900),
        absolute_ttl_secs: Some(86_400),
        transport,
    }
}

/// [`saml_config`] with `post_login_redirect_uri`.
fn saml_config_redirecting(
    idp: &SigningIdp,
    database_url: &str,
    redirect: Option<&str>,
) -> ServerConfig {
    let mut config = saml_config(idp, database_url);
    config.saml.as_mut().expect("[saml]").post_login_redirect_uri = redirect.map(str::to_string);
    config
}

/// The value of the cookie `name` among `set_cookies`.
fn set_cookie_value(set_cookies: &[String], name: &str) -> String {
    set_cookies
        .iter()
        .find_map(|c| c.strip_prefix(&format!("{name}=")))
        .and_then(|rest| rest.split(';').next())
        .unwrap_or_default()
        .to_string()
}

/// A session route with no credential but the refresh cookie `refresh`: its status,
/// `Set-Cookie`s and body.
async fn session_route(
    base: &str,
    route: &str,
    refresh: &str,
) -> (reqwest::StatusCode, Vec<String>, String) {
    let resp = reqwest::Client::new()
        .post(format!("{base}/auth/v1/session/{route}"))
        .header(reqwest::header::COOKIE, format!("{REFRESH_COOKIE}={refresh}"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("session request");
    let status = resp.status();
    let set = set_cookies(&resp);
    (status, set, resp.text().await.unwrap_or_default())
}

/// `/graphql` with no credential but the access cookie `access`.
async fn graphql_with_cookie(base: &str, access: &str) -> reqwest::StatusCode {
    reqwest::Client::new()
        .post(format!("{base}/graphql"))
        .header(reqwest::header::COOKIE, format!("{ACCESS_COOKIE}={access}"))
        .json(&serde_json::json!({ "query": "{ __typename }" }))
        .send()
        .await
        .expect("graphql request")
        .status()
}

const ACCESS_COOKIE: &str = fraiseql_auth::session_cookies::ACCESS_COOKIE;
const REFRESH_COOKIE: &str = fraiseql_auth::session_cookies::REFRESH_COOKIE;

/// In cookie transport the ACS answers `303` to `[saml] post_login_redirect_uri` with the
/// session cookies; neither its body nor its `Location` carries a token; and the session
/// they hold works on `/graphql`, refreshes and logs out by cookie alone.
#[tokio::test]
async fn in_cookie_transport_the_acs_sets_the_session_cookies_on_its_redirect() {
    let Some(url) = database_url_or_skip("in_cookie_transport_the_acs_sets_the_cookies") else {
        return;
    };
    let db = "fraiseql_saml_acs_cookie";
    let idp = SigningIdp::new();
    let server = boot_with(&url, db, schema(session_as(SessionTransport::Cookie)), |scratch| {
        saml_config_redirecting(&idp, scratch, Some(POST_LOGIN))
    })
    .await;

    let (relay_state, request_id) = server.login().await;
    let login = server.cookie_of(&relay_state);
    let response = idp.response(&request_id, "ada", &DigestAlgorithm::Sha256);
    let resp = server
        .client
        .post(format!("{}/auth/saml/acs", server.base))
        .header(reqwest::header::COOKIE, &login)
        .form(&[
            ("SAMLResponse", response.as_str()),
            ("RelayState", relay_state.as_str()),
        ])
        .send()
        .await
        .expect("ACS request");
    let status = resp.status();
    let location = resp.headers()[reqwest::header::LOCATION]
        .to_str()
        .expect("a Location")
        .to_string();
    let set = set_cookies(&resp);
    let cleared = clears(&resp, &login);
    let body = resp.text().await.unwrap_or_default();
    assert_eq!(status, reqwest::StatusCode::SEE_OTHER, "{body}");
    assert_eq!(location, POST_LOGIN, "the redirect carries nothing but its target");
    assert!(cleared, "the answer clears the login cookie: {set:?}");
    let access = set_cookie_value(&set, ACCESS_COOKIE);
    let refresh = set_cookie_value(&set, REFRESH_COOKIE);
    assert!(!access.is_empty() && !refresh.is_empty(), "{set:?}");
    for carried in [&body, &location] {
        for needle in [
            access.as_str(),
            refresh.as_str(),
            "access_token",
            "refresh_token",
        ] {
            assert!(!carried.contains(needle), "no body or Location carries a token: {carried}");
        }
    }

    assert_eq!(graphql_with_cookie(&server.base, &access).await, reqwest::StatusCode::OK);
    assert_eq!(
        graphql_with_cookie(&server.base, &format!("{access}x")).await,
        reqwest::StatusCode::UNAUTHORIZED,
        "the access cookie is what /graphql checks"
    );
    let (status, set, body) = session_route(&server.base, "refresh", &refresh).await;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    let next_access = set_cookie_value(&set, ACCESS_COOKIE);
    let next_refresh = set_cookie_value(&set, REFRESH_COOKIE);
    for needle in [
        access.as_str(),
        refresh.as_str(),
        &next_access,
        &next_refresh,
    ] {
        assert!(!body.contains(needle), "the refresh body carries no token: {body}");
    }
    assert_eq!(graphql_with_cookie(&server.base, &next_access).await, reqwest::StatusCode::OK);
    let (status, set, body) = session_route(&server.base, "logout", &next_refresh).await;
    assert_eq!(status, reqwest::StatusCode::NO_CONTENT, "{body}");
    for name in [ACCESS_COOKIE, REFRESH_COOKIE] {
        assert!(set.iter().any(|c| c.starts_with(&format!("{name}=;"))), "logout clears {name}");
    }

    server.stop().await;
    drop_scratch(&url, db).await;
}

/// `[saml] post_login_redirect_uri` is required in cookie transport, refused in `JSON`
/// transport, and refused when the browser could not be sent to it as written: each
/// refusal names the key.
#[tokio::test]
async fn the_post_login_redirect_is_required_by_cookie_transport_and_read_only_by_it() {
    let Some(url) = database_url_or_skip("the_post_login_redirect_is_required") else {
        return;
    };
    FIRST_PARTY.set_env();
    let db = "fraiseql_saml_acs_cookie_refusals";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);
    let idp = SigningIdp::new();
    let cookie = SessionTransport::Cookie;
    for (what, transport, redirect, names) in [
        ("cookie transport with no redirect", cookie, None, "[auth.session] transport"),
        (
            "JSON transport with a redirect",
            SessionTransport::Json,
            Some(POST_LOGIN),
            "transport",
        ),
        ("an http redirect", cookie, Some("http://app.example.com/"), "https"),
        ("a relative redirect", cookie, Some("/signed-in"), "absolute"),
        (
            "a redirect with a fragment",
            cookie,
            Some("https://app.example.com/#x"),
            "fragment",
        ),
    ] {
        let adapter =
            Arc::new(PostgresAdapter::new(&scratch_url).await.expect("PostgresAdapter::new"));
        let refused = Box::pin(Server::new(
            saml_config_redirecting(&idp, &scratch_url, redirect),
            schema(session_as(transport)),
            adapter,
            Some(pool.clone()),
        ))
        .await;
        let Err(error) = refused else {
            panic!("{what} must refuse to boot");
        };
        let error = error.to_string();
        assert!(
            error.contains("post_login_redirect_uri") && error.contains(names),
            "{what}: {error}"
        );
    }
    pool.close().await;
    drop_scratch(&url, db).await;
}

#[tokio::test]
async fn the_server_refuses_a_sha1_digest() {
    let Some(url) = database_url_or_skip("the_server_refuses_a_sha1_digest") else {
        return;
    };
    let db = "fraiseql_saml_acs_sha1";
    let idp = SigningIdp::new();
    let server = boot(&url, db, &idp, schema(FIRST_PARTY.session())).await;

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
    let server = boot(&url, db, &idp, schema(FIRST_PARTY.session())).await;

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
    let server = boot(&url, db, &idp, schema(FIRST_PARTY.session())).await;

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
    FIRST_PARTY.set_env();
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
            // Reason: test SQL built from names and values this test defines.
            sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
                "CREATE ROLE {role} LOGIN PASSWORD '{password}' NOSUPERUSER NOBYPASSRLS"
            )))
            .execute(&pool)
            .await
            .expect("create role");
        }
    }
    // Reason: test SQL built from names and values this test defines.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "GRANT CONNECT, CREATE ON DATABASE {db} TO {ENGINE_ROLE}, {auth_role}; \
         GRANT USAGE, CREATE ON SCHEMA public TO {ENGINE_ROLE};"
    )))
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
        schema(FIRST_PARTY.session()),
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
        started: std::sync::Mutex::default(),
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

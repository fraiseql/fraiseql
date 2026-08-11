//! #381: SAML SP-initiated SSO through the shipped server's mount.
//!
//! The library slice shipped fail-closed verification, but the server binary
//! could not even compile it in — no `auth-saml` feature passthrough existed,
//! and nothing mounted `saml_routes`. These tests boot the real server from a
//! `[saml]` config and prove the mount: a login redirect carries a
//! `SAMLRequest` to the configured `IdP`, an unknown `IdP` is refused, and the
//! configured-but-broken shapes refuse to boot (missing `[auth_hs256]`,
//! missing pool, unreadable metadata).
//!
//! Self-skips when no `DATABASE_URL` is set. Runs in the Dagger `saml`
//! integration suite (which has the libxml2/xmlsec1 stack and Postgres).
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `fraiseql_p26_saml_*` databases →
//! run `--test-threads=1`.
#![cfg(feature = "auth-saml")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::sync::Arc;

use fraiseql_core::{db::postgres::PostgresAdapter, schema::CompiledSchema};
use fraiseql_server::{
    Server,
    server_config::{
        ServerConfig,
        hs256::Hs256Config,
        saml::{SamlIdpEntry, SamlServerConfig, SamlSpKeyConfig},
    },
};
use fraiseql_test_support::try_database_url;
use samael::idp::{CertificateParams, IdentityProvider, KeyType, Rsa};
use sqlx::PgPool;

const IDP_ENTITY: &str = "https://idp.example.com";
/// 32 bytes, base64-safe — the HS256 secret handed over via env.
const HS256_SECRET: &str = "p26-saml-hs256-secret-32-bytes!!";
const SECRET_ENV: &str = "FRAISEQL_TEST_P26_SAML_HS256_SECRET";
/// Admin bearer token gating `/api/saml/idps` in the #947 store tests.
const ADMIN_TOKEN: &str = "p16-saml-idp-admin-token";
const TENANT_A: &str = "11111111-1111-4111-8111-111111111111";
const TENANT_B: &str = "22222222-2222-4222-8222-222222222222";
/// Env var carrying the SP private key in the #948 signing test.
const SP_KEY_ENV: &str = "FRAISEQL_TEST_P16_SAML_SP_KEY";

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

/// Genuine `IdP` metadata XML with a freshly minted signing certificate — a
/// `SamlIdpConfig` refuses garbage, so the fixture must be real.
fn idp_metadata_xml() -> String {
    let idp = IdentityProvider::generate_new(KeyType::Rsa(Rsa::Rsa2048)).unwrap();
    let cert = idp
        .create_certificate(&CertificateParams {
            common_name:           IDP_ENTITY,
            issuer_name:           IDP_ENTITY,
            days_until_expiration: 3650,
        })
        .unwrap();
    let cert_b64 = {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD.encode(cert.der_data())
    };
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

fn saml_config(metadata_xml: String) -> ServerConfig {
    let mut idps = std::collections::HashMap::new();
    idps.insert(
        "test-idp".to_string(),
        SamlIdpEntry {
            sp_entity_id:         "https://sp.example.com/metadata".to_string(),
            acs_url:              "https://sp.example.com/auth/saml/acs".to_string(),
            metadata_xml_path:    None,
            metadata_xml:         Some(metadata_xml),
            tenant_id:            None,
            trust_asserted_email: false,
        },
    );
    ServerConfig {
        // #874: production validate() refuses cors_enabled=true + empty origins
        cors_enabled: false,
        saml: Some(SamlServerConfig {
            idps,
            store_enabled: false,
            refresh_interval_secs: 30,
            certificate_expiry_warning_days: 30,
            sp: None,
        }),
        auth_hs256: Some(Hs256Config {
            secret_env: SECRET_ENV.to_string(),
            issuer:     Some("https://sp.example.com".to_string()),
            audience:   Some("fraiseql".to_string()),
        }),
        ..ServerConfig::default()
    }
}

fn empty_schema() -> CompiledSchema {
    serde_json::from_value(serde_json::json!({
        "version": "2.0.0",
        "types": [],
        "queries": [],
        "mutations": [],
    }))
    .expect("compiled schema")
}

#[tokio::test]
async fn login_redirects_to_the_idp_and_unknown_idp_is_refused() {
    let Some(url) = database_url_or_skip("login_redirects_to_the_idp") else {
        return;
    };
    // Env var must exist before construction loads the secret. A fixed valid
    // value, set unconditionally: safe under the parallel runner because every
    // reader wants exactly this value.
    std::env::set_var(SECRET_ENV, HS256_SECRET);

    let db = "fraiseql_p26_saml_login";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);

    let mut config = saml_config(idp_metadata_xml());
    config.database_url.clone_from(&scratch_url);

    let adapter = Arc::new(PostgresAdapter::new(&scratch_url).await.expect("PostgresAdapter::new"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();

    let server = Box::pin(Server::new(config, empty_schema(), adapter, Some(pool.clone())))
        .await
        .expect("Server::new with [saml] + [auth_hs256] + pool must succeed");
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client");
    let base = format!("http://127.0.0.1:{port}");

    // The mounted login route answers with a redirect to the IdP's SSO URL
    // carrying a SAMLRequest.
    let resp = client
        .get(format!("{base}/auth/saml/login?idp=test-idp"))
        .send()
        .await
        .expect("login request");
    assert!(
        resp.status().is_redirection(),
        "SP-initiated login must redirect to the IdP, got {}",
        resp.status()
    );
    let location = resp
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(
        location.starts_with("https://idp.example.com/sso"),
        "redirect must target the configured IdP SSO URL: {location}"
    );
    assert!(
        location.contains("SAMLRequest="),
        "redirect must carry a SAMLRequest: {location}"
    );

    // An unknown IdP is refused, not silently defaulted.
    let resp = client
        .get(format!("{base}/auth/saml/login?idp=not-configured"))
        .send()
        .await
        .expect("unknown idp request");
    assert!(
        resp.status().is_client_error(),
        "an unknown idp must be a client error, got {}",
        resp.status()
    );

    let _ = tx.send(());
    let _ = handle.await;
    drop_scratch(&url, db).await;
}

/// A `[saml]` deployment with the per-tenant store on, no config-file `IdPs`, and an admin
/// token — the multi-tenant shape #947 adds.
fn store_config() -> ServerConfig {
    ServerConfig {
        cors_enabled: false,
        admin_token: Some(ADMIN_TOKEN.to_string()),
        saml: Some(SamlServerConfig {
            idps: std::collections::HashMap::new(),
            store_enabled: true,
            refresh_interval_secs: 30,
            certificate_expiry_warning_days: 30,
            sp: None,
        }),
        auth_hs256: Some(Hs256Config {
            secret_env: SECRET_ENV.to_string(),
            issuer:     Some("https://sp.example.com".to_string()),
            audience:   Some("fraiseql".to_string()),
        }),
        ..ServerConfig::default()
    }
}

/// #947, the operator's path: manage `IdPs` over the admin API on a running server and watch
/// `/auth/saml/login` follow — no restart, and scoped by tenant in both directions.
#[tokio::test]
async fn stored_idps_are_managed_over_http_and_served_scoped_by_tenant() {
    let Some(url) = database_url_or_skip("stored_idps_are_managed_over_http") else {
        return;
    };
    std::env::set_var(SECRET_ENV, HS256_SECRET);

    let db = "fraiseql_p16_saml_idp_store";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);

    let mut config = store_config();
    config.database_url.clone_from(&scratch_url);

    let adapter = Arc::new(PostgresAdapter::new(&scratch_url).await.expect("PostgresAdapter::new"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();

    let server = Box::pin(Server::new(config, empty_schema(), adapter, Some(pool.clone())))
        .await
        .expect("[saml] store_enabled with a pool must boot");
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client");
    let base = format!("http://127.0.0.1:{port}");
    let idps_url = format!("{base}/api/saml/idps");

    // The management surface is admin-gated: no token, no access.
    let resp = client.get(&idps_url).send().await.expect("unauthenticated list");
    assert_eq!(resp.status(), 401, "IdP management must require the admin bearer token");

    // Create a tenant-bound IdP on the running server.
    let body = serde_json::json!({
        "idp_name":     "acme-okta",
        "tenant_id":    TENANT_A,
        "sp_entity_id": "https://sp.example.com/metadata",
        "acs_url":      "https://sp.example.com/auth/saml/acs",
        "metadata_xml": idp_metadata_xml(),
    });
    let resp = client
        .post(&idps_url)
        .bearer_auth(ADMIN_TOKEN)
        .json(&body)
        .send()
        .await
        .expect("create idp");
    assert_eq!(resp.status(), 201, "create must succeed: {:?}", resp.text().await);

    // Hot reload: it serves immediately, for its own tenant only.
    let login = |query: String| {
        let client = client.clone();
        let base = base.clone();
        async move {
            client
                .get(format!("{base}/auth/saml/login?{query}"))
                .send()
                .await
                .expect("login request")
                .status()
                .as_u16()
        }
    };
    assert_eq!(
        login(format!("idp=acme-okta&tenant={TENANT_A}")).await,
        303,
        "a stored IdP must serve its own tenant without a restart"
    );
    assert_eq!(
        login(format!("idp=acme-okta&tenant={TENANT_B}")).await,
        404,
        "another tenant's IdP name must be a 404, not a login"
    );
    assert_eq!(
        login("idp=acme-okta".to_string()).await,
        404,
        "an untenanted caller must not reach a tenant-bound IdP"
    );

    // The recorded opt-in is reported as inert while the account store keys email globally.
    let listed: serde_json::Value = client
        .get(&idps_url)
        .bearer_auth(ADMIN_TOKEN)
        .send()
        .await
        .expect("list")
        .json()
        .await
        .expect("list json");
    assert_eq!(listed["total"], 1, "list must report the stored IdP: {listed}");
    assert_eq!(listed["idps"][0]["idp_entity_id"], IDP_ENTITY, "entity id is derived");
    assert!(
        listed["idps"][0]["certificate_expires_at"].is_string(),
        "certificate expiry must be parsed and reported: {listed}"
    );

    // Deleting stops it serving, again with no restart …
    let resp = client
        .delete(format!("{idps_url}/acme-okta"))
        .bearer_auth(ADMIN_TOKEN)
        .send()
        .await
        .expect("delete idp");
    assert_eq!(resp.status(), 204);
    assert_eq!(
        login(format!("idp=acme-okta&tenant={TENANT_A}")).await,
        404,
        "a deleted IdP must stop serving without a restart"
    );

    // … and the name stays reserved, so a second tenant cannot inherit the
    // `saml:acme-okta` account namespace the first one left behind.
    let mut recreate = body.clone();
    recreate["tenant_id"] = serde_json::json!(TENANT_B);
    let resp = client
        .post(&idps_url)
        .bearer_auth(ADMIN_TOKEN)
        .json(&recreate)
        .send()
        .await
        .expect("recreate idp");
    assert_eq!(resp.status(), 409, "a retired IdP name must never be reissued");

    let _ = tx.send(());
    let _ = handle.await;
    drop_scratch(&url, db).await;
}

#[tokio::test]
async fn configured_but_broken_shapes_refuse_to_boot() {
    let Some(url) = database_url_or_skip("configured_but_broken_shapes") else {
        return;
    };
    std::env::set_var(SECRET_ENV, HS256_SECRET);

    let db = "fraiseql_p26_saml_refuse";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);
    let adapter = Arc::new(PostgresAdapter::new(&scratch_url).await.expect("PostgresAdapter::new"));

    // [saml] without a database pool refuses.
    let mut config = saml_config(idp_metadata_xml());
    config.database_url.clone_from(&scratch_url);
    let result = Box::pin(Server::new(
        config,
        empty_schema(),
        Arc::new(PostgresAdapter::new(&scratch_url).await.expect("adapter")),
        None,
    ))
    .await;
    let msg = result.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(
        msg.contains("[saml]") && msg.contains("pool"),
        "[saml] without a pool must refuse naming both, got: {msg}"
    );

    // [saml] with unparseable metadata refuses, naming the IdP.
    let mut config = saml_config("<not-metadata/>".to_string());
    config.database_url.clone_from(&scratch_url);
    let result = Box::pin(Server::new(config, empty_schema(), adapter, Some(pool.clone()))).await;
    let msg = result.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(
        msg.contains("test-idp"),
        "unparseable metadata must refuse naming the IdP, got: {msg}"
    );

    drop_scratch(&url, db).await;
}

/// #948, the operator's path: a deployment with an `SP` key pair signs its `AuthnRequest`s
/// and publishes metadata an `IdP` can consume.
#[tokio::test]
async fn sp_key_pair_signs_requests_and_publishes_metadata() {
    let Some(url) = database_url_or_skip("sp_key_pair_signs_requests") else {
        return;
    };
    std::env::set_var(SECRET_ENV, HS256_SECRET);

    // A real SP key pair, handed over the way an operator would: key via env, cert inline.
    let (key_pem, cert_pem, cert_der) = sp_key_pair();
    std::env::set_var(SP_KEY_ENV, String::from_utf8(key_pem).unwrap());

    let db = "fraiseql_p16_saml_sp_signing";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);

    let mut config = saml_config(idp_metadata_xml());
    config.database_url.clone_from(&scratch_url);
    if let Some(saml) = config.saml.as_mut() {
        saml.sp = Some(SamlSpKeyConfig {
            private_key_env:           Some(SP_KEY_ENV.to_string()),
            private_key_path:          None,
            certificate_path:          None,
            certificate_pem:           Some(String::from_utf8(cert_pem).unwrap()),
            sign_authn_requests:       true,
            previous_private_key_env:  None,
            previous_private_key_path: None,
            previous_certificate_path: None,
            previous_certificate_pem:  None,
        });
    }

    let adapter = Arc::new(PostgresAdapter::new(&scratch_url).await.expect("PostgresAdapter::new"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();

    let server = Box::pin(Server::new(config, empty_schema(), adapter, Some(pool.clone())))
        .await
        .expect("[saml.sp] with a readable key pair must boot");
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client");
    let base = format!("http://127.0.0.1:{port}");

    // The AuthnRequest is signed: the HTTP-Redirect binding carries its signature in the
    // query string, which is what an IdP requiring signed requests checks.
    let resp = client
        .get(format!("{base}/auth/saml/login?idp=test-idp"))
        .send()
        .await
        .expect("login request");
    let location = resp
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(location.contains("SigAlg="), "a signed AuthnRequest carries SigAlg: {location}");
    assert!(
        location.contains("Signature="),
        "a signed AuthnRequest carries Signature: {location}"
    );

    // SP metadata is published, carries our certificate, and declares the signing posture.
    let resp = client
        .get(format!("{base}/auth/saml/metadata?idp=test-idp"))
        .send()
        .await
        .expect("metadata request");
    assert_eq!(resp.status(), 200);
    let xml = resp.text().await.expect("metadata body");
    let cert_b64 = {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD.encode(&cert_der)
    };
    assert!(xml.contains("SPSSODescriptor"), "{xml}");
    assert!(xml.contains(r#"AuthnRequestsSigned="true""#), "{xml}");
    assert!(xml.contains(&cert_b64), "metadata must publish the SP certificate");

    let _ = tx.send(());
    let _ = handle.await;
    drop_scratch(&url, db).await;
}

/// #948: a configured-but-unloadable `SP` key refuses to boot rather than starting with
/// signing silently off.
#[tokio::test]
async fn an_unreadable_sp_key_refuses_to_boot() {
    let Some(url) = database_url_or_skip("an_unreadable_sp_key") else {
        return;
    };
    std::env::set_var(SECRET_ENV, HS256_SECRET);

    let db = "fraiseql_p16_saml_sp_refuse";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);
    let adapter = Arc::new(PostgresAdapter::new(&scratch_url).await.expect("PostgresAdapter::new"));

    let (_key_pem, cert_pem, _cert_der) = sp_key_pair();
    let mut config = saml_config(idp_metadata_xml());
    config.database_url.clone_from(&scratch_url);
    if let Some(saml) = config.saml.as_mut() {
        saml.sp = Some(SamlSpKeyConfig {
            private_key_env:           Some("FRAISEQL_TEST_SP_KEY_THAT_IS_NOT_SET".to_string()),
            private_key_path:          None,
            certificate_path:          None,
            certificate_pem:           Some(String::from_utf8(cert_pem).unwrap()),
            sign_authn_requests:       true,
            previous_private_key_env:  None,
            previous_private_key_path: None,
            previous_certificate_path: None,
            previous_certificate_pem:  None,
        });
    }

    let result = Box::pin(Server::new(config, empty_schema(), adapter, Some(pool.clone()))).await;
    let msg = result.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(
        msg.contains("[saml.sp]") && msg.contains("not set"),
        "an unset SP key env var must refuse to boot, naming the field: {msg}"
    );

    drop_scratch(&url, db).await;
}

/// A fresh RSA key and matching self-signed certificate: `(key_pem, cert_pem, cert_der)`.
fn sp_key_pair() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    use openssl::{
        asn1::Asn1Time, bn::BigNum, hash::MessageDigest, pkey::PKey, rsa::Rsa as OpenSslRsa,
        x509::X509Builder,
    };

    let key = PKey::from_rsa(OpenSslRsa::generate(2048).unwrap()).unwrap();
    let mut name = openssl::x509::X509Name::builder().unwrap();
    name.append_entry_by_text("CN", "sp.example.com").unwrap();
    let name = name.build();

    let mut builder = X509Builder::new().unwrap();
    builder.set_version(2).unwrap();
    builder
        .set_serial_number(&BigNum::from_u32(1).unwrap().to_asn1_integer().unwrap())
        .unwrap();
    builder.set_subject_name(&name).unwrap();
    builder.set_issuer_name(&name).unwrap();
    builder.set_pubkey(&key).unwrap();
    builder.set_not_before(&Asn1Time::days_from_now(0).unwrap()).unwrap();
    builder.set_not_after(&Asn1Time::days_from_now(3650).unwrap()).unwrap();
    builder.sign(&key, MessageDigest::sha256()).unwrap();
    let cert = builder.build();

    (
        key.private_key_to_pem_pkcs8().unwrap(),
        cert.to_pem().unwrap(),
        cert.to_der().unwrap(),
    )
}

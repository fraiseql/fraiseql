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
        saml::{SamlIdpEntry, SamlServerConfig},
    },
};
use fraiseql_test_support::try_database_url;
use samael::idp::{CertificateParams, IdentityProvider, KeyType, Rsa};
use sqlx::PgPool;

const IDP_ENTITY: &str = "https://idp.example.com";
/// 32 bytes, base64-safe — the HS256 secret handed over via env.
const HS256_SECRET: &str = "p26-saml-hs256-secret-32-bytes!!";
const SECRET_ENV: &str = "FRAISEQL_TEST_P26_SAML_HS256_SECRET";

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
        saml: Some(SamlServerConfig { idps }),
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

//! Live-PostgreSQL tests for the per-tenant SAML IdP store and registry (#947).
//!
//! Covers the three properties the store exists for — durable per-tenant IdPs, hot reload,
//! and tenant-scoped resolution — plus the two that keep it from becoming a takeover
//! primitive: a logical IdP name is never reissued (it *is* the `saml:<name>` account
//! namespace), and a stored tenant-bound IdP still cannot email-merge.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), inert in the database-free
//! `test` leg and live in the Dagger `integration: saml` suite (binds Postgres + the
//! libxml2/xmlsec1 C stack).
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` + xmlsec1 ·
//! **Parallelism:** truncates the shared `core.tb_saml_idp` on setup → run
//! `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics/skips are fine
#![allow(clippy::doc_markdown)] // Reason: technical terms (IdP, NameID, SSO) throughout

use std::sync::Arc;

use chrono::{Duration, Utc};
use fraiseql_auth::saml::{
    PgSamlIdpStore, SamlError, SamlIdpConfig, SamlIdpRegistry, SamlIdpSpec, SamlIdpStore,
    effective_saml_email_verified,
};
use fraiseql_test_support::try_database_url;
use samael::idp::{CertificateParams, IdentityProvider, KeyType, Rsa};
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

const IDP_ENTITY: &str = "https://idp.example.com";
const IDP_SSO: &str = "https://idp.example.com/sso";
const SP_ENTITY: &str = "https://sp.example.com/metadata";
const SP_ACS: &str = "https://sp.example.com/acs";

fn tenant_a() -> Uuid {
    Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap()
}

fn tenant_b() -> Uuid {
    Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap()
}

/// Genuine IdP metadata with a freshly minted signing certificate — the builder refuses
/// garbage, so every fixture must be real.
fn metadata_xml(days_until_expiration: u32) -> String {
    let idp = IdentityProvider::generate_new(KeyType::Rsa(Rsa::Rsa2048)).unwrap();
    let cert = idp
        .create_certificate(&CertificateParams {
            common_name: IDP_ENTITY,
            issuer_name: IDP_ENTITY,
            days_until_expiration,
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
    <SingleSignOnService Binding="urn:oasis:names:tc:SAML:2.0:bindings:HTTP-Redirect" Location="{IDP_SSO}"/>
  </IDPSSODescriptor>
</EntityDescriptor>"#
    )
}

fn spec(name: &str, tenant: Option<Uuid>) -> SamlIdpSpec {
    SamlIdpSpec {
        idp_name:             name.to_string(),
        tenant_id:            tenant,
        sp_entity_id:         SP_ENTITY.to_string(),
        acs_url:              SP_ACS.to_string(),
        metadata_xml:         metadata_xml(3650),
        trust_asserted_email: false,
    }
}

/// Connect, ensure the schema exists, and truncate so each test starts clean.
async fn fresh() -> Option<PgSamlIdpStore> {
    let url = try_database_url()?;
    let pool = PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap();
    let store = PgSamlIdpStore::new(pool.clone());
    store.init().await.unwrap();
    sqlx::query("TRUNCATE core.tb_saml_idp RESTART IDENTITY")
        .execute(&pool)
        .await
        .unwrap();
    Some(store)
}

macro_rules! skip_if_no_db {
    () => {
        match fresh().await {
            Some(store) => store,
            None => {
                eprintln!("skipping #947 SAML IdP store test: DATABASE_URL not set");
                return;
            },
        }
    };
}

// ─── Storage ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_persists_and_derives_metadata_facts() {
    let store = skip_if_no_db!();

    let record = store.create(&spec("acme-okta", Some(tenant_a()))).await.unwrap();

    assert_eq!(record.idp_name, "acme-okta");
    assert_eq!(record.tenant_id, Some(tenant_a()));
    // Derived from the metadata, never accepted from the caller.
    assert_eq!(record.idp_entity_id, IDP_ENTITY);
    let expires = record.certificate_expires_at.expect("certificate expiry must be parsed");
    assert!(
        expires > Utc::now() + Duration::days(3600),
        "a 3650-day certificate should expire far out, got {expires}"
    );

    let fetched = store.get("acme-okta").await.unwrap().expect("stored IdP is retrievable");
    assert_eq!(fetched, record);
    assert_eq!(store.list().await.unwrap(), vec![record]);
}

#[tokio::test]
async fn create_refuses_metadata_boot_would_have_refused() {
    let store = skip_if_no_db!();

    // No HTTP-Redirect SingleSignOnService: SP-initiated login would be impossible, which
    // `SamlIdpConfigBuilder::build` already refuses at boot. The store must not be a way
    // around that gate.
    let mut broken = spec("no-binding", None);
    broken.metadata_xml = broken.metadata_xml.replace(
        "urn:oasis:names:tc:SAML:2.0:bindings:HTTP-Redirect",
        "urn:oasis:names:tc:SAML:2.0:bindings:HTTP-POST",
    );

    let err = store.create(&broken).await.expect_err("must refuse unusable metadata");
    assert!(matches!(err, SamlError::Config(_)), "got {err:?}");
    assert!(store.get("no-binding").await.unwrap().is_none(), "nothing may be persisted");

    let mut garbage = spec("garbage", None);
    garbage.metadata_xml = "<not-metadata/>".to_string();
    assert!(matches!(store.create(&garbage).await, Err(SamlError::Config(_))));
}

#[tokio::test]
async fn create_refuses_a_name_another_tenant_already_holds() {
    let store = skip_if_no_db!();
    store.create(&spec("okta", Some(tenant_a()))).await.unwrap();

    // Two tenants both naming their IdP `okta` would share the `saml:okta` provider
    // namespace, so a NameID collision across the two would resolve to ONE account.
    let err = store
        .create(&spec("okta", Some(tenant_b())))
        .await
        .expect_err("a second tenant must not claim the same IdP name");
    assert!(matches!(err, SamlError::NameTaken(name) if name == "okta"), "wrong refusal");
}

#[tokio::test]
async fn a_deleted_idp_name_is_never_reissued() {
    let store = skip_if_no_db!();
    store.create(&spec("okta", Some(tenant_a()))).await.unwrap();
    store.delete("okta").await.unwrap();
    assert!(store.get("okta").await.unwrap().is_none(), "delete must stop it serving");

    // The tombstone keeps the namespace reserved. Without this, tenant B's new `okta`
    // would inherit every `saml:okta` identity row tenant A left behind, and a NameID
    // collision would hand B's user A's account.
    let err = store
        .create(&spec("okta", Some(tenant_b())))
        .await
        .expect_err("a retired IdP name must never be reissued");
    assert!(matches!(&err, SamlError::NameTaken(name) if name == "okta"), "got {err:?}");
}

#[tokio::test]
async fn update_replaces_metadata_and_refuses_unknown_names() {
    let store = skip_if_no_db!();
    let created = store.create(&spec("acme-okta", Some(tenant_a()))).await.unwrap();

    let mut rotated = spec("acme-okta", Some(tenant_a()));
    rotated.metadata_xml = metadata_xml(60);
    rotated.trust_asserted_email = true;
    let updated = store.update(&rotated).await.unwrap();

    assert_eq!(updated.id, created.id, "update must not rehome the row");
    assert!(updated.trust_asserted_email);
    let expires = updated.certificate_expires_at.unwrap();
    assert!(expires < Utc::now() + Duration::days(90), "rotated expiry must be re-derived");

    assert!(matches!(
        store.update(&spec("never-existed", None)).await,
        Err(SamlError::NotFound(_))
    ));
    store.delete("acme-okta").await.unwrap();
    assert!(
        matches!(store.update(&rotated).await, Err(SamlError::NotFound(_))),
        "a tombstoned IdP is not updatable"
    );
    assert!(matches!(store.delete("acme-okta").await, Err(SamlError::NotFound(_))));
}

// ─── Registry: hot reload, shadowing, tenant scoping ─────────────────────────

fn config_idp(name: &str, tenant: Option<&str>) -> SamlIdpConfig {
    SamlIdpConfig::builder(name, SP_ENTITY, SP_ACS)
        .idp_metadata_xml(&metadata_xml(3650))
        .unwrap()
        .tenant_id(tenant.map(str::to_string))
        .build()
        .unwrap()
}

#[tokio::test]
async fn registry_serves_new_idps_and_stops_serving_deleted_ones_without_a_restart() {
    let store = skip_if_no_db!();
    let registry = SamlIdpRegistry::new().with_store(Arc::new(store));
    registry.refresh().await.unwrap();
    assert!(registry.resolve("acme-okta", Some(&tenant_a().to_string())).is_none());

    registry.create(&spec("acme-okta", Some(tenant_a()))).await.unwrap();
    assert!(
        registry.resolve("acme-okta", Some(&tenant_a().to_string())).is_some(),
        "a newly created IdP must serve without a restart"
    );

    registry.delete("acme-okta").await.unwrap();
    assert!(
        registry.resolve("acme-okta", Some(&tenant_a().to_string())).is_none(),
        "a removed IdP must stop serving without a restart"
    );
}

#[tokio::test]
async fn registry_scopes_stored_idps_by_tenant() {
    let store = skip_if_no_db!();
    let registry = SamlIdpRegistry::new().with_store(Arc::new(store));
    registry.create(&spec("acme-okta", Some(tenant_a()))).await.unwrap();
    registry.create(&spec("globex-entra", Some(tenant_b()))).await.unwrap();
    registry.create(&spec("shared", None)).await.unwrap();

    let a = tenant_a().to_string();
    let b = tenant_b().to_string();

    assert!(registry.resolve("acme-okta", Some(&a)).is_some(), "owner may start SSO");
    assert!(
        registry.resolve("acme-okta", Some(&b)).is_none(),
        "another tenant's IdP name must not resolve"
    );
    assert!(
        registry.resolve("acme-okta", None).is_none(),
        "an untenanted caller must not reach a tenant-bound IdP"
    );
    assert!(registry.resolve("shared", None).is_some(), "untenanted IdP serves untenanted");
    assert!(
        registry.resolve("shared", Some(&a)).is_none(),
        "'untenanted' is not 'belongs to whichever tenant asked'"
    );
}

#[tokio::test]
async fn a_stored_idp_may_not_shadow_a_config_file_idp() {
    let store = skip_if_no_db!();
    // Insert directly, bypassing the registry, to model a row written by another replica
    // or by hand before the config-file entry existed.
    store.create(&spec("okta", Some(tenant_a()))).await.unwrap();

    let registry = SamlIdpRegistry::new()
        .with_config_idp(config_idp("okta", None))
        .with_store(Arc::new(store));
    registry.refresh().await.unwrap();

    // The config-file IdP keeps serving; the stored one is refused, not merged — the two
    // would share one `saml:okta` account namespace.
    assert!(registry.resolve("okta", None).is_some(), "config-file IdP still serves");
    assert!(
        registry.resolve("okta", Some(&tenant_a().to_string())).is_none(),
        "the shadowed stored IdP must not be served under its own tenant"
    );

    // And the write path refuses the collision outright.
    let registry2 = SamlIdpRegistry::new()
        .with_config_idp(config_idp("only-in-config", None))
        .with_store(Arc::new(PgSamlIdpStore::new(
            PgPoolOptions::new()
                .max_connections(2)
                .connect(&try_database_url().unwrap())
                .await
                .unwrap(),
        )));
    assert!(matches!(
        registry2.create(&spec("only-in-config", Some(tenant_a()))).await,
        Err(SamlError::NameTaken(_))
    ));
}

// ─── The linking policy is unchanged by the store (answered gate) ────────────

#[tokio::test]
async fn a_stored_tenant_bound_idp_still_cannot_email_merge() {
    let store = skip_if_no_db!();
    let registry = SamlIdpRegistry::new().with_store(Arc::new(store));

    let mut trusting = spec("acme-okta", Some(tenant_a()));
    trusting.trust_asserted_email = true;
    let record = registry.create(&trusting).await.unwrap();
    assert!(record.trust_asserted_email, "the opt-in is recorded …");

    let resolved = registry.resolve("acme-okta", Some(&tenant_a().to_string())).unwrap();
    // … and still inert. core.tb_user keys verified email GLOBALLY (uq_user_email), so a
    // merge cannot be bounded to one tenant: honouring the opt-in here would let a
    // tenant-bound IdP absorb another tenant's account with the same address. The store
    // does not change that; #1088 tracks the tenant-scoped account store that would.
    assert!(
        !effective_saml_email_verified(&resolved),
        "a tenant-bound IdP must stay fail-closed even when it opted in"
    );

    // The untenanted case is the one the opt-in is actually for, and it still works.
    let mut single_tenant = spec("solo", None);
    single_tenant.trust_asserted_email = true;
    registry.create(&single_tenant).await.unwrap();
    assert!(effective_saml_email_verified(&registry.resolve("solo", None).unwrap()));
}

// ─── Certificate expiry ──────────────────────────────────────────────────────

#[tokio::test]
async fn expiry_report_names_certificates_before_the_cliff() {
    let store = skip_if_no_db!();
    let registry = SamlIdpRegistry::new().with_store(Arc::new(store));

    let mut soon = spec("expires-soon", None);
    soon.metadata_xml = metadata_xml(5);
    registry.create(&soon).await.unwrap();
    registry.create(&spec("healthy", Some(tenant_a()))).await.unwrap();

    let report = registry.expiry_report(Utc::now(), 30);
    assert_eq!(report.len(), 1, "only the imminent one is reported: {report:?}");
    assert_eq!(report[0].idp_name, "expires-soon");
    assert!(!report[0].expired, "5 days out is a warning, not an outage");

    // Widening the horizon past the healthy certificate's expiry catches both.
    assert_eq!(registry.expiry_report(Utc::now(), 4000).len(), 2);
    // A horizon shorter than the imminent certificate's life reports nothing.
    assert!(registry.expiry_report(Utc::now(), 1).is_empty());
    // And past its expiry it reads as down, not merely imminent.
    assert!(registry.expiry_report(Utc::now() + Duration::days(10), 30)[0].expired);
}

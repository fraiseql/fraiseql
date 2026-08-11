//! IdP resolution: config-file IdPs, stored IdPs, and the tenant scoping between them (#947).
//!
//! [`SamlIdpRegistry`] is the single seam `/auth/saml/login` and the ACS resolve through. It
//! composes two sources:
//!
//! - **`[saml.idps.*]`** — resolved once at boot, immutable, the single-tenant path. Still fully
//!   supported; the store is an addition, not a replacement.
//! - **[`SamlIdpStore`]** — durable, per-tenant, hot-reloaded, managed over the admin API.
//!
//! # Tenant scoping is a match, not a filter
//!
//! Before #947 the tenant binding constrained only what an assertion could *link* to; any
//! caller could start a login with any configured IdP name. [`SamlIdpRegistry::resolve`]
//! makes the binding load-bearing at the front door with one symmetric rule:
//!
//! > the tenant named by the request must **equal** the tenant bound to the IdP, where
//! > "absent" is a value that equals only itself.
//!
//! So an untenanted IdP serves an untenanted request and refuses a tenant-qualified one,
//! and a tenant-bound IdP serves only its own tenant. Both directions matter: treating
//! "untenanted" as "belongs to whoever asked" would hand every tenant the deployment-wide
//! IdP, which is the same hole from the other side.
//!
//! A refusal is indistinguishable from an unknown name (both resolve to `None`, both
//! answer `404`) so the route cannot be used to enumerate other tenants' IdP names.

use std::{collections::HashMap, sync::Arc, time::Duration};

use arc_swap::ArcSwap;
use chrono::{DateTime, Utc};

use super::{
    SamlError, SamlIdpConfig,
    store::{SamlIdpRecord, SamlIdpSpec, SamlIdpStore},
};

/// How long after a write the periodic refresher takes to notice a change made by *another*
/// replica. Writes through this registry refresh immediately, so this bounds only
/// cross-replica propagation.
pub const DEFAULT_REFRESH_INTERVAL: Duration = Duration::from_secs(30);

/// How far ahead of a certificate's expiry the registry starts warning.
pub const DEFAULT_EXPIRY_WARNING_DAYS: i64 = 30;

/// Where a resolved IdP came from — config file or store. Reported by the admin API so an
/// operator can tell a hot-reloadable IdP from one that needs a config deploy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdpSource {
    /// Declared in `[saml.idps.<name>]`; changes need a restart.
    ConfigFile,
    /// Stored in `core.tb_saml_idp`; hot-reloadable.
    Store,
}

/// A certificate whose expiry an operator needs to act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertExpiryWarning {
    /// Logical IdP name.
    pub idp_name:   String,
    /// Tenant binding, if any.
    pub tenant_id:  Option<String>,
    /// When the earliest signing certificate expires.
    pub expires_at: DateTime<Utc>,
    /// Whether SSO is already broken or about to be.
    pub expired:    bool,
}

/// This SP's own key material, applied to every IdP the registry serves (#948).
///
/// Deployment-level rather than per-IdP: the key pair is this deployment's identity, so
/// stored IdPs get request signing and assertion decryption without private keys ever
/// living in a database row.
#[derive(Clone, Default)]
pub struct SpKeyMaterial {
    /// Current private key (PEM or DER).
    pub private_key:          Vec<u8>,
    /// Current certificate (PEM or DER), published in SP metadata.
    pub certificate:          Vec<u8>,
    /// Whether to sign outbound `AuthnRequest`s.
    pub sign_authn_requests:  bool,
    /// Previous key pair, accepted for decryption only during a rotation window.
    pub previous_private_key: Option<Vec<u8>>,
    /// Previous certificate, published for encryption during the same window.
    pub previous_certificate: Option<Vec<u8>>,
}

impl std::fmt::Debug for SpKeyMaterial {
    /// Hand-written: a derived `Debug` would print private key bytes into any log line.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpKeyMaterial")
            .field("sign_authn_requests", &self.sign_authn_requests)
            .field("has_previous_key", &self.previous_private_key.is_some())
            .finish_non_exhaustive()
    }
}

/// Registered IdPs from both sources, with tenant-scoped resolution.
#[derive(Clone)]
pub struct SamlIdpRegistry {
    /// `[saml.idps.*]`, resolved at boot. Authoritative on a name collision.
    config_idps: Arc<HashMap<String, Arc<SamlIdpConfig>>>,
    /// Durable store, when one is configured.
    store:       Option<Arc<dyn SamlIdpStore>>,
    /// Last successful projection of the store. Swapped wholesale, never partially updated,
    /// so a reader always sees one coherent generation.
    cached:      Arc<ArcSwap<HashMap<String, Arc<SamlIdpConfig>>>>,
    /// SP key material applied to stored IdPs as they are built (#948).
    sp_keys:     Option<Arc<SpKeyMaterial>>,
}

impl std::fmt::Debug for SamlIdpRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SamlIdpRegistry")
            .field("config_idps", &self.config_idps.keys().collect::<Vec<_>>())
            .field("has_store", &self.store.is_some())
            .field("stored", &self.cached.load().keys().cloned().collect::<Vec<_>>())
            .field("sp_keys", &self.sp_keys)
            .finish()
    }
}

impl Default for SamlIdpRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Normalize a tenant identifier for comparison: absent and blank are the same thing.
///
/// Comparison is exact (after trimming) rather than case-insensitive on purpose. Store
/// tenants are UUIDs rendered canonically, so exactness costs nothing there; config-file
/// tenants are free-form strings, where folding case would merge two operator-declared
/// tenants that differ only in case into one.
fn normalize_tenant(tenant: Option<&str>) -> Option<&str> {
    tenant.map(str::trim).filter(|t| !t.is_empty())
}

impl SamlIdpRegistry {
    /// An empty registry: no config IdPs, no store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            config_idps: Arc::new(HashMap::new()),
            store:       None,
            cached:      Arc::new(ArcSwap::from_pointee(HashMap::new())),
            sp_keys:     None,
        }
    }

    /// Apply this SP's key material to every **stored** IdP the registry builds (#948).
    ///
    /// Config-file IdPs receive it at construction instead, where their own builder runs.
    #[must_use]
    pub fn with_sp_keys(mut self, sp_keys: SpKeyMaterial) -> Self {
        self.sp_keys = Some(Arc::new(sp_keys));
        self
    }

    /// Register a config-file IdP. Builder-style; last write wins, as configuration is
    /// operator-controlled and `[saml.idps.<name>]` keys are unique by construction.
    #[must_use]
    pub fn with_config_idp(mut self, idp: SamlIdpConfig) -> Self {
        let idps = Arc::make_mut(&mut self.config_idps);
        idps.insert(idp.idp_name.clone(), Arc::new(idp));
        self
    }

    /// Attach a durable store. Call [`refresh`](Self::refresh) to populate the cache.
    #[must_use]
    pub fn with_store(mut self, store: Arc<dyn SamlIdpStore>) -> Self {
        self.store = Some(store);
        self
    }

    /// Whether a durable store is attached.
    #[must_use]
    pub fn has_store(&self) -> bool {
        self.store.is_some()
    }

    /// Resolve an IdP for a caller claiming `tenant`.
    ///
    /// Returns `None` both for an unknown name and for a tenant that does not match — see
    /// the module docs for why those two are deliberately indistinguishable.
    #[must_use]
    pub fn resolve(&self, idp_name: &str, tenant: Option<&str>) -> Option<Arc<SamlIdpConfig>> {
        let idp = self
            .config_idps
            .get(idp_name)
            .cloned()
            .or_else(|| self.cached.load().get(idp_name).cloned())?;

        (normalize_tenant(idp.tenant_id.as_deref()) == normalize_tenant(tenant)).then_some(idp)
    }

    /// Names of every registered IdP, both sources, sorted. For introspection and tests —
    /// never for authorization, which must go through [`resolve`](Self::resolve).
    #[must_use]
    pub fn idp_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .config_idps
            .keys()
            .cloned()
            .chain(self.cached.load().keys().cloned())
            .collect();
        names.sort_unstable();
        names.dedup();
        names
    }

    /// Where a registered IdP came from, or `None` if the name is unknown.
    #[must_use]
    pub fn source_of(&self, idp_name: &str) -> Option<IdpSource> {
        if self.config_idps.contains_key(idp_name) {
            return Some(IdpSource::ConfigFile);
        }
        self.cached.load().contains_key(idp_name).then_some(IdpSource::Store)
    }

    /// Reload the store into the cache. Idempotent, and safe to call concurrently: the new
    /// generation is built fully before it is swapped in.
    ///
    /// A stored row whose name collides with a config-file IdP is **not** served and is
    /// logged as an error: the two would share one `saml:<name>` identity namespace, and
    /// resolving the ambiguity silently in either direction is how one tenant's users end
    /// up on another's accounts.
    ///
    /// # Errors
    ///
    /// [`SamlError::Store`] if the store cannot be read. The previous generation keeps
    /// serving — a database blip must not log everyone out of SSO.
    pub async fn refresh(&self) -> Result<(), SamlError> {
        let Some(store) = &self.store else {
            return Ok(());
        };
        let records = store.list().await?;

        let mut next: HashMap<String, Arc<SamlIdpConfig>> = HashMap::with_capacity(records.len());
        for record in records {
            if self.config_idps.contains_key(&record.idp_name) {
                tracing::error!(
                    idp = %record.idp_name,
                    "stored SAML IdP shadows a [saml.idps.*] entry of the same name and will \
                     NOT be served: both would share the saml:<name> account namespace. \
                     Rename the stored IdP or remove the config-file entry."
                );
                continue;
            }
            match config_from_record(&record, self.sp_keys.as_deref()) {
                Ok(config) => {
                    next.insert(record.idp_name.clone(), Arc::new(config));
                },
                Err(e) => {
                    // Writes validate through the same builder, so this is a row that was
                    // edited outside the API — refuse it rather than serve a broken login.
                    tracing::error!(
                        idp = %record.idp_name, error = %e,
                        "stored SAML IdP failed to build and will NOT be served"
                    );
                },
            }
        }
        self.cached.store(Arc::new(next));
        Ok(())
    }

    /// Create a stored IdP, then refresh so it serves immediately.
    ///
    /// # Errors
    ///
    /// [`SamlError::Store`] if no store is attached; [`SamlError::NameTaken`] if a
    /// config-file IdP or another stored IdP (live or deleted) already owns the name;
    /// otherwise whatever [`SamlIdpStore::create`] returns.
    pub async fn create(&self, spec: &SamlIdpSpec) -> Result<SamlIdpRecord, SamlError> {
        let store = self.require_store()?;
        if self.config_idps.contains_key(&spec.idp_name) {
            return Err(SamlError::NameTaken(spec.idp_name.clone()));
        }
        let record = store.create(spec).await?;
        self.refresh().await?;
        Ok(record)
    }

    /// Update a stored IdP, then refresh so the change serves immediately.
    ///
    /// # Errors
    ///
    /// [`SamlError::Store`] if no store is attached; otherwise whatever
    /// [`SamlIdpStore::update`] returns.
    pub async fn update(&self, spec: &SamlIdpSpec) -> Result<SamlIdpRecord, SamlError> {
        let record = self.require_store()?.update(spec).await?;
        self.refresh().await?;
        Ok(record)
    }

    /// Tombstone a stored IdP, then refresh so it stops serving immediately.
    ///
    /// # Errors
    ///
    /// [`SamlError::Store`] if no store is attached; otherwise whatever
    /// [`SamlIdpStore::delete`] returns.
    pub async fn delete(&self, idp_name: &str) -> Result<(), SamlError> {
        self.require_store()?.delete(idp_name).await?;
        self.refresh().await
    }

    /// Every live stored IdP.
    ///
    /// # Errors
    ///
    /// [`SamlError::Store`] if no store is attached, or the read fails.
    pub async fn list_stored(&self) -> Result<Vec<SamlIdpRecord>, SamlError> {
        self.require_store()?.list().await
    }

    /// One live stored IdP by name.
    ///
    /// # Errors
    ///
    /// [`SamlError::Store`] if no store is attached, or the read fails.
    pub async fn get_stored(&self, idp_name: &str) -> Result<Option<SamlIdpRecord>, SamlError> {
        self.require_store()?.get(idp_name).await
    }

    fn require_store(&self) -> Result<&Arc<dyn SamlIdpStore>, SamlError> {
        self.store.as_ref().ok_or_else(|| {
            SamlError::Store(
                "no SAML IdP store is configured — set [saml] store_enabled = true on a \
                 deployment with a database pool"
                    .to_string(),
            )
        })
    }

    /// Certificates that are expired or expire within `threshold_days` of `now`.
    ///
    /// Pure over the currently-registered IdPs so it can be asserted on directly and served
    /// by the admin API; the periodic refresher logs whatever it returns.
    #[must_use]
    pub fn expiry_report(&self, now: DateTime<Utc>, threshold_days: i64) -> Vec<CertExpiryWarning> {
        let horizon = now + chrono::Duration::days(threshold_days);
        let stored = self.cached.load();
        let mut warnings: Vec<CertExpiryWarning> = self
            .config_idps
            .values()
            .chain(stored.values())
            .filter_map(|idp| {
                let expires_at = idp.signing_certificate_expiry()?;
                (expires_at <= horizon).then(|| CertExpiryWarning {
                    idp_name: idp.idp_name.clone(),
                    tenant_id: idp.tenant_id.clone(),
                    expires_at,
                    expired: expires_at <= now,
                })
            })
            .collect();
        warnings.sort_by(|a, b| a.expires_at.cmp(&b.expires_at).then(a.idp_name.cmp(&b.idp_name)));
        warnings
    }

    /// Log the current [`expiry_report`](Self::expiry_report) — expired certificates at
    /// `error`, imminent ones at `warn`.
    pub fn log_expiry_report(&self, threshold_days: i64) {
        for warning in self.expiry_report(Utc::now(), threshold_days) {
            if warning.expired {
                tracing::error!(
                    idp = %warning.idp_name, expired_at = %warning.expires_at,
                    "SAML IdP signing certificate has EXPIRED — SSO for this IdP is down \
                     until the operator loads fresh metadata"
                );
            } else {
                tracing::warn!(
                    idp = %warning.idp_name, expires_at = %warning.expires_at,
                    "SAML IdP signing certificate expires soon — load fresh metadata before \
                     the cliff"
                );
            }
        }
    }

    /// The background refresher: reload the store every `interval`, then log the expiry
    /// report. Never returns; the caller owns the task (the server spawns it into its
    /// `JoinSet` so shutdown reaps it).
    ///
    /// Writes through this registry already refresh synchronously, so this exists to pick up
    /// changes made by *other* replicas and to keep the expiry warning current.
    pub async fn refresh_loop(self, interval: Duration, warning_days: i64) {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            if let Err(e) = self.refresh().await {
                // Keep serving the last good generation: a database blip must not log
                // every SSO user out.
                tracing::error!(
                    error = %e,
                    "SAML IdP refresh failed; continuing to serve the last good generation"
                );
            }
            self.log_expiry_report(warning_days);
        }
    }
}

/// Build a servable config from a stored row, through the same builder boot uses.
fn config_from_record(
    record: &SamlIdpRecord,
    sp_keys: Option<&SpKeyMaterial>,
) -> Result<SamlIdpConfig, SamlError> {
    let mut builder = SamlIdpConfig::builder(
        record.idp_name.clone(),
        record.sp_entity_id.clone(),
        record.acs_url.clone(),
    )
    .idp_metadata_xml(&record.metadata_xml)?
    .tenant_id(record.tenant_id.map(|t| t.to_string()))
    .trust_asserted_email(record.trust_asserted_email);

    if let Some(keys) = sp_keys {
        builder = builder
            .sp_key_pair(&keys.private_key, &keys.certificate)?
            .sign_authn_requests(keys.sign_authn_requests);
        if let (Some(key), Some(cert)) =
            (keys.previous_private_key.as_deref(), keys.previous_certificate.as_deref())
        {
            builder = builder.sp_previous_key_pair(key, cert)?;
        }
    }
    builder.build()
}

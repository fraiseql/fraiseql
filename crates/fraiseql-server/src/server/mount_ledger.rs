//! The record of every router the server mounts, and what admits a caller on it.
//!
//! Every router `build_router` merges or nests into the application goes through
//! [`MountLedger::merge`] or [`MountLedger::nest`], which record a [`MountRecord`]
//! before mounting. The transport refusal matrix reads the ledger of a real server and
//! fails on a transport it has no row for, so a new transport cannot be mounted without
//! saying how it refuses. `tools/check-mount-ledger.py` keeps every mount site in
//! `server/routing` and `server/capabilities` going through these two functions.

use std::sync::{Arc, Mutex, PoisonError};

use axum::Router;

/// What admits a caller on a mounted router.
// Reason: a variant's only mount site may sit behind a Cargo feature (MCP, inbound
// webhooks, the auth routes), so a build without it constructs no such record.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Credential {
    /// The deployment's auth layer: a bearer token or a service account, then admission.
    AuthLayer,
    /// The admin bearer token (and, where accepted, a tenant administrator's token).
    AdminToken,
    /// The MCP endpoint's own credential gate.
    McpGate,
    /// The storage API's admission: its own token, else the auth layer's credentials.
    StorageAdmission,
    /// A message signature or a provider secret, not a caller credential.
    Signature,
    /// The routes that issue credentials: login, callback, token and session flows.
    Issuance,
    /// No credential: the route serves nothing a credential would scope.
    Public,
    /// A capability's routes, whose own mounts are recorded where they are made.
    Capability,
}

/// One mount: the transport's name and what admits a caller on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MountRecord {
    /// The transport's name, as the refusal matrix keys its rows.
    pub transport:  &'static str,
    /// What admits a caller.
    pub credential: Credential,
}

// Reason: as on `Credential`, a constructor's only caller may be feature-gated.
#[allow(dead_code)]
impl MountRecord {
    /// A mount of `transport`, admitted by `credential`.
    #[must_use]
    pub const fn new(transport: &'static str, credential: Credential) -> Self {
        Self {
            transport,
            credential,
        }
    }

    /// A mount of `transport` behind the deployment's auth layer.
    #[must_use]
    pub const fn auth_layer(transport: &'static str) -> Self {
        Self::new(transport, Credential::AuthLayer)
    }

    /// A mount of `transport` behind the admin bearer token.
    #[must_use]
    pub const fn admin_token(transport: &'static str) -> Self {
        Self::new(transport, Credential::AdminToken)
    }

    /// A mount of `transport` behind the MCP credential gate.
    #[must_use]
    pub const fn mcp_gate(transport: &'static str) -> Self {
        Self::new(transport, Credential::McpGate)
    }

    /// A mount of `transport` behind storage admission.
    #[must_use]
    pub const fn storage_admission(transport: &'static str) -> Self {
        Self::new(transport, Credential::StorageAdmission)
    }

    /// A mount of `transport` checked by a message signature.
    #[must_use]
    pub const fn signature(transport: &'static str) -> Self {
        Self::new(transport, Credential::Signature)
    }

    /// A mount of `transport` that issues credentials.
    #[must_use]
    pub const fn issuance(transport: &'static str) -> Self {
        Self::new(transport, Credential::Issuance)
    }

    /// A mount of `transport` that takes no credential.
    #[must_use]
    pub const fn public(transport: &'static str) -> Self {
        Self::new(transport, Credential::Public)
    }

    /// A mount of `transport` of a capability's routes.
    #[must_use]
    pub const fn capability(transport: &'static str) -> Self {
        Self::new(transport, Credential::Capability)
    }
}

/// The mounts made so far, shared by every clone.
#[derive(Debug, Clone, Default)]
pub struct MountLedger(Arc<Mutex<Vec<MountRecord>>>);

impl MountLedger {
    /// Record `mount`, then merge `router` into `app`.
    pub fn merge(&self, app: Router, mount: MountRecord, router: Router) -> Router {
        self.record(mount);
        app.merge(router)
    }

    /// Record `mount`, then nest `router` into `app` under `path`.
    pub fn nest(&self, app: Router, mount: MountRecord, path: &str, router: Router) -> Router {
        self.record(mount);
        app.nest(path, router)
    }

    /// Every mount recorded so far, in the order it was made.
    #[cfg(any(test, feature = "testing"))]
    #[must_use]
    pub fn records(&self) -> Vec<MountRecord> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    fn record(&self, mount: MountRecord) {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).push(mount);
    }
}

impl super::Server {
    /// The ledger of the mounts this server makes, filled when its router is built.
    #[cfg(feature = "testing")]
    #[doc(hidden)]
    #[must_use]
    pub fn mount_ledger(&self) -> MountLedger {
        self.mounts.clone()
    }
}

#[cfg(test)]
#[path = "mount_ledger_tests.rs"]
mod tests;

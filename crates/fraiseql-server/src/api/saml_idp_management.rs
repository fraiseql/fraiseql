//! SAML `IdP` management API (#947) — CRUD over `core.tb_saml_idp`.
//!
//! Mounted when `[saml] store_enabled = true` and an `admin_token` is set, behind the same
//! admin bearer gate as the RBAC and API-key management routers. Every write goes through
//! the live [`SamlIdpRegistry`](fraiseql_auth::saml::SamlIdpRegistry), so a change serves
//! (or stops serving) on the next request without a restart, and the metadata is validated
//! by the same builder boot uses.
//!
//! # On "scoped so a tenant admin manages only their own `IdPs`"
//!
//! The admin bearer token carries no tenant identity — it is one deployment-wide
//! credential, exactly as for `/api/roles` — so this router cannot derive a caller's tenant
//! and does not pretend to. The tenant is named explicitly on each request and `GET
//! /api/saml/idps?tenant_id=…` filters by it. True per-tenant delegated administration
//! needs a tenant-scoped admin principal, which is a new authorization model rather than a
//! parameter; #1089 carries it. Naming this limitation is the point: a router that silently
//! accepted a `tenant_id` it never enforced would be the accepted-and-unconsumed shape.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use fraiseql_auth::saml::{SamlError, SamlIdpRecord, SamlIdpRegistry, SamlIdpSpec};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Shared state: the registry the SAML routes resolve through, so a write here is visible
/// to `/auth/saml/login` immediately.
#[derive(Clone)]
pub struct SamlIdpManagementState {
    /// The live registry.
    pub registry: SamlIdpRegistry,
}

/// One `IdP` as the API reports it. The metadata XML is echoed back so an operator can
/// diff what is stored against what the `IdP` publishes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamlIdpDto {
    /// Surrogate row identifier.
    pub id: Uuid,
    /// Logical `IdP` name — globally unique, and the `saml:<name>` provider namespace.
    pub idp_name: String,
    /// Tenant binding (`null` = untenanted).
    pub tenant_id: Option<Uuid>,
    /// SP entity ID.
    pub sp_entity_id: String,
    /// Assertion Consumer Service URL.
    pub acs_url: String,
    /// The `IdP` metadata XML as stored.
    pub metadata_xml: String,
    /// `IdP` entity ID, parsed from the metadata.
    pub idp_entity_id: String,
    /// Email-linking opt-in as stored.
    pub trust_asserted_email: bool,
    /// Whether the opt-in is actually honoured. `false` for every tenant-bound `IdP` while
    /// the account store keys verified email globally — reported so an operator is never
    /// left believing a recorded flag is in effect (#1088).
    pub email_linking_effective: bool,
    /// Earliest signing-certificate expiry, parsed from the metadata.
    pub certificate_expires_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Row creation time.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last update time.
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<SamlIdpRecord> for SamlIdpDto {
    fn from(r: SamlIdpRecord) -> Self {
        // Mirrors `effective_saml_email_verified`: opted in AND provably bounded to one
        // tenant, which the global-email account store can only guarantee when unbound.
        let email_linking_effective = r.trust_asserted_email && r.tenant_id.is_none();
        Self {
            id: r.id,
            idp_name: r.idp_name,
            tenant_id: r.tenant_id,
            sp_entity_id: r.sp_entity_id,
            acs_url: r.acs_url,
            metadata_xml: r.metadata_xml,
            idp_entity_id: r.idp_entity_id,
            trust_asserted_email: r.trust_asserted_email,
            email_linking_effective,
            certificate_expires_at: r.certificate_expires_at,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

/// `POST /api/saml/idps` body.
///
/// `deny_unknown_fields` is deliberate: a misspelled `tenantId` that serde silently ignored
/// would create a **deployment-wide** `IdP` while the operator believed it was tenant-bound —
/// the same silent-widening class `CreateRoleRequest` guards against, applied to a
/// credential boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateSamlIdpRequest {
    /// Logical `IdP` name.
    pub idp_name:             String,
    /// Tenant binding. Omit for an untenanted (single-tenant) `IdP`.
    #[serde(default)]
    pub tenant_id:            Option<Uuid>,
    /// SP entity ID.
    pub sp_entity_id:         String,
    /// Assertion Consumer Service URL.
    pub acs_url:              String,
    /// `IdP` metadata XML.
    pub metadata_xml:         String,
    /// Email-linking opt-in (default off).
    #[serde(default)]
    pub trust_asserted_email: bool,
}

/// `PUT /api/saml/idps/{idp_name}` body.
///
/// Neither the name nor the tenant appears: both are identity, not settings. Rebinding
/// either would silently rehome every account already linked under `saml:<name>`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateSamlIdpRequest {
    /// SP entity ID.
    pub sp_entity_id:         String,
    /// Assertion Consumer Service URL.
    pub acs_url:              String,
    /// Replacement `IdP` metadata XML — this is the certificate-rotation path.
    pub metadata_xml:         String,
    /// Email-linking opt-in.
    #[serde(default)]
    pub trust_asserted_email: bool,
}

/// `GET /api/saml/idps` query.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListQuery {
    /// Report only `IdPs` bound to this tenant. Omit for all of them.
    #[serde(default)]
    pub tenant_id: Option<Uuid>,
}

fn json_error(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

/// Map a store error onto a status. `NameTaken` is a `409` because it is a genuine
/// conflict an operator resolves by choosing another name — including when the holder is a
/// deleted `IdP`, whose namespace is never reissued.
fn store_error(e: &SamlError) -> Response {
    match e {
        SamlError::NotFound(_) => json_error(StatusCode::NOT_FOUND, &e.to_string()),
        SamlError::NameTaken(_) => json_error(StatusCode::CONFLICT, &e.to_string()),
        SamlError::Config(_) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
        _ => {
            tracing::error!(error = %e, "SAML IdP management operation failed");
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "SAML IdP store error")
        },
    }
}

/// Build the SAML `IdP` management router.
///
/// Routes (all behind the admin bearer token):
/// - `POST   /api/saml/idps`             — create an `IdP`
/// - `GET    /api/saml/idps`             — list `IdPs`, optionally filtered by `tenant_id`
/// - `GET    /api/saml/idps/{idp_name}`  — one `IdP`
/// - `PUT    /api/saml/idps/{idp_name}`  — replace metadata / policy (certificate rotation)
/// - `DELETE /api/saml/idps/{idp_name}`  — stop serving it; the name stays reserved forever
pub fn saml_idp_management_router(state: SamlIdpManagementState) -> Router {
    Router::new()
        .route("/api/saml/idps", axum::routing::post(create_idp).get(list_idps))
        .route("/api/saml/idps/{idp_name}", get(get_idp).put(update_idp).delete(delete_idp))
        .with_state(Arc::new(state))
}

async fn create_idp(
    State(state): State<Arc<SamlIdpManagementState>>,
    Json(payload): Json<CreateSamlIdpRequest>,
) -> Response {
    let spec = SamlIdpSpec {
        idp_name:             payload.idp_name,
        tenant_id:            payload.tenant_id,
        sp_entity_id:         payload.sp_entity_id,
        acs_url:              payload.acs_url,
        metadata_xml:         payload.metadata_xml,
        trust_asserted_email: payload.trust_asserted_email,
    };
    match state.registry.create(&spec).await {
        Ok(record) => (StatusCode::CREATED, Json(SamlIdpDto::from(record))).into_response(),
        Err(e) => store_error(&e),
    }
}

async fn list_idps(
    State(state): State<Arc<SamlIdpManagementState>>,
    Query(q): Query<ListQuery>,
) -> Response {
    match state.registry.list_stored().await {
        Ok(records) => {
            let idps: Vec<SamlIdpDto> = records
                .into_iter()
                .filter(|r| q.tenant_id.is_none_or(|t| r.tenant_id == Some(t)))
                .map(SamlIdpDto::from)
                .collect();
            Json(serde_json::json!({ "total": idps.len(), "idps": idps })).into_response()
        },
        Err(e) => store_error(&e),
    }
}

async fn get_idp(
    State(state): State<Arc<SamlIdpManagementState>>,
    Path(idp_name): Path<String>,
) -> Response {
    match state.registry.get_stored(&idp_name).await {
        Ok(Some(record)) => Json(SamlIdpDto::from(record)).into_response(),
        Ok(None) => json_error(StatusCode::NOT_FOUND, "no such SAML IdP"),
        Err(e) => store_error(&e),
    }
}

async fn update_idp(
    State(state): State<Arc<SamlIdpManagementState>>,
    Path(idp_name): Path<String>,
    Json(payload): Json<UpdateSamlIdpRequest>,
) -> Response {
    // The tenant is not in the body and must not be changed by an update, so it is read
    // back from the stored row rather than taken from the caller.
    let existing = match state.registry.get_stored(&idp_name).await {
        Ok(Some(record)) => record,
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "no such SAML IdP"),
        Err(e) => return store_error(&e),
    };
    let spec = SamlIdpSpec {
        idp_name,
        tenant_id: existing.tenant_id,
        sp_entity_id: payload.sp_entity_id,
        acs_url: payload.acs_url,
        metadata_xml: payload.metadata_xml,
        trust_asserted_email: payload.trust_asserted_email,
    };
    match state.registry.update(&spec).await {
        Ok(record) => Json(SamlIdpDto::from(record)).into_response(),
        Err(e) => store_error(&e),
    }
}

async fn delete_idp(
    State(state): State<Arc<SamlIdpManagementState>>,
    Path(idp_name): Path<String>,
) -> Response {
    match state.registry.delete(&idp_name).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => store_error(&e),
    }
}

#[cfg(test)]
mod tests;

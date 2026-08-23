//! SCIM 2.0 provisioning endpoints (#946).
//!
//! `/scim/v2/Users`, `/scim/v2/Groups` and the discovery trio, behind a **provisioning**
//! bearer token that is deliberately not the admin token.
//!
//! # The half that is security, not integration
//!
//! Before this, an offboarded employee's FraiseQL account stayed active. SAML stopped them
//! signing in *through the `IdP`*; a local password, a social link or an API key on the same
//! account kept working. So `active = false` here does two things, and the tests that matter
//! assert both:
//!
//! 1. every existing session is revoked, so access ends now rather than when a refresh token
//!    happens to expire; and
//! 2. new sessions are refused at
//!    [`PostgresSessionStore::create_session`](fraiseql_auth::PostgresSessionStore) — the one point
//!    every credential path converges on.
//!
//! # Groups grant membership, never permission
//!
//! A SCIM group is mirrored onto an RBAC role and its members onto role assignments, which
//! is what group-driven access needs. Creating a group creates a role with **no
//! permissions**: a provisioning credential that could grant permissions would be an admin
//! credential wearing a different name, which is the distinction this module exists to keep.

mod filter;
mod resources;

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::get,
};
use fraiseql_auth::{
    SessionStore,
    scim::{PgScimStore, PgScimTokenStore, ScimPrincipal, ScimStore as _, ScimUserWrite},
};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

pub use self::resources::SCIM_CONTENT_TYPE;
use self::{
    filter::{expect_attribute, parse_eq},
    resources::{
        GROUP_SCHEMA, GroupBody, PATCH_OP_SCHEMA, PatchBody, USER_SCHEMA, UserBody, error_response,
        etag, group_to_json, list_response, project, resource_types, schemas,
        service_provider_config, user_to_json,
    },
};
use crate::api::rbac_management::db_backend::RbacDbBackend;

/// Largest page a client may request. Mirrors `filter.maxResults` in
/// `/ServiceProviderConfig`, so what the discovery document promises is what the server does.
const MAX_COUNT: i64 = 200;
/// Page size when the client names none.
const DEFAULT_COUNT: i64 = 100;

/// Shared state for the SCIM router.
#[derive(Clone)]
pub struct ScimState {
    /// Pool the per-request, tenant-scoped store is built from.
    pub pool:          sqlx::PgPool,
    /// Provisioning-credential store.
    pub tokens:        Arc<PgScimTokenStore>,
    /// Session backend — deactivation revokes through it.
    pub session_store: Arc<dyn SessionStore>,
    /// RBAC backend that SCIM groups are mirrored onto.
    pub rbac:          Arc<RbacDbBackend>,
    /// Externally reachable base URL of the SCIM surface, used for `meta.location`.
    pub base_url:      String,
}

impl std::fmt::Debug for ScimState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScimState")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

/// A SCIM error response carrying the SCIM media type.
fn scim_error(status: StatusCode, detail: &str, scim_type: Option<&str>) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, SCIM_CONTENT_TYPE)],
        Json(error_response(status.as_u16(), detail, scim_type)),
    )
        .into_response()
}

/// A SCIM success response carrying the media type and, when the resource has a version, an
/// `ETag` a client can send back in `If-Match`.
fn scim_json(status: StatusCode, body: Value, version: Option<i64>) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, header::HeaderValue::from_static(SCIM_CONTENT_TYPE));
    if let Some(version) = version {
        if let Ok(value) = header::HeaderValue::from_str(&etag(version)) {
            headers.insert(header::ETAG, value);
        }
    }
    (status, headers, Json(body)).into_response()
}

/// Map an auth-store error onto its SCIM status.
fn store_error(e: &fraiseql_auth::AuthError) -> Response {
    use fraiseql_auth::AuthError;
    match e {
        AuthError::TokenNotFound => scim_error(StatusCode::NOT_FOUND, "Resource not found", None),
        // The store signals a unique-constraint collision this way; SCIM names it
        // `uniqueness`, and a provisioning client branches on that to reconcile.
        AuthError::EmailAlreadyRegistered => scim_error(
            StatusCode::CONFLICT,
            "A resource with this userName or displayName already exists",
            Some("uniqueness"),
        ),
        _ => {
            tracing::error!(error = %e, "SCIM store operation failed");
            scim_error(StatusCode::INTERNAL_SERVER_ERROR, "Provisioning store error", None)
        },
    }
}

/// Authenticate the provisioning bearer token and attach the principal to the request.
///
/// The tenant comes from the credential and nothing else, so an `IdP` cannot provision into a
/// tenant it was not issued for — there is no request field that could say otherwise.
async fn scim_auth_middleware(
    State(state): State<ScimState>,
    mut request: axum::extract::Request,
    next: Next,
) -> Response {
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|raw| raw.split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
        .map(|(_, token)| token.trim().to_string());

    let Some(token) = token else {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
            Json(error_response(401, "Provisioning bearer token required", None)),
        )
            .into_response();
    };

    match state.tokens.authenticate(&token).await {
        Ok(principal) => {
            request.extensions_mut().insert(principal);
            next.run(request).await
        },
        Err(e) => {
            tracing::warn!(error = %e, "SCIM provisioning token rejected");
            (
                StatusCode::UNAUTHORIZED,
                [(header::WWW_AUTHENTICATE, "Bearer")],
                Json(error_response(401, "Invalid provisioning token", None)),
            )
                .into_response()
        },
    }
}

/// Build the SCIM router. Every route sits behind the provisioning-token gate.
pub fn scim_router(state: ScimState) -> Router {
    Router::new()
        .route("/scim/v2/Users", get(list_users).post(create_user))
        .route(
            "/scim/v2/Users/{id}",
            get(get_user).put(replace_user).patch(patch_user).delete(delete_user),
        )
        .route("/scim/v2/Groups", get(list_groups).post(create_group))
        .route(
            "/scim/v2/Groups/{id}",
            get(get_group).put(replace_group).patch(patch_group).delete(delete_group),
        )
        .route("/scim/v2/ServiceProviderConfig", get(get_service_provider_config))
        .route("/scim/v2/ResourceTypes", get(get_resource_types))
        .route("/scim/v2/ResourceTypes/{id}", get(get_resource_type))
        .route("/scim/v2/Schemas", get(get_schemas))
        .route("/scim/v2/Schemas/{id}", get(get_schema))
        // RFC 7644 §3.4.3: the POST-with-a-body form of a query, for clients whose filters
        // are too long or too sensitive for a URL.
        .route("/scim/v2/Users/.search", axum::routing::post(search_users))
        .route("/scim/v2/Groups/.search", axum::routing::post(search_groups))
        // The root form searches across every resource type at once.
        .route("/scim/v2/.search", axum::routing::post(search_all))
        // An unknown path under the SCIM surface must still answer with a SCIM error body
        // and media type; axum's default 404 carries neither, and a strict client reports
        // "unexpected content type" rather than "not found".
        .fallback(scim_not_found)
        // A matched path with an unmatched method otherwise answers 405 with an EMPTY body,
        // which a strict client reports as "unexpected response content format" rather than
        // "method not allowed".
        .method_not_allowed_fallback(scim_method_not_allowed)
        .route_layer(axum::middleware::from_fn_with_state(state.clone(), scim_auth_middleware))
        .with_state(state)
}

/// SCIM-shaped `405` for a known path reached with the wrong method.
async fn scim_method_not_allowed() -> Response {
    scim_error(
        StatusCode::METHOD_NOT_ALLOWED,
        "That method is not supported on this SCIM endpoint",
        None,
    )
}

/// SCIM-shaped `404` for any unrouted path under the surface.
async fn scim_not_found() -> Response {
    scim_error(StatusCode::NOT_FOUND, "No such SCIM endpoint", None)
}

/// The tenant-scoped store for this request's principal.
fn store_for(state: &ScimState, principal: &ScimPrincipal) -> PgScimStore {
    PgScimStore::new(state.pool.clone(), principal.tenant_id)
}

/// Pagination + filter query parameters.
#[derive(Debug, Deserialize)]
struct ListQuery {
    #[serde(default)]
    filter:      Option<String>,
    #[serde(default, rename = "startIndex")]
    start_index: Option<i64>,
    #[serde(default)]
    count:       Option<i64>,
    /// RFC 7644 §3.9 — return only these attributes.
    #[serde(default)]
    attributes:  Option<String>,
    /// RFC 7644 §3.9 — return everything but these.
    #[serde(default, rename = "excludedAttributes")]
    excluded:    Option<String>,
}

impl ListQuery {
    /// SCIM `startIndex` is 1-based, and a value below 1 is clamped to 1 by RFC 7644 §3.4.2.4.
    const fn start(&self) -> i64 {
        match self.start_index {
            Some(i) if i > 1 => i,
            _ => 1,
        }
    }

    /// `count` is clamped to `MAX_COUNT`; a negative count means zero, per the RFC.
    const fn count(&self) -> i64 {
        match self.count {
            Some(c) if c < 0 => 0,
            Some(c) if c > MAX_COUNT => MAX_COUNT,
            Some(c) => c,
            None => DEFAULT_COUNT,
        }
    }
}

/// A `SearchRequest` body (RFC 7644 §3.4.3) — the same parameters as the query string.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchBody {
    #[serde(default)]
    filter:              Option<String>,
    #[serde(default)]
    start_index:         Option<i64>,
    #[serde(default)]
    count:               Option<i64>,
    #[serde(default, deserialize_with = "attribute_list")]
    attributes:          Option<String>,
    #[serde(default, deserialize_with = "attribute_list")]
    excluded_attributes: Option<String>,
}

/// Read `attributes` / `excludedAttributes` in either spelling RFC 7644 gives them.
///
/// §3.9 defines them as one comma-separated **string** in the query parameter; §3.4.3's
/// `SearchRequest` body carries the same information as a multi-valued **array**. They are the
/// same field with two wire types, and a conformant client sends the array when it posts to
/// `/.search`. Reading only the string form made every such request fail deserialization, so
/// axum answered `422 text/plain` — which a SCIM client reports as "unexpected response
/// content format", indistinguishable from an empty body (#1090).
///
/// Both are accepted and normalised to the comma-separated form `project` consumes. Accepting
/// the string in a body is a superset, not a second bug: clients do send it, and refusing it
/// would turn a request that works today into a new failure.
fn attribute_list<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrList {
        String(String),
        List(Vec<String>),
    }

    Ok(Option::<StringOrList>::deserialize(deserializer)?.map(|value| match value {
        StringOrList::String(one) => one,
        StringOrList::List(many) => many.join(","),
    }))
}

impl From<SearchBody> for ListQuery {
    fn from(body: SearchBody) -> Self {
        Self {
            filter:      body.filter,
            start_index: body.start_index,
            count:       body.count,
            attributes:  body.attributes,
            excluded:    body.excluded_attributes,
        }
    }
}

async fn search_users(
    state: State<ScimState>,
    principal: axum::Extension<ScimPrincipal>,
    Json(body): Json<SearchBody>,
) -> Response {
    list_users(state, principal, Query(body.into())).await
}

async fn search_groups(
    state: State<ScimState>,
    principal: axum::Extension<ScimPrincipal>,
    Json(body): Json<SearchBody>,
) -> Response {
    list_groups(state, principal, Query(body.into())).await
}

/// Root `.search`: every resource type in one `ListResponse`.
///
/// A filter is refused rather than applied to only one type — answering a `userName`
/// filter with every group as well would be the silent-widening shape the filter parser
/// exists to prevent.
async fn search_all(
    State(state): State<ScimState>,
    axum::Extension(principal): axum::Extension<ScimPrincipal>,
    Json(body): Json<SearchBody>,
) -> Response {
    let q: ListQuery = body.into();
    if q.filter.is_some() {
        return scim_error(
            StatusCode::BAD_REQUEST,
            "a filter on the root .search is not supported; POST to /Users/.search or \
             /Groups/.search instead",
            Some("invalidFilter"),
        );
    }

    let store = store_for(&state, &principal);
    let users = match store.list_users(None, q.start(), q.count()).await {
        Ok(page) => page,
        Err(e) => return store_error(&e),
    };
    let groups = match store.list_groups(None, q.start(), q.count()).await {
        Ok(page) => page,
        Err(e) => return store_error(&e),
    };

    let mut resources: Vec<Value> =
        Vec::with_capacity(users.resources.len() + groups.resources.len());
    for user in &users.resources {
        let user_groups = store.groups_of_user(&user.id).await.unwrap_or_default();
        resources.push(project(
            user_to_json(user, &state.base_url, &user_groups),
            q.attributes.as_deref(),
            q.excluded.as_deref(),
        ));
    }
    for group in &groups.resources {
        resources.push(project(
            group_to_json(group, &state.base_url),
            q.attributes.as_deref(),
            q.excluded.as_deref(),
        ));
    }
    let total = users.total_results + groups.total_results;
    scim_json(StatusCode::OK, list_response(&resources, total, q.start()), None)
}

// ─── Users ───────────────────────────────────────────────────────────────────

async fn list_users(
    State(state): State<ScimState>,
    axum::Extension(principal): axum::Extension<ScimPrincipal>,
    Query(q): Query<ListQuery>,
) -> Response {
    let user_name = match q.filter.as_deref() {
        None => None,
        Some(raw) => match parse_eq(raw).and_then(|f| expect_attribute(&f, "userName")) {
            Ok(value) => Some(value),
            Err(e) => return scim_error(StatusCode::BAD_REQUEST, &e.0, Some("invalidFilter")),
        },
    };

    let store = store_for(&state, &principal);
    let page = match store.list_users(user_name.as_deref(), q.start(), q.count()).await {
        Ok(page) => page,
        Err(e) => return store_error(&e),
    };

    let mut resources = Vec::with_capacity(page.resources.len());
    for user in &page.resources {
        let groups = store.groups_of_user(&user.id).await.unwrap_or_default();
        resources.push(project(
            user_to_json(user, &state.base_url, &groups),
            q.attributes.as_deref(),
            q.excluded.as_deref(),
        ));
    }
    scim_json(StatusCode::OK, list_response(&resources, page.total_results, q.start()), None)
}

async fn get_user(
    State(state): State<ScimState>,
    axum::Extension(principal): axum::Extension<ScimPrincipal>,
    Path(id): Path<String>,
    Query(q): Query<ListQuery>,
) -> Response {
    let store = store_for(&state, &principal);
    match store.get_user(&id).await {
        Ok(Some(user)) => {
            let groups = store.groups_of_user(&user.id).await.unwrap_or_default();
            scim_json(
                StatusCode::OK,
                project(
                    user_to_json(&user, &state.base_url, &groups),
                    q.attributes.as_deref(),
                    q.excluded.as_deref(),
                ),
                Some(user.version),
            )
        },
        Ok(None) => scim_error(StatusCode::NOT_FOUND, "User not found", None),
        Err(e) => store_error(&e),
    }
}

async fn create_user(
    State(state): State<ScimState>,
    axum::Extension(principal): axum::Extension<ScimPrincipal>,
    Json(body): Json<UserBody>,
) -> Response {
    let Some(user_name) = body.user_name.clone().filter(|u| !u.trim().is_empty()) else {
        return scim_error(StatusCode::BAD_REQUEST, "userName is required", Some("invalidValue"));
    };

    let write = ScimUserWrite {
        user_name,
        external_id: body.external_id.clone(),
        email: body.primary_email(),
        given_name: body.name.as_ref().and_then(|n| n.given_name.clone()),
        family_name: body.name.as_ref().and_then(|n| n.family_name.clone()),
        display_name: body.display_name.clone(),
        // RFC 7643: a created user is active unless the client says otherwise.
        active: body.active.unwrap_or(true),
    };

    let store = store_for(&state, &principal);
    match store.create_user(&write).await {
        Ok(user) => scim_json(
            StatusCode::CREATED,
            user_to_json(&user, &state.base_url, &[]),
            Some(user.version),
        ),
        Err(e) => store_error(&e),
    }
}

async fn replace_user(
    State(state): State<ScimState>,
    axum::Extension(principal): axum::Extension<ScimPrincipal>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<UserBody>,
) -> Response {
    let store = store_for(&state, &principal);
    let Some(existing) = (match store.get_user(&id).await {
        Ok(user) => user,
        Err(e) => return store_error(&e),
    }) else {
        return scim_error(StatusCode::NOT_FOUND, "User not found", None);
    };
    if let Some(response) = precondition_failed(&headers, existing.version) {
        return response;
    }

    let Some(user_name) = body.user_name.clone().filter(|u| !u.trim().is_empty()) else {
        return scim_error(StatusCode::BAD_REQUEST, "userName is required", Some("invalidValue"));
    };
    let write = ScimUserWrite {
        user_name,
        external_id: body.external_id.clone(),
        email: body.primary_email(),
        given_name: body.name.as_ref().and_then(|n| n.given_name.clone()),
        family_name: body.name.as_ref().and_then(|n| n.family_name.clone()),
        display_name: body.display_name.clone(),
        active: body.active.unwrap_or(true),
    };

    match store.replace_user(&id, &write).await {
        Ok(user) => {
            // A PUT that flips `active` to false is an offboarding just as much as a PATCH.
            if existing.active && !user.active {
                revoke_sessions(&state, &user.id).await;
            }
            let groups = store.groups_of_user(&user.id).await.unwrap_or_default();
            scim_json(
                StatusCode::OK,
                user_to_json(&user, &state.base_url, &groups),
                Some(user.version),
            )
        },
        Err(e) => store_error(&e),
    }
}

async fn patch_user(
    State(state): State<ScimState>,
    axum::Extension(principal): axum::Extension<ScimPrincipal>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<PatchBody>,
) -> Response {
    if let Some(response) = wrong_patch_schema(&body) {
        return response;
    }
    let store = store_for(&state, &principal);
    let Some(existing) = (match store.get_user(&id).await {
        Ok(user) => user,
        Err(e) => return store_error(&e),
    }) else {
        return scim_error(StatusCode::NOT_FOUND, "User not found", None);
    };
    if let Some(response) = precondition_failed(&headers, existing.version) {
        return response;
    }

    // Apply the operations onto the current resource, then write it back. RFC 7644 §3.5.2
    // allows PATCH on any mutable attribute, and a client that is refused one it may
    // legitimately send simply cannot provision — the third-party conformance run is what
    // surfaced that the earlier `active`-only handling was too narrow.
    let mut write = ScimUserWrite {
        user_name:    existing.user_name.clone(),
        external_id:  existing.external_id.clone(),
        email:        existing.email.clone(),
        given_name:   existing.given_name.clone(),
        family_name:  existing.family_name.clone(),
        display_name: existing.display_name.clone(),
        active:       existing.active,
    };
    for op in &body.operations {
        if let Err(detail) = apply_user_op(&mut write, op) {
            return scim_error(StatusCode::BAD_REQUEST, &detail, Some("invalidPath"));
        }
    }

    match store.replace_user(&id, &write).await {
        Ok(user) => {
            if existing.active && !user.active {
                revoke_sessions(&state, &user.id).await;
            }
            let groups = store.groups_of_user(&user.id).await.unwrap_or_default();
            scim_json(
                StatusCode::OK,
                user_to_json(&user, &state.base_url, &groups),
                Some(user.version),
            )
        },
        Err(e) => store_error(&e),
    }
}

/// Refuse a `PatchOp` body that does not declare the `PatchOp` schema.
///
/// RFC 7644 §3.5.2 requires it, and a body without it is usually a client that meant to send
/// a `PUT`. Accepting it would apply partial-update semantics to what the caller believed was
/// a full replace.
fn wrong_patch_schema(body: &PatchBody) -> Option<Response> {
    if body.schemas.iter().any(|s| s == PATCH_OP_SCHEMA) {
        return None;
    }
    Some(scim_error(
        StatusCode::BAD_REQUEST,
        &format!("a PatchOp body must declare the \"{PATCH_OP_SCHEMA}\" schema"),
        Some("invalidSyntax"),
    ))
}

/// Apply one `PatchOp` operation to a pending user write.
///
/// Both shapes clients send are handled: a targeted `{"path": "active", "value": false}` and
/// an untargeted `{"value": {"active": false, "displayName": "…"}}`. `remove` clears the
/// named attribute rather than being refused, because that is how a client unsets one.
///
/// An unknown path is an error, never a silent no-op: telling a provisioning client a change
/// landed when it did not is the drift this whole surface exists to prevent.
fn apply_user_op(write: &mut ScimUserWrite, op: &resources::PatchOperation) -> Result<(), String> {
    let removing = op.op.eq_ignore_ascii_case("remove");
    if let Some(path) = op.path.as_deref().map(str::trim) {
        let value = if removing { None } else { op.value.as_ref() };
        set_user_attribute(write, path, value)
    } else {
        let Some(object) = op.value.as_ref().and_then(Value::as_object) else {
            return Err("a `PatchOp` without a path needs an object value".to_string());
        };
        for (key, value) in object {
            set_user_attribute(write, key, Some(value))?;
        }
        Ok(())
    }
}

/// Set (or, with `value = None`, clear) one user attribute named by a SCIM path.
fn set_user_attribute(
    write: &mut ScimUserWrite,
    path: &str,
    value: Option<&Value>,
) -> Result<(), String> {
    /// A string attribute: present-and-string sets it, absent clears it.
    fn text(value: Option<&Value>, field: &str) -> Result<Option<String>, String> {
        match value {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(_) => Err(format!("'{field}' requires a string value")),
        }
    }

    match path.to_ascii_lowercase().as_str() {
        "active" => {
            write.active = match value {
                None | Some(Value::Null) => true,
                Some(Value::Bool(b)) => *b,
                Some(_) => return Err("'active' requires a boolean value".to_string()),
            };
        },
        "username" => {
            write.user_name =
                text(value, "userName")?.ok_or("userName may not be removed".to_string())?;
        },
        "externalid" => write.external_id = text(value, "externalId")?,
        "displayname" => write.display_name = text(value, "displayName")?,
        "name.givenname" => write.given_name = text(value, "name.givenName")?,
        "name.familyname" => write.family_name = text(value, "name.familyName")?,
        "name" => {
            let object = value.and_then(Value::as_object);
            write.given_name = object
                .and_then(|o| o.get("givenName"))
                .and_then(Value::as_str)
                .map(str::to_string);
            write.family_name = object
                .and_then(|o| o.get("familyName"))
                .and_then(Value::as_str)
                .map(str::to_string);
        },
        "emails" => {
            // Multi-valued: take the primary, else the first, else clear.
            write.email = value
                .and_then(Value::as_array)
                .and_then(|entries| {
                    entries
                        .iter()
                        .find(|e| e.get("primary").and_then(Value::as_bool) == Some(true))
                        .or_else(|| entries.first())
                })
                .and_then(|e| e.get("value"))
                .and_then(Value::as_str)
                .map(str::to_string);
        },
        other => {
            return Err(format!(
                "unsupported PatchOp path '{other}' on User: supported paths are active, \
                 userName, externalId, displayName, name, name.givenName, name.familyName, emails"
            ));
        },
    }
    Ok(())
}

async fn delete_user(
    State(state): State<ScimState>,
    axum::Extension(principal): axum::Extension<ScimPrincipal>,
    Path(id): Path<String>,
) -> Response {
    let store = store_for(&state, &principal);
    match store.delete_user(&id).await {
        Ok(()) => {
            // A deleted user's live sessions would otherwise outlive the account itself.
            revoke_sessions(&state, &id).await;
            StatusCode::NO_CONTENT.into_response()
        },
        Err(e) => store_error(&e),
    }
}

/// Revoke every session for `user_id`, logging rather than failing the provisioning call.
///
/// The write already landed: reporting `500` would tell the `IdP` the deactivation failed and
/// invite a retry loop, when in fact the account is deactivated and new sessions are already
/// refused at creation. The revoke is what makes it immediate, so a failure is loud in the
/// logs and the operator can force it, but it is not a reason to un-tell the `IdP`.
async fn revoke_sessions(state: &ScimState, user_id: &str) {
    if let Err(e) = state.session_store.revoke_all_sessions(user_id).await {
        tracing::error!(
            user_id = %user_id, error = %e,
            "SCIM deactivation could not revoke existing sessions; the account is deactivated \
             and new sessions are refused, but live refresh tokens may survive until expiry"
        );
    } else {
        tracing::info!(user_id = %user_id, "SCIM deactivation revoked all sessions");
    }
}

// ─── Groups ──────────────────────────────────────────────────────────────────

async fn list_groups(
    State(state): State<ScimState>,
    axum::Extension(principal): axum::Extension<ScimPrincipal>,
    Query(q): Query<ListQuery>,
) -> Response {
    let display_name = match q.filter.as_deref() {
        None => None,
        Some(raw) => match parse_eq(raw).and_then(|f| expect_attribute(&f, "displayName")) {
            Ok(value) => Some(value),
            Err(e) => return scim_error(StatusCode::BAD_REQUEST, &e.0, Some("invalidFilter")),
        },
    };

    let store = store_for(&state, &principal);
    match store.list_groups(display_name.as_deref(), q.start(), q.count()).await {
        Ok(page) => {
            let resources = page
                .resources
                .iter()
                .map(|g| {
                    project(
                        group_to_json(g, &state.base_url),
                        q.attributes.as_deref(),
                        q.excluded.as_deref(),
                    )
                })
                .collect::<Vec<_>>();
            scim_json(
                StatusCode::OK,
                list_response(&resources, page.total_results, q.start()),
                None,
            )
        },
        Err(e) => store_error(&e),
    }
}

async fn get_group(
    State(state): State<ScimState>,
    axum::Extension(principal): axum::Extension<ScimPrincipal>,
    Path(id): Path<String>,
    Query(q): Query<ListQuery>,
) -> Response {
    let Ok(uuid) = Uuid::parse_str(&id) else {
        return scim_error(StatusCode::NOT_FOUND, "Group not found", None);
    };
    match store_for(&state, &principal).get_group(uuid).await {
        Ok(Some(group)) => scim_json(
            StatusCode::OK,
            project(
                group_to_json(&group, &state.base_url),
                q.attributes.as_deref(),
                q.excluded.as_deref(),
            ),
            Some(group.version),
        ),
        Ok(None) => scim_error(StatusCode::NOT_FOUND, "Group not found", None),
        Err(e) => store_error(&e),
    }
}

async fn create_group(
    State(state): State<ScimState>,
    axum::Extension(principal): axum::Extension<ScimPrincipal>,
    Json(body): Json<GroupBody>,
) -> Response {
    let Some(display_name) = body.display_name.clone().filter(|d| !d.trim().is_empty()) else {
        return scim_error(
            StatusCode::BAD_REQUEST,
            "displayName is required",
            Some("invalidValue"),
        );
    };
    let members = body.member_ids();
    let store = store_for(&state, &principal);
    match store.create_group(&display_name, body.external_id.as_deref(), &members).await {
        Ok(group) => {
            mirror_group_to_rbac(&state, &principal, &group.display_name, &members, &[]).await;
            scim_json(
                StatusCode::CREATED,
                group_to_json(&group, &state.base_url),
                Some(group.version),
            )
        },
        Err(e) => store_error(&e),
    }
}

async fn replace_group(
    State(state): State<ScimState>,
    axum::Extension(principal): axum::Extension<ScimPrincipal>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<GroupBody>,
) -> Response {
    let Ok(uuid) = Uuid::parse_str(&id) else {
        return scim_error(StatusCode::NOT_FOUND, "Group not found", None);
    };
    let store = store_for(&state, &principal);
    let Some(existing) = (match store.get_group(uuid).await {
        Ok(group) => group,
        Err(e) => return store_error(&e),
    }) else {
        return scim_error(StatusCode::NOT_FOUND, "Group not found", None);
    };
    if let Some(response) = precondition_failed(&headers, existing.version) {
        return response;
    }

    let Some(display_name) = body.display_name.clone().filter(|d| !d.trim().is_empty()) else {
        return scim_error(
            StatusCode::BAD_REQUEST,
            "displayName is required",
            Some("invalidValue"),
        );
    };
    let members = body.member_ids();
    match store
        .replace_group(uuid, &display_name, body.external_id.as_deref(), &members)
        .await
    {
        Ok(group) => {
            let removed: Vec<String> =
                existing.members.iter().filter(|m| !members.contains(m)).cloned().collect();
            mirror_group_to_rbac(&state, &principal, &group.display_name, &members, &removed).await;
            scim_json(StatusCode::OK, group_to_json(&group, &state.base_url), Some(group.version))
        },
        Err(e) => store_error(&e),
    }
}

async fn patch_group(
    State(state): State<ScimState>,
    axum::Extension(principal): axum::Extension<ScimPrincipal>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<PatchBody>,
) -> Response {
    if let Some(response) = wrong_patch_schema(&body) {
        return response;
    }
    let Ok(uuid) = Uuid::parse_str(&id) else {
        return scim_error(StatusCode::NOT_FOUND, "Group not found", None);
    };
    let store = store_for(&state, &principal);
    let Some(existing) = (match store.get_group(uuid).await {
        Ok(group) => group,
        Err(e) => return store_error(&e),
    }) else {
        return scim_error(StatusCode::NOT_FOUND, "Group not found", None);
    };
    if let Some(response) = precondition_failed(&headers, existing.version) {
        return response;
    }

    let mut add: Vec<String> = Vec::new();
    let mut remove: Vec<String> = Vec::new();
    let mut display_name = existing.display_name.clone();
    let mut external_id = existing.external_id.clone();
    let mut membership_touched = false;

    for op in &body.operations {
        let removing = op.op.eq_ignore_ascii_case("remove");
        let replacing = op.op.eq_ignore_ascii_case("replace");
        if !removing && !replacing && !op.op.eq_ignore_ascii_case("add") {
            return scim_error(
                StatusCode::BAD_REQUEST,
                &format!("unsupported PatchOp op '{}'", op.op),
                Some("invalidSyntax"),
            );
        }

        // An untargeted op carries an object of attributes, exactly as on User.
        let Some(raw_path) = op.path.as_deref().map(str::trim) else {
            let Some(object) = op.value.as_ref().and_then(Value::as_object) else {
                return scim_error(
                    StatusCode::BAD_REQUEST,
                    "a PatchOp without a path needs an object value",
                    Some("invalidValue"),
                );
            };
            for (key, value) in object {
                match key.to_ascii_lowercase().as_str() {
                    "displayname" => {
                        display_name = value.as_str().map_or_else(String::new, str::to_string);
                    },
                    "externalid" => external_id = value.as_str().map(str::to_string),
                    "members" => {
                        membership_touched = true;
                        remove.extend(existing.members.iter().cloned());
                        add.extend(member_ids_from(Some(value)));
                    },
                    other => {
                        return scim_error(
                            StatusCode::BAD_REQUEST,
                            &format!("unsupported PatchOp path '{other}' on Group"),
                            Some("invalidPath"),
                        );
                    },
                }
            }
            continue;
        };

        let lowered = raw_path.to_ascii_lowercase();
        // `members[value eq "x"]` targets one member; take the id out of the value filter
        // rather than pretending the whole attribute was addressed.
        let targeted = lowered.strip_prefix("members[").and_then(|rest| {
            rest.strip_suffix(']').and_then(|inner| parse_eq(inner).ok().map(|f| f.value))
        });

        match lowered.as_str() {
            "displayname" => {
                if removing {
                    return scim_error(
                        StatusCode::BAD_REQUEST,
                        "displayName is required and may not be removed",
                        Some("invalidValue"),
                    );
                }
                let Some(value) = op.value.as_ref().and_then(Value::as_str) else {
                    return scim_error(
                        StatusCode::BAD_REQUEST,
                        "'displayName' requires a string value",
                        Some("invalidValue"),
                    );
                };
                display_name = value.to_string();
            },
            "externalid" => {
                external_id = if removing {
                    None
                } else {
                    op.value.as_ref().and_then(Value::as_str).map(str::to_string)
                };
            },
            path if path.starts_with("members") => {
                membership_touched = true;
                let ids =
                    targeted.map_or_else(|| member_ids_from(op.value.as_ref()), |id| vec![id]);
                if removing {
                    // `remove` with no target clears the whole membership.
                    if ids.is_empty() {
                        remove.extend(existing.members.iter().cloned());
                    } else {
                        remove.extend(ids);
                    }
                } else {
                    if replacing {
                        remove.extend(existing.members.iter().cloned());
                    }
                    add.extend(ids);
                }
            },
            other => {
                return scim_error(
                    StatusCode::BAD_REQUEST,
                    &format!(
                        "unsupported PatchOp path '{other}' on Group: supported paths are \
                         displayName, externalId and members"
                    ),
                    Some("invalidPath"),
                );
            },
        }
    }

    // A name or externalId change is a replace; membership alone is the cheaper path.
    if display_name != existing.display_name || external_id != existing.external_id {
        let members: Vec<String> = if membership_touched {
            let mut next: Vec<String> =
                existing.members.iter().filter(|m| !remove.contains(m)).cloned().collect();
            for id in &add {
                if !next.contains(id) {
                    next.push(id.clone());
                }
            }
            next
        } else {
            existing.members.clone()
        };
        return match store
            .replace_group(uuid, &display_name, external_id.as_deref(), &members)
            .await
        {
            Ok(group) => {
                mirror_group_to_rbac(&state, &principal, &group.display_name, &add, &remove).await;
                scim_json(
                    StatusCode::OK,
                    group_to_json(&group, &state.base_url),
                    Some(group.version),
                )
            },
            Err(e) => store_error(&e),
        };
    }

    match store.patch_group_members(uuid, &add, &remove).await {
        Ok(group) => {
            mirror_group_to_rbac(&state, &principal, &group.display_name, &add, &remove).await;
            scim_json(StatusCode::OK, group_to_json(&group, &state.base_url), Some(group.version))
        },
        Err(e) => store_error(&e),
    }
}

async fn delete_group(
    State(state): State<ScimState>,
    axum::Extension(principal): axum::Extension<ScimPrincipal>,
    Path(id): Path<String>,
) -> Response {
    let Ok(uuid) = Uuid::parse_str(&id) else {
        return scim_error(StatusCode::NOT_FOUND, "Group not found", None);
    };
    let store = store_for(&state, &principal);
    let existing = store.get_group(uuid).await.ok().flatten();
    match store.delete_group(uuid).await {
        Ok(()) => {
            if let Some(group) = existing {
                mirror_group_to_rbac(&state, &principal, &group.display_name, &[], &group.members)
                    .await;
            }
            StatusCode::NO_CONTENT.into_response()
        },
        Err(e) => store_error(&e),
    }
}

/// Member ids inside a `members` patch value, which is an array of `{ "value": … }`.
fn member_ids_from(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|e| e.get("value").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Mirror group membership onto RBAC role assignments.
///
/// The role is created if absent, **with no permissions**: a provisioning credential must be
/// able to put someone in a role, never to decide what that role may do. A FraiseQL admin
/// grants the permissions through `/api/roles`, and the `IdP` decides who is in it.
///
/// Failures are logged, not surfaced: the SCIM write already succeeded, and reporting `500`
/// would make an `IdP` retry a provisioning call whose primary effect landed.
async fn mirror_group_to_rbac(
    state: &ScimState,
    principal: &ScimPrincipal,
    display_name: &str,
    add: &[String],
    remove: &[String],
) {
    let tenant = principal.tenant_id.map(|t| t.to_string());
    let Some(role_id) = ensure_role(state, display_name, tenant.as_deref()).await else {
        return;
    };
    for user_id in add {
        if let Err(e) = state.rbac.assign_role_to_user(user_id, &role_id, tenant.as_deref()).await {
            tracing::error!(
                user_id = %user_id, group = %display_name, error = %e,
                "SCIM group membership could not be mirrored onto the RBAC role"
            );
        }
    }
    for user_id in remove {
        if let Err(e) = state.rbac.revoke_role_from_user(user_id, &role_id).await {
            tracing::error!(
                user_id = %user_id, group = %display_name, error = %e,
                "SCIM group removal could not be mirrored onto the RBAC role"
            );
        }
    }
}

/// The RBAC role id for a SCIM group, creating a permission-less role if it does not exist.
async fn ensure_role(
    state: &ScimState,
    display_name: &str,
    tenant: Option<&str>,
) -> Option<String> {
    match state.rbac.list_roles(tenant, 1000, 0).await {
        Ok(page) => {
            if let Some(role) = page.items.iter().find(|r| r.name == display_name) {
                return Some(role.id.clone());
            }
        },
        Err(e) => {
            tracing::error!(error = %e, "could not list RBAC roles for SCIM group mirroring");
            return None;
        },
    }
    match state
        .rbac
        .create_role(
            display_name,
            Some("Created by SCIM group provisioning; permissions are granted by an admin"),
            Vec::new(),
            tenant,
        )
        .await
    {
        Ok(role) => Some(role.id),
        Err(e) => {
            tracing::error!(
                group = %display_name, error = %e,
                "could not create the RBAC role mirroring a SCIM group"
            );
            None
        },
    }
}

// ─── Concurrency ─────────────────────────────────────────────────────────────

/// Enforce `If-Match` against the stored version, returning a `412` response when it does
/// not match.
///
/// This is what stops a lost update: two provisioning workers reconciling the same user
/// concurrently would otherwise have the second silently overwrite the first.
fn precondition_failed(headers: &HeaderMap, version: i64) -> Option<Response> {
    let if_match = headers.get(header::IF_MATCH)?.to_str().ok()?.trim().to_string();
    if if_match == "*" {
        return None;
    }
    // A client may send several candidates, and may or may not use the weak marker.
    let current = etag(version);
    let matches = if_match.split(',').map(str::trim).any(|candidate| {
        candidate == current
            || candidate.trim_start_matches("W/") == current.trim_start_matches("W/")
    });
    if matches {
        return None;
    }
    Some(scim_error(
        StatusCode::PRECONDITION_FAILED,
        "The resource has changed since the version named in If-Match",
        None,
    ))
}

// ─── Discovery ───────────────────────────────────────────────────────────────

async fn get_service_provider_config(State(state): State<ScimState>) -> Response {
    scim_json(StatusCode::OK, service_provider_config(&state.base_url), None)
}

async fn get_resource_types(State(state): State<ScimState>) -> Response {
    let types = resource_types(&state.base_url);
    let total = i64::try_from(types.len()).unwrap_or(i64::MAX);
    scim_json(StatusCode::OK, list_response(&types, total, 1), None)
}

async fn get_resource_type(State(state): State<ScimState>, Path(id): Path<String>) -> Response {
    resource_types(&state.base_url)
        .into_iter()
        .find(|t| t.get("id").and_then(Value::as_str) == Some(id.as_str()))
        .map_or_else(
            || scim_error(StatusCode::NOT_FOUND, "ResourceType not found", None),
            |t| scim_json(StatusCode::OK, t, None),
        )
}

async fn get_schemas(State(state): State<ScimState>) -> Response {
    let all = schemas(&state.base_url);
    let total = i64::try_from(all.len()).unwrap_or(i64::MAX);
    scim_json(StatusCode::OK, list_response(&all, total, 1), None)
}

async fn get_schema(State(state): State<ScimState>, Path(id): Path<String>) -> Response {
    let wanted = match id.as_str() {
        "User" => USER_SCHEMA,
        "Group" => GROUP_SCHEMA,
        other => other,
    };
    schemas(&state.base_url)
        .into_iter()
        .find(|s| s.get("id").and_then(Value::as_str) == Some(wanted))
        .map_or_else(
            || scim_error(StatusCode::NOT_FOUND, "Schema not found", None),
            |s| scim_json(StatusCode::OK, s, None),
        )
}

/// Health-style probe used by the mount test to confirm the router is reachable.
#[must_use]
pub fn scim_base_paths() -> Vec<&'static str> {
    vec![
        "/scim/v2/Users",
        "/scim/v2/Groups",
        "/scim/v2/ServiceProviderConfig",
        "/scim/v2/ResourceTypes",
        "/scim/v2/Schemas",
    ]
}

/// Re-exported for the management API, which mints provisioning tokens.
pub use fraiseql_auth::scim::MintedScimToken;

#[cfg(test)]
mod tests;

// ─── Provisioning-credential management (admin) ──────────────────────────────

/// State for the admin-side token endpoints.
#[derive(Clone)]
pub struct ScimTokenManagementState {
    /// Provisioning-credential store.
    pub tokens: Arc<PgScimTokenStore>,
}

impl std::fmt::Debug for ScimTokenManagementState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScimTokenManagementState").finish_non_exhaustive()
    }
}

/// `POST /api/scim/tokens` body.
///
/// `deny_unknown_fields` for the same reason as the RBAC and `IdP` bodies: a misspelled
/// `tenantId` that serde ignored would mint a **deployment-wide** provisioning credential
/// while the operator believed it was scoped to one tenant.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct MintScimTokenRequest {
    /// The `IdP` this credential provisions for.
    pub idp_name:    String,
    /// Tenant the credential is confined to. Omit for an untenanted deployment.
    #[serde(default)]
    pub tenant_id:   Option<Uuid>,
    /// Operator note.
    #[serde(default)]
    pub description: Option<String>,
}

/// Build the admin router that mints and revokes provisioning credentials.
///
/// Mounted behind the admin bearer token, deliberately apart from the SCIM surface itself:
/// the credential that *creates* provisioning credentials is an admin credential, and the
/// one an `IdP` holds is not.
pub fn scim_token_management_router(state: ScimTokenManagementState) -> Router {
    Router::new()
        .route("/api/scim/tokens", axum::routing::post(mint_token).get(list_tokens))
        .route("/api/scim/tokens/{id}", axum::routing::delete(revoke_token))
        .with_state(Arc::new(state))
}

async fn mint_token(
    State(state): State<Arc<ScimTokenManagementState>>,
    Json(payload): Json<MintScimTokenRequest>,
) -> Response {
    match state
        .tokens
        .mint(&payload.idp_name, payload.tenant_id, payload.description.as_deref())
        .await
    {
        Ok(minted) => (
            StatusCode::CREATED,
            Json(json!({
                "id":          minted.record.id,
                "idp_name":    minted.record.idp_name,
                "tenant_id":   minted.record.tenant_id,
                "description": minted.record.description,
                "created_at":  minted.record.created_at,
                // Shown exactly once: only sha256(token) is stored, so this response is the
                // only place the credential exists in plaintext.
                "token":       minted.token,
            })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "could not mint a SCIM provisioning token");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "could not mint provisioning token" })),
            )
                .into_response()
        },
    }
}

async fn list_tokens(State(state): State<Arc<ScimTokenManagementState>>) -> Response {
    match state.tokens.list().await {
        Ok(records) => Json(json!({
            "total": records.len(),
            "tokens": records.iter().map(|r| json!({
                "id":           r.id,
                "idp_name":     r.idp_name,
                "tenant_id":    r.tenant_id,
                "description":  r.description,
                "created_at":   r.created_at,
                "last_used_at": r.last_used_at,
            })).collect::<Vec<_>>(),
        }))
        .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "could not list SCIM provisioning tokens");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "could not list provisioning tokens" })),
            )
                .into_response()
        },
    }
}

async fn revoke_token(
    State(state): State<Arc<ScimTokenManagementState>>,
    Path(id): Path<String>,
) -> Response {
    let Ok(uuid) = Uuid::parse_str(&id) else {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "no such token" }))).into_response();
    };
    match state.tokens.revoke(uuid).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(fraiseql_auth::AuthError::TokenNotFound) => {
            (StatusCode::NOT_FOUND, Json(json!({ "error": "no such token" }))).into_response()
        },
        Err(e) => {
            tracing::error!(error = %e, "could not revoke a SCIM provisioning token");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "could not revoke provisioning token" })),
            )
                .into_response()
        },
    }
}

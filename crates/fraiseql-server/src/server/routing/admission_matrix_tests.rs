//! Every mounted transport refuses the same hostile requests.
//!
//! The matrix is generated from the mounted surface, not written from memory: the
//! [`mount_scan`](super::mount_scan) finds every function that adds routes to a router,
//! [`MOUNTS`] classifies each one as the [`Transport`] it serves, and each transport's
//! cells are an exhaustive `match`. A new mount function fails the scan until it is
//! classified; a new transport fails to compile until its row is written.

#![allow(clippy::panic)] // Reason: test module — a scan or table mismatch must fail loudly

use std::collections::{BTreeMap, BTreeSet};

use super::mount_scan::{self, MountFn};

/// A surface the server serves, with one admission rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Transport {
    /// `/graphql` over POST, GET and QUERY, including its `@stream` SSE responses.
    GraphQl,
    /// `/operations/v1`: a GraphQL operation submitted now, read later.
    AsyncOperations,
    /// `/ws`: GraphQL subscriptions.
    Subscriptions,
    /// `/rest/v1`.
    Rest,
    /// The MCP tool endpoint.
    Mcp,
    /// `/storage/v1`.
    Storage,
    /// Platform administration behind the static admin bearer token.
    AdminBearer,
    /// Administration that also accepts a tenant administrator's credential, scoped to
    /// that tenant.
    AdminPrincipal,
    /// Administration behind an OIDC token carrying the admin scope.
    AdminOidc,
    /// The SQL console: the admin bearer token or an OIDC admin.
    AdminDual,
    /// SCIM 2.0 provisioning, behind a provisioning bearer token.
    Scim,
    /// Introspection and the schema exports, authenticated when configured to be.
    Introspection,
    /// The design-audit API, authenticated when configured to be.
    DesignAudit,
    /// Prometheus and JSON metrics, behind the admin bearer token.
    Metrics,
    /// The GraphQL playground and the Studio shell: static pages.
    Playground,
    /// Sign-in flows and the issuer's public documents: these mint credentials or publish
    /// what verifies them, so they serve anonymous callers.
    AuthFlows,
    /// `/auth/me` and token revocation: the caller's own session.
    AuthSession,
    /// Inbound webhooks, authenticated by the sender's signature.
    InboundWebhook,
    /// `/health`, `/health/live`, `/health/ready`.
    Health,
}

/// What a mount function contributes.
#[derive(Debug, Clone, Copy)]
enum Role {
    /// It serves routes of these transports itself.
    Serves(&'static [Transport]),
    /// It only merges routers built by other classified functions, or wraps them in
    /// layers; it adds no route of its own (checked).
    Composes,
}

/// Every mount function the scan finds, keyed `crate/src/path.rs::fn`.
const MOUNTS: &[(&str, Role)] = &[
    // fraiseql-auth
    (
        "fraiseql-auth/src/local_password/routes/mod.rs::email_verification_routes",
        Role::Serves(&[Transport::AuthFlows]),
    ),
    (
        "fraiseql-auth/src/local_password/routes/mod.rs::local_password_routes",
        Role::Serves(&[Transport::AuthFlows]),
    ),
    (
        "fraiseql-auth/src/saml/handler.rs::saml_routes",
        Role::Serves(&[Transport::AuthFlows]),
    ),
    (
        "fraiseql-auth/src/session_routes.rs::session_routes",
        Role::Serves(&[Transport::AuthFlows]),
    ),
    // fraiseql-storage
    (
        "fraiseql-storage/src/routes/mod.rs::render_routes",
        Role::Serves(&[Transport::Storage]),
    ),
    (
        "fraiseql-storage/src/routes/mod.rs::storage_router",
        Role::Serves(&[Transport::Storage]),
    ),
    // fraiseql-server: transport routers
    (
        "fraiseql-server/src/api/admin_principal.rs::admin_token_management_router",
        Role::Serves(&[Transport::AdminBearer]),
    ),
    (
        "fraiseql-server/src/api/api_key_management.rs::api_key_management_router",
        Role::Serves(&[Transport::AdminBearer]),
    ),
    (
        "fraiseql-server/src/api/rbac_management.rs::rbac_management_router",
        Role::Serves(&[Transport::AdminPrincipal]),
    ),
    (
        "fraiseql-server/src/api/saml_idp_management.rs::saml_idp_management_router",
        Role::Serves(&[Transport::AdminPrincipal]),
    ),
    (
        "fraiseql-server/src/api/scim/mod.rs::scim_router",
        Role::Serves(&[Transport::Scim]),
    ),
    (
        "fraiseql-server/src/api/scim/mod.rs::scim_token_management_router",
        Role::Serves(&[Transport::AdminPrincipal]),
    ),
    (
        "fraiseql-server/src/first_party/routes.rs::routes",
        Role::Serves(&[Transport::AuthFlows]),
    ),
    (
        "fraiseql-server/src/identity/admin.rs::identity_admin_router",
        Role::Serves(&[Transport::AdminBearer]),
    ),
    (
        "fraiseql-server/src/inbound/email/admin.rs::suppression_admin_router",
        Role::Serves(&[Transport::AdminBearer]),
    ),
    (
        "fraiseql-server/src/inbound/webhook.rs::webhook_router",
        Role::Serves(&[Transport::InboundWebhook]),
    ),
    ("fraiseql-server/src/mcp/http.rs::mcp_router", Role::Serves(&[Transport::Mcp])),
    (
        "fraiseql-server/src/observers/routes.rs::observer_changelog_routes",
        Role::Serves(&[Transport::AdminOidc]),
    ),
    (
        "fraiseql-server/src/observers/routes.rs::observer_dlq_routes",
        Role::Serves(&[Transport::AdminOidc]),
    ),
    (
        "fraiseql-server/src/observers/routes.rs::observer_routes",
        Role::Serves(&[Transport::AdminOidc]),
    ),
    (
        "fraiseql-server/src/observers/routes.rs::observer_runtime_routes",
        Role::Serves(&[Transport::AdminOidc]),
    ),
    (
        "fraiseql-server/src/routes/async_operations.rs::router",
        Role::Serves(&[Transport::AsyncOperations]),
    ),
    (
        "fraiseql-server/src/routes/rest/router/mod.rs::rest_query_router",
        Role::Serves(&[Transport::Rest]),
    ),
    (
        "fraiseql-server/src/routes/rest/router/mod.rs::rest_router",
        Role::Serves(&[Transport::Rest]),
    ),
    // fraiseql-server: capability mounts
    (
        "fraiseql-server/src/server/capabilities/identity.rs::Identity::routes",
        Role::Serves(&[Transport::AdminBearer]),
    ),
    (
        "fraiseql-server/src/server/capabilities/inbound_email.rs::InboundEmail::routes",
        Role::Serves(&[Transport::AdminBearer]),
    ),
    (
        "fraiseql-server/src/server/capabilities/inbound_webhooks.rs::InboundWebhooks::routes",
        Role::Serves(&[Transport::InboundWebhook]),
    ),
    (
        "fraiseql-server/src/server/capabilities/mcp.rs::Mcp::routes",
        Role::Serves(&[Transport::Mcp]),
    ),
    (
        "fraiseql-server/src/server/capabilities/observers.rs::Observers::routes",
        Role::Serves(&[Transport::AdminOidc]),
    ),
    (
        "fraiseql-server/src/server/capabilities/rest.rs::Rest::routes",
        Role::Serves(&[Transport::Rest]),
    ),
    (
        "fraiseql-server/src/server/capabilities/saml.rs::Saml::routes",
        Role::Serves(&[Transport::AdminPrincipal]),
    ),
    // fraiseql-server: the application's mount sequence
    (
        "fraiseql-server/src/server/routing/admin.rs::Server::mount_admin_api",
        Role::Serves(&[Transport::AdminBearer]),
    ),
    (
        "fraiseql-server/src/server/routing/admin.rs::Server::mount_admin_sql_console",
        Role::Serves(&[Transport::AdminDual]),
    ),
    (
        "fraiseql-server/src/server/routing/admin.rs::Server::mount_base_and_admin_routes",
        Role::Serves(&[Transport::Health, Transport::Playground]),
    ),
    (
        "fraiseql-server/src/server/routing/admin.rs::Server::mount_design_audit",
        Role::Serves(&[Transport::DesignAudit]),
    ),
    (
        "fraiseql-server/src/server/routing/admin.rs::Server::mount_introspection",
        Role::Serves(&[Transport::Introspection]),
    ),
    (
        "fraiseql-server/src/server/routing/admin.rs::Server::mount_jwks_refresh",
        Role::Serves(&[Transport::AdminBearer]),
    ),
    (
        "fraiseql-server/src/server/routing/admin.rs::Server::mount_metrics",
        Role::Serves(&[Transport::Metrics]),
    ),
    (
        "fraiseql-server/src/server/routing/admin.rs::Server::mount_playground",
        Role::Serves(&[Transport::Playground]),
    ),
    (
        "fraiseql-server/src/server/routing/admin.rs::Server::mount_storage_policy_admin",
        Role::Serves(&[Transport::AdminBearer]),
    ),
    (
        "fraiseql-server/src/server/routing/admin.rs::Server::mount_studio_admin_api",
        Role::Serves(&[Transport::AdminBearer]),
    ),
    ("fraiseql-server/src/server/mount_ledger.rs::MountLedger::merge", Role::Composes),
    ("fraiseql-server/src/server/mount_ledger.rs::MountLedger::nest", Role::Composes),
    (
        "fraiseql-server/src/server/routing/admin.rs::Server::mount_subscriptions",
        Role::Serves(&[Transport::Subscriptions]),
    ),
    (
        "fraiseql-server/src/server/routing/auth.rs::Server::behind_session",
        Role::Composes,
    ),
    (
        "fraiseql-server/src/server/routing/auth.rs::Server::mount_auth_routes",
        Role::Serves(&[Transport::AuthFlows, Transport::AuthSession]),
    ),
    (
        "fraiseql-server/src/server/routing/auth.rs::social_router",
        Role::Serves(&[Transport::AuthFlows]),
    ),
    (
        "fraiseql-server/src/server/routing/extensions.rs::Server::add_inbound_routes",
        Role::Serves(&[Transport::InboundWebhook]),
    ),
    (
        "fraiseql-server/src/server/routing/extensions.rs::Server::mount_capability_routes",
        Role::Composes,
    ),
    (
        "fraiseql-server/src/server/routing/extensions.rs::Server::mount_extensions",
        Role::Composes,
    ),
    (
        "fraiseql-server/src/server/routing/extensions.rs::Server::mount_mcp",
        Role::Serves(&[Transport::Mcp]),
    ),
    (
        "fraiseql-server/src/server/routing/extensions.rs::Server::mount_rbac",
        Role::Serves(&[Transport::AdminPrincipal]),
    ),
    (
        "fraiseql-server/src/server/routing/extensions.rs::Server::mount_storage_state",
        Role::Serves(&[Transport::Storage]),
    ),
    (
        "fraiseql-server/src/server/routing/graphql.rs::Server::build_graphql_router",
        Role::Serves(&[Transport::GraphQl]),
    ),
    (
        "fraiseql-server/src/server/routing/middleware.rs::Server::apply_middleware",
        Role::Composes,
    ),
    ("fraiseql-server/src/server/routing/mod.rs::Server::attach_auth", Role::Composes),
    (
        "fraiseql-server/src/server/routing/mod.rs::Server::build_router",
        Role::Composes,
    ),
    (
        "fraiseql-server/src/server/routing/observers.rs::Server::add_observer_routes",
        Role::Serves(&[Transport::AdminOidc]),
    ),
    (
        "fraiseql-server/src/server/routing/observers.rs::mount_observer_runtime_routes",
        Role::Serves(&[Transport::AdminOidc]),
    ),
];

fn table() -> BTreeMap<&'static str, Role> {
    let mut out = BTreeMap::new();
    for (key, role) in MOUNTS {
        assert!(out.insert(*key, *role).is_none(), "`{key}` is classified twice");
    }
    out
}

#[test]
fn every_mount_function_is_classified() {
    let surface = mount_scan::scan();
    let table = table();
    let found: BTreeSet<String> = surface.mounts.iter().map(MountFn::key).collect();
    let unclassified: Vec<&String> =
        found.iter().filter(|k| !table.contains_key(k.as_str())).collect();
    assert!(
        unclassified.is_empty(),
        "these functions add routes to a router but are not in MOUNTS, so no matrix row \
         says which refusals they owe: {unclassified:#?}"
    );
    let stale: Vec<&&str> = table.keys().filter(|k| !found.contains(**k)).collect();
    assert!(
        stale.is_empty(),
        "MOUNTS names functions the scan no longer finds; a stale entry would vouch for \
         a mount nobody checks: {stale:#?}"
    );
}

#[test]
fn a_composing_function_adds_no_route_of_its_own() {
    let table = table();
    let direct: Vec<String> = mount_scan::scan()
        .mounts
        .iter()
        .filter(|m| m.direct && matches!(table.get(m.key().as_str()), Some(Role::Composes)))
        .map(MountFn::key)
        .collect();
    assert!(
        direct.is_empty(),
        "classified as only composing routers, yet they add routes themselves; classify \
         the transport those routes serve: {direct:#?}"
    );
}

#[test]
fn no_route_is_added_to_a_router_after_its_guard() {
    let unguarded: Vec<String> = mount_scan::scan()
        .unguarded
        .iter()
        .map(|u| format!("{}:{} .{}", u.mount.key(), u.line, u.method))
        .collect();
    assert!(
        unguarded.is_empty(),
        "axum's layers wrap only the routes already on a router, so these are served \
         without the guard every other route of their transport has: {unguarded:#?}"
    );
}

#[test]
fn the_scan_reaches_every_crate_that_mounts_routes() {
    let scanned = mount_scan::router_crates();
    let mounting = mount_scan::crates_that_mount_routes();
    let missed: Vec<&String> = mounting.difference(&scanned).collect();
    assert!(
        missed.is_empty(),
        "these crates add routes to a router but the scan does not read them: {missed:?}"
    );
    assert!(
        mounting.contains("fraiseql-server"),
        "the server crate mounts routes; a scan that finds none is reading nothing"
    );
}

/// The transports some mount serves: the matrix's rows. Generated from [`MOUNTS`], so a
/// row exists because a route does. A variant no mount serves is never constructed,
/// which `dead_code` refuses.
fn mounted_transports() -> BTreeSet<Transport> {
    MOUNTS
        .iter()
        .filter_map(|(_, role)| match role {
            Role::Serves(ts) => Some(ts.iter().copied()),
            Role::Composes => None,
        })
        .flatten()
        .collect()
}

#[test]
fn the_rows_include_every_data_transport() {
    let rows = mounted_transports();
    for t in [
        Transport::GraphQl,
        Transport::AsyncOperations,
        Transport::Subscriptions,
        Transport::Rest,
        Transport::Mcp,
        Transport::Storage,
    ] {
        assert!(rows.contains(&t), "{t:?} serves data but has no row: the scan read nothing");
    }
}

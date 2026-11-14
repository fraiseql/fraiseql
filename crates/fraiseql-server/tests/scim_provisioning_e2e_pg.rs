//! #946: SCIM 2.0 provisioning through the shipped server's mount.
//!
//! Boots the real server from a `[scim]` config and drives the surface over HTTP the way a
//! provisioning client does: discover, create, filter, page, update under `If-Match`, and
//! deactivate. The assertions that matter are the two the issue is about — a provisioning
//! credential is **not** the admin credential, and `active = false` ends access rather than
//! merely recording an intention.
//!
//! Self-skips when no `DATABASE_URL` is set. Runs in the Dagger `saml` integration suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `fraiseql_p16_scim_*` databases →
//! run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics/skips are fine
#![allow(clippy::doc_markdown)] // Reason: technical terms (IdP, SCIM) throughout the docs

mod common;

use std::sync::Arc;

use common::first_party::FirstPartyAuth;
use fraiseql_core::{db::postgres::PostgresAdapter, schema::CompiledSchema};
use fraiseql_server::{
    Server,
    server_config::{ServerConfig, scim::ScimServerConfig},
    token_revocation::{
        InMemoryRevocationStore, RevocationError, RevocationStore, TokenRevocationManager,
    },
};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};
use sqlx::PgPool;

/// The variable `[auth.database]` names in this suite (#1965). Auth runs on the scratch
/// database's own URL here; `auth_database_e2e_pg` covers auth on a role of its own.
const AUTH_URL_ENV: &str = "FRAISEQL_TEST_SCIM_PROVISIONING_AUTH_URL";

const ADMIN_TOKEN: &str = "p16-scim-admin-token-at-least-32-chars";
const PATCH_OP: &str = "urn:ietf:params:scim:api:messages:2.0:PatchOp";
/// How this server signs the sessions it mints.
const FIRST_PARTY: FirstPartyAuth = FirstPartyAuth::new("https://api.example.com", "fraiseql");

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
    std::env::set_var(AUTH_URL_ENV, with_database(admin_url, db));
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

fn scim_config() -> ServerConfig {
    FIRST_PARTY.apply(ServerConfig {
        cors_enabled: false,
        admin_token: Some(ADMIN_TOKEN.to_string()),
        scim: Some(ScimServerConfig {
            enabled:  true,
            base_url: "/scim/v2".to_string(),
        }),
        ..ServerConfig::default()
    })
}

fn empty_schema() -> CompiledSchema {
    serde_json::from_value(json!({
        "fraiseql_version": fraiseql_core::schema::CURRENT_FRAISEQL_VERSION,
        "types": [],
        "queries": [],
        "mutations": [],
        "auth": {
            "database": { "url_env": AUTH_URL_ENV },
            "session": FIRST_PARTY.session_json(),
        },
    }))
    .expect("compiled schema")
}

/// A booted server plus everything a test needs to talk to it.
struct Rig {
    base:   String,
    client: reqwest::Client,
    token:  String,
    pool:   PgPool,
    stop:   tokio::sync::oneshot::Sender<()>,
    handle: tokio::task::JoinHandle<Result<(), fraiseql_server::ServerError>>,
}

impl Rig {
    // Reason: builds the request rather than sending it, but sits alongside the rig's
    // awaited helpers so every call site reads the same way.
    #[allow(unknown_lints, clippy::unused_async_trait_impl)]
    async fn scim(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        self.client
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(&self.token)
    }

    async fn shutdown(self) {
        let _ = self.stop.send(());
        let _ = self.handle.await;
    }
}

async fn boot(url: &str, db: &str) -> Rig {
    boot_with(url, db, None).await
}

/// [`boot`], with a token revocation installed after `Server::new`, the way the binary
/// installs its `postgres` backend.
async fn boot_with(url: &str, db: &str, revocation: Option<Arc<TokenRevocationManager>>) -> Rig {
    FIRST_PARTY.set_env();
    let pool = scratch_pool(url, db).await;
    let scratch_url = with_database(url, db);

    let mut config = scim_config();
    config.database_url.clone_from(&scratch_url);

    let adapter = Arc::new(PostgresAdapter::new(&scratch_url).await.expect("PostgresAdapter::new"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();

    let mut server = Box::pin(Server::new(config, empty_schema(), adapter, Some(pool.clone())))
        .await
        .expect("[scim] with a pool and an admin token must boot");
    if let Some(revocation) = revocation {
        server = server.with_revocation_manager(revocation);
    }
    let (stop, rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let client = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{port}");

    // Mint a provisioning credential the way an operator would: through the admin API.
    let minted: Value = client
        .post(format!("{base}/api/scim/tokens"))
        .bearer_auth(ADMIN_TOKEN)
        .json(&json!({ "idp_name": "acme-okta" }))
        .send()
        .await
        .expect("mint token")
        .json()
        .await
        .expect("mint token json");
    let token = minted["token"].as_str().expect("the token is shown once").to_string();

    Rig {
        base,
        client,
        token,
        pool,
        stop,
        handle,
    }
}

/// The credential separation the issue calls for: a provisioning token must not be an admin
/// token, and — the direction that actually matters — the admin token must not be usable as
/// a provisioning credential.
#[tokio::test]
async fn provisioning_and_admin_credentials_are_not_interchangeable() {
    let Some(url) = database_url_or_skip("provisioning_and_admin_credentials") else {
        return;
    };
    let db = "fraiseql_p16_scim_creds";
    let rig = boot(&url, db).await;

    // No credential at all.
    let resp = rig.client.get(format!("{}/scim/v2/Users", rig.base)).send().await.unwrap();
    assert_eq!(resp.status(), 401, "SCIM must require a credential");

    // The ADMIN token is not a provisioning token. If this ever passes, every SCIM
    // integration is holding an admin credential.
    let resp = rig
        .client
        .get(format!("{}/scim/v2/Users", rig.base))
        .bearer_auth(ADMIN_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401, "the admin token must not authenticate to SCIM");

    // The provisioning token is not an admin token either.
    let resp = rig
        .client
        .get(format!("{}/api/scim/tokens", rig.base))
        .bearer_auth(&rig.token)
        .send()
        .await
        .unwrap();
    // The admin bearer gate answers 403 rather than 401 — either way it is refused, and
    // the property under test is that this credential does not open that door.
    assert_eq!(resp.status(), 403, "a provisioning credential must not reach the admin surface");

    // And the one it is meant for works.
    let resp = rig.scim(reqwest::Method::GET, "/scim/v2/Users").await.send().await.unwrap();
    assert_eq!(resp.status(), 200);

    rig.shutdown().await;
    drop_scratch(&url, db).await;
}

/// The lifecycle an IdP drives, end to end, with the deactivation assertion at the centre.
#[tokio::test]
async fn provisioning_lifecycle_and_deactivation_over_http() {
    let Some(url) = database_url_or_skip("provisioning_lifecycle") else {
        return;
    };
    let db = "fraiseql_p16_scim_lifecycle";
    let rig = boot(&url, db).await;

    // Create.
    let resp = rig
        .scim(reqwest::Method::POST, "/scim/v2/Users")
        .await
        .json(&json!({
            "schemas": ["urn:ietf:params:scim:schemas:core:2.0:User"],
            "userName": "ada@example.com",
            "externalId": "okta-123",
            "name": { "givenName": "Ada", "familyName": "Lovelace" },
            "emails": [{ "value": "ada@example.com", "primary": true }],
            "active": true,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "create must answer 201");
    assert!(
        resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("application/scim+json")),
        "SCIM responses must carry the SCIM media type"
    );
    let etag = resp.headers().get("etag").and_then(|v| v.to_str().ok()).map(str::to_string);
    let created: Value = resp.json().await.unwrap();
    let user_id = created["id"].as_str().expect("id").to_string();
    assert_eq!(created["userName"], "ada@example.com");
    assert_eq!(created["active"], true);
    assert!(created["meta"]["version"].is_string(), "meta.version drives If-Match");

    // A repeat is a `uniqueness` conflict, which is how a client reconciles.
    let resp = rig
        .scim(reqwest::Method::POST, "/scim/v2/Users")
        .await
        .json(&json!({ "userName": "ada@example.com" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["scimType"], "uniqueness", "the client branches on this: {body}");

    // Filter — the "does this user already exist?" probe every client sends first.
    let found: Value = rig
        .scim(reqwest::Method::GET, "/scim/v2/Users")
        .await
        .query(&[("filter", r#"userName eq "ada@example.com""#)])
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(found["totalResults"], 1, "{found}");
    assert_eq!(found["Resources"][0]["id"], user_id.as_str());

    // An unsupported filter is refused, never silently answered with the whole directory.
    let resp = rig
        .scim(reqwest::Method::GET, "/scim/v2/Users")
        .await
        .query(&[("filter", r#"userName sw "ad""#)])
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "an unsupported filter must not widen the result set");

    // A stale If-Match is a lost update, and is refused.
    let resp = rig
        .scim(reqwest::Method::PUT, &format!("/scim/v2/Users/{user_id}"))
        .await
        .header("If-Match", "W/\"999\"")
        .json(&json!({ "userName": "ada@example.com", "active": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 412, "a stale version must not overwrite a newer one");

    // The current one succeeds.
    let resp = rig
        .scim(reqwest::Method::PUT, &format!("/scim/v2/Users/{user_id}"))
        .await
        .header("If-Match", etag.expect("create returned an ETag"))
        .json(&json!({
            "userName": "ada@example.com",
            "displayName": "Ada L.",
            "active": true,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // A session exists, as it would after any login.
    let sessions = FIRST_PARTY.session_store(&rig.pool).await;
    let tokens = {
        use fraiseql_auth::SessionStore as _;
        sessions
            .create_session(&user_id, fraiseql_auth::session::unix_now().unwrap() + 3600)
            .await
            .expect("an active account may hold a session")
    };
    let hash = fraiseql_auth::session::hash_token(&tokens.refresh_token);

    // Offboard. This is the request an IdP sends when someone leaves.
    let resp = rig
        .scim(reqwest::Method::PATCH, &format!("/scim/v2/Users/{user_id}"))
        .await
        .json(&json!({
            "schemas": [PATCH_OP],
            "Operations": [{ "op": "replace", "path": "active", "value": false }],
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let patched: Value = resp.json().await.unwrap();
    assert_eq!(patched["active"], false);

    // Both halves of "offboarding is not cosmetic":
    {
        use fraiseql_auth::SessionStore as _;
        assert!(
            sessions.get_session(&hash).await.is_err(),
            "the existing session must have been revoked"
        );
        let refused = sessions
            .create_session(&user_id, fraiseql_auth::session::unix_now().unwrap() + 3600)
            .await;
        assert!(
            matches!(refused, Err(fraiseql_auth::AuthError::AccountDeactivated)),
            "a new session must be refused for a deactivated account: {refused:?}"
        );
    }

    // A PatchOp without its schema is a client that meant to PUT; refused rather than
    // silently given partial-update semantics.
    let resp = rig
        .scim(reqwest::Method::PATCH, &format!("/scim/v2/Users/{user_id}"))
        .await
        .json(&json!({ "Operations": [{ "op": "replace", "path": "active", "value": true }] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    // Delete.
    let resp = rig
        .scim(reqwest::Method::DELETE, &format!("/scim/v2/Users/{user_id}"))
        .await
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);
    let resp = rig
        .scim(reqwest::Method::GET, &format!("/scim/v2/Users/{user_id}"))
        .await
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);

    rig.shutdown().await;
    drop_scratch(&url, db).await;
}

/// Discovery is what a provisioning client fetches before it will provision anything.
#[tokio::test]
async fn discovery_documents_describe_the_surface_that_exists() {
    let Some(url) = database_url_or_skip("discovery_documents") else {
        return;
    };
    let db = "fraiseql_p16_scim_discovery";
    let rig = boot(&url, db).await;

    let config: Value = rig
        .scim(reqwest::Method::GET, "/scim/v2/ServiceProviderConfig")
        .await
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(config["patch"]["supported"], true);
    assert_eq!(config["filter"]["supported"], true);
    assert_eq!(config["etag"]["supported"], true);
    // Declared unsupported because they are: a client told otherwise would send them.
    assert_eq!(config["bulk"]["supported"], false);
    assert_eq!(config["sort"]["supported"], false);

    let types: Value = rig
        .scim(reqwest::Method::GET, "/scim/v2/ResourceTypes")
        .await
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(types["totalResults"], 2, "User and Group: {types}");

    let schemas: Value = rig
        .scim(reqwest::Method::GET, "/scim/v2/Schemas")
        .await
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(schemas["totalResults"], 2);

    let user_schema: Value = rig
        .scim(
            reqwest::Method::GET,
            "/scim/v2/Schemas/urn:ietf:params:scim:schemas:core:2.0:User",
        )
        .await
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(user_schema["id"], "urn:ietf:params:scim:schemas:core:2.0:User");

    rig.shutdown().await;
    drop_scratch(&url, db).await;
}

/// A SCIM group becomes an RBAC role with **no permissions**, and its members become role
/// assignments. A provisioning credential that could grant permissions would be an admin
/// credential under another name.
#[tokio::test]
async fn groups_become_permissionless_rbac_roles() {
    let Some(url) = database_url_or_skip("groups_become_rbac_roles") else {
        return;
    };
    let db = "fraiseql_p16_scim_groups";
    let rig = boot(&url, db).await;

    let user: Value = rig
        .scim(reqwest::Method::POST, "/scim/v2/Users")
        .await
        .json(&json!({ "userName": "eng@example.com", "active": true }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let user_id = user["id"].as_str().unwrap().to_string();

    let resp = rig
        .scim(reqwest::Method::POST, "/scim/v2/Groups")
        .await
        .json(&json!({
            "schemas": ["urn:ietf:params:scim:schemas:core:2.0:Group"],
            "displayName": "Engineering",
            "members": [{ "value": user_id }],
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let group: Value = resp.json().await.unwrap();
    assert_eq!(group["displayName"], "Engineering");

    // The mirrored role exists and holds no permissions.
    let roles: Value = rig
        .client
        .get(format!("{}/api/roles", rig.base))
        .bearer_auth(ADMIN_TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let engineering = roles["items"]
        .as_array()
        .expect("roles page")
        .iter()
        .find(|r| r["name"] == "Engineering")
        .expect("the SCIM group must be mirrored onto a role");
    assert!(
        engineering["permissions"].as_array().is_none_or(Vec::is_empty),
        "a SCIM-created role must carry no permissions: {engineering}"
    );

    // The member holds the role.
    let assignments: Value = rig
        .client
        .get(format!("{}/api/user-roles", rig.base))
        .query(&[("user_id", user_id.as_str())])
        .bearer_auth(ADMIN_TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        assignments["items"].as_array().map(Vec::len),
        Some(1),
        "group membership must become a role assignment: {assignments}"
    );

    // Removing the member through PATCH revokes it again.
    let group_id = group["id"].as_str().unwrap();
    let resp = rig
        .scim(reqwest::Method::PATCH, &format!("/scim/v2/Groups/{group_id}"))
        .await
        .json(&json!({
            "schemas": [PATCH_OP],
            "Operations": [{ "op": "remove", "path": format!("members[value eq \"{user_id}\"]") }],
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let patched: Value = resp.json().await.unwrap();
    assert!(
        patched["members"].as_array().is_none_or(Vec::is_empty),
        "the member must be gone: {patched}"
    );

    let assignments: Value = rig
        .client
        .get(format!("{}/api/user-roles", rig.base))
        .query(&[("user_id", user_id.as_str())])
        .bearer_auth(ADMIN_TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        assignments["items"].as_array().map(Vec::len),
        Some(0),
        "removal must be mirrored too: {assignments}"
    );

    rig.shutdown().await;
    drop_scratch(&url, db).await;
}

/// Mint a provisioning credential scoped to `tenant`, the way an operator would.
async fn mint_for_tenant(rig: &Rig, idp_name: &str, tenant: &str) -> String {
    let minted: Value = rig
        .client
        .post(format!("{}/api/scim/tokens", rig.base))
        .bearer_auth(ADMIN_TOKEN)
        .json(&json!({ "idp_name": idp_name, "tenant_id": tenant }))
        .send()
        .await
        .expect("mint token")
        .json()
        .await
        .expect("mint token json");
    assert_eq!(minted["tenant_id"], tenant, "the minted token must carry its tenant: {minted}");
    minted["token"].as_str().expect("the token is shown once").to_string()
}

/// Two IdPs, two tenants, one server. Neither credential reaches the other tenant's users,
/// and a group cannot enlist one: the refusal is SCIM's `invalidValue`, and no role is
/// granted to the foreign user.
#[tokio::test]
async fn a_tenant_credential_cannot_reach_another_tenants_users() {
    let Some(url) = database_url_or_skip("tenant_credential_isolation") else {
        return;
    };
    let db = "fraiseql_scim_tenant_isolation";
    let rig = boot(&url, db).await;
    let acme = mint_for_tenant(&rig, "acme-okta", "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa").await;
    let rival = mint_for_tenant(&rig, "rival-entra", "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb").await;
    let as_tenant = |token: &str, method: reqwest::Method, path: &str| {
        rig.client
            .request(method, format!("{}{path}", rig.base))
            .bearer_auth(token.to_string())
    };

    let resp = as_tenant(&rival, reqwest::Method::POST, "/scim/v2/Users")
        .json(&json!({ "userName": "victim@rival.example", "active": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let victim_id = resp.json::<Value>().await.unwrap()["id"].as_str().unwrap().to_string();

    let listed: Value = as_tenant(&acme, reqwest::Method::GET, "/scim/v2/Users")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(listed["totalResults"], 0, "another tenant's users must not be listed: {listed}");

    let resp = as_tenant(&acme, reqwest::Method::GET, &format!("/scim/v2/Users/{victim_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404, "another tenant's user must not be readable");

    // The victim is signed in: a refused delete must not end their session either.
    let sessions = FIRST_PARTY.session_store(&rig.pool).await;
    let victim_session = {
        use fraiseql_auth::SessionStore as _;
        sessions
            .create_session(&victim_id, fraiseql_auth::session::unix_now().unwrap() + 3600)
            .await
            .expect("an active account may hold a session")
    };
    let resp = as_tenant(&acme, reqwest::Method::DELETE, &format!("/scim/v2/Users/{victim_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404, "another tenant's user must not be deletable");
    // ...and the refusal is the one an id that exists nowhere gets, byte for byte.
    let unknown = as_tenant(
        &acme,
        reqwest::Method::DELETE,
        "/scim/v2/Users/00000000-0000-4000-8000-000000000000",
    )
    .send()
    .await
    .unwrap();
    assert_eq!(unknown.status(), resp.status());
    assert_eq!(
        unknown.headers().get("content-type"),
        resp.headers().get("content-type"),
        "another tenant's id and an unknown id must answer alike"
    );
    let unknown = unknown.bytes().await.unwrap();
    assert_eq!(
        unknown,
        resp.bytes().await.unwrap(),
        "another tenant's id and an unknown id must answer alike"
    );
    // Both are the store's not-found, the reply a delete of an unknown id has always given.
    assert_eq!(
        serde_json::from_slice::<Value>(&unknown).unwrap(),
        json!({
            "schemas": ["urn:ietf:params:scim:api:messages:2.0:Error"],
            "status": "404",
            "detail": "Resource not found",
        }),
        "a delete of an unknown id keeps its reply"
    );
    {
        use fraiseql_auth::SessionStore as _;
        let hash = fraiseql_auth::session::hash_token(&victim_session.refresh_token);
        assert!(
            sessions.get_session(&hash).await.is_ok(),
            "another tenant's credential must not end the user's session"
        );
    }

    let resp = as_tenant(&acme, reqwest::Method::POST, "/scim/v2/Groups")
        .json(&json!({
            "schemas": ["urn:ietf:params:scim:schemas:core:2.0:Group"],
            "displayName": "Admins",
            "members": [{ "value": victim_id }],
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "a group must not enlist another tenant's user");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["scimType"], "invalidValue", "{body}");

    let assignments: Value = rig
        .client
        .get(format!("{}/api/user-roles", rig.base))
        .query(&[("user_id", victim_id.as_str())])
        .bearer_auth(ADMIN_TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        assignments["items"].as_array().map(Vec::len),
        Some(0),
        "no role may reach the foreign user: {assignments}"
    );

    // Its own tenant still sees it.
    let resp = as_tenant(&rival, reqwest::Method::GET, &format!("/scim/v2/Users/{victim_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    rig.shutdown().await;
    drop_scratch(&url, db).await;
}

// ── Deactivation ends the access tokens a user already holds ─────────────────────

fn in_memory_revocation() -> Arc<TokenRevocationManager> {
    Arc::new(TokenRevocationManager::new(
        Arc::new(InMemoryRevocationStore::new()),
        false,
        false,
        3600,
    ))
}

/// A revocation store that reads but cannot write.
struct UnwritableRevocations;

// Reason: implements the dyn-dispatched `RevocationStore`, an `async_trait`
#[async_trait::async_trait]
impl RevocationStore for UnwritableRevocations {
    async fn is_revoked(&self, _jti: &str) -> Result<bool, RevocationError> {
        Ok(false)
    }

    async fn revoke(&self, _jti: &str, _ttl_secs: u64) -> Result<(), RevocationError> {
        Err(RevocationError::Backend("the store refuses writes".to_string()))
    }

    async fn revoke_all_for_user(&self, _sub: &str, _ttl: u64) -> Result<(), RevocationError> {
        Err(RevocationError::Backend("the store refuses writes".to_string()))
    }

    async fn user_revoked_after(&self, _sub: &str) -> Result<Option<i64>, RevocationError> {
        Ok(None)
    }
}

/// Provision an active user and sign them in the way this server's sign-in does: a
/// session row and an access token signed with the server's own key.
async fn provisioned_session(rig: &Rig, user_name: &str) -> (String, fraiseql_auth::TokenPair) {
    let resp = rig
        .scim(reqwest::Method::POST, "/scim/v2/Users")
        .await
        .json(&json!({ "userName": user_name, "active": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "create must answer 201");
    let created: Value = resp.json().await.unwrap();
    let user_id = created["id"].as_str().expect("id").to_string();
    let sessions = FIRST_PARTY.session_store(&rig.pool).await;
    let tokens = {
        use fraiseql_auth::SessionStore as _;
        sessions
            .create_session(&user_id, fraiseql_auth::session::unix_now().unwrap() + 3600)
            .await
            .expect("an active account may hold a session")
    };
    (user_id, tokens)
}

/// What the server answers a holder of `access`.
async fn probe(rig: &Rig, access: &str) -> reqwest::StatusCode {
    rig.client
        .post(format!("{}/graphql", rig.base))
        .bearer_auth(access)
        .json(&json!({ "query": "{ __typename }" }))
        .send()
        .await
        .expect("request")
        .status()
}

async fn deactivate_by_put(rig: &Rig, user_id: &str, user_name: &str) -> reqwest::Response {
    rig.scim(reqwest::Method::PUT, &format!("/scim/v2/Users/{user_id}"))
        .await
        .json(&json!({ "userName": user_name, "active": false }))
        .send()
        .await
        .unwrap()
}

async fn deactivate_by_patch(rig: &Rig, user_id: &str) -> reqwest::Response {
    rig.scim(reqwest::Method::PATCH, &format!("/scim/v2/Users/{user_id}"))
        .await
        .json(&json!({
            "schemas": [PATCH_OP],
            "Operations": [{ "op": "replace", "path": "active", "value": false }],
        }))
        .send()
        .await
        .unwrap()
}

async fn delete_user(rig: &Rig, user_id: &str) -> reqwest::Response {
    rig.scim(reqwest::Method::DELETE, &format!("/scim/v2/Users/{user_id}"))
        .await
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn a_put_deactivation_ends_the_users_access_tokens() {
    let Some(url) = database_url_or_skip("a_put_deactivation_ends_access") else {
        return;
    };
    let db = "fraiseql_p16_scim_put_access";
    let rig = boot_with(&url, db, Some(in_memory_revocation())).await;
    let (user_id, tokens) = provisioned_session(&rig, "put@example.com").await;
    assert_eq!(probe(&rig, &tokens.access_token).await, 200, "before the deactivation");

    let resp = deactivate_by_put(&rig, &user_id, "put@example.com").await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        probe(&rig, &tokens.access_token).await,
        401,
        "a deactivated user's access token must stop working"
    );

    rig.shutdown().await;
    drop_scratch(&url, db).await;
}

#[tokio::test]
async fn a_patch_deactivation_ends_the_users_access_tokens() {
    let Some(url) = database_url_or_skip("a_patch_deactivation_ends_access") else {
        return;
    };
    let db = "fraiseql_p16_scim_patch_access";
    let rig = boot_with(&url, db, Some(in_memory_revocation())).await;
    let (user_id, tokens) = provisioned_session(&rig, "patch@example.com").await;
    assert_eq!(probe(&rig, &tokens.access_token).await, 200, "before the deactivation");

    let resp = deactivate_by_patch(&rig, &user_id).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        probe(&rig, &tokens.access_token).await,
        401,
        "a deactivated user's access token must stop working"
    );

    rig.shutdown().await;
    drop_scratch(&url, db).await;
}

#[tokio::test]
async fn a_delete_ends_the_users_access_tokens() {
    let Some(url) = database_url_or_skip("a_delete_ends_access") else {
        return;
    };
    let db = "fraiseql_p16_scim_delete_access";
    let rig = boot_with(&url, db, Some(in_memory_revocation())).await;
    let (user_id, tokens) = provisioned_session(&rig, "delete@example.com").await;
    assert_eq!(probe(&rig, &tokens.access_token).await, 200, "before the delete");

    let resp = delete_user(&rig, &user_id).await;
    assert_eq!(resp.status(), 204);
    assert_eq!(
        probe(&rig, &tokens.access_token).await,
        401,
        "a deleted user's access token must stop working"
    );

    rig.shutdown().await;
    drop_scratch(&url, db).await;
}

/// A delete whose access-token denial fails answers 500, so the `IdP` retries it: the
/// account is gone, but its tokens are not, and a 204 would end the conversation.
#[tokio::test]
async fn a_delete_that_cannot_deny_the_access_tokens_answers_500() {
    let Some(url) = database_url_or_skip("a_delete_that_cannot_deny") else {
        return;
    };
    let db = "fraiseql_p16_scim_delete_unwritable";
    let revocation =
        Arc::new(TokenRevocationManager::new(Arc::new(UnwritableRevocations), false, false, 3600));
    let rig = boot_with(&url, db, Some(revocation)).await;
    let (user_id, _tokens) = provisioned_session(&rig, "unwritable@example.com").await;

    let resp = delete_user(&rig, &user_id).await;
    assert_eq!(resp.status(), 500, "a denial that failed must not answer 204");
    // The user is still there, so the IdP's retry revokes again instead of meeting a 404.
    let resp = rig
        .scim(reqwest::Method::GET, &format!("/scim/v2/Users/{user_id}"))
        .await
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "a delete that answered 500 must leave the user to retry on");

    rig.shutdown().await;
    drop_scratch(&url, db).await;
}

/// Make the database refuse to end a session, as a failing or read-only one would.
async fn refuse_session_updates(pool: &PgPool) {
    sqlx::raw_sql(
        "CREATE FUNCTION refuse_session_update() RETURNS trigger LANGUAGE plpgsql AS \
         $$ BEGIN RAISE EXCEPTION 'sessions are read-only'; END $$; \
         CREATE TRIGGER refuse_session_update BEFORE UPDATE ON auth.sessions \
         FOR EACH ROW EXECUTE FUNCTION refuse_session_update();",
    )
    .execute(pool)
    .await
    .expect("install the refusing trigger");
}

async fn allow_session_updates(pool: &PgPool) {
    sqlx::raw_sql("DROP TRIGGER refuse_session_update ON auth.sessions")
        .execute(pool)
        .await
        .expect("drop the refusing trigger");
}

async fn assert_session_ended(rig: &Rig, refresh_token: &str) {
    use fraiseql_auth::SessionStore as _;
    let sessions = fraiseql_auth::PostgresSessionStore::new(rig.pool.clone());
    let hash = fraiseql_auth::session::hash_token(refresh_token);
    assert!(sessions.get_session(&hash).await.is_err(), "the retry ended the session");
}

/// The PUT twin of the retry below: a PUT that left the user inactive but could not end
/// the sessions, sent again, ends them although the user is already inactive.
#[tokio::test]
async fn a_put_retried_after_a_failed_revocation_ends_the_sessions() {
    let Some(url) = database_url_or_skip("a_put_retried_after_a_failed_revocation") else {
        return;
    };
    let db = "fraiseql_p16_scim_put_retry";
    let rig = boot_with(&url, db, Some(in_memory_revocation())).await;
    let (user_id, tokens) = provisioned_session(&rig, "put-retry@example.com").await;

    refuse_session_updates(&rig.pool).await;
    let resp = deactivate_by_put(&rig, &user_id, "put-retry@example.com").await;
    assert_eq!(resp.status(), 500, "a session revocation that failed must not answer 200");

    allow_session_updates(&rig.pool).await;
    let resp = deactivate_by_put(&rig, &user_id, "put-retry@example.com").await;
    assert_eq!(resp.status(), 200, "the retry succeeds");
    assert_session_ended(&rig, &tokens.refresh_token).await;

    rig.shutdown().await;
    drop_scratch(&url, db).await;
}

/// A deactivation whose session revocation fails answers 500, so the `IdP` retries it: a
/// live refresh token outlives every access token, and would mint new ones.
#[tokio::test]
async fn a_deactivation_that_cannot_end_the_sessions_answers_500() {
    let Some(url) = database_url_or_skip("a_deactivation_that_cannot_end_sessions") else {
        return;
    };
    let db = "fraiseql_p16_scim_sessions_unwritable";
    let rig = boot_with(&url, db, Some(in_memory_revocation())).await;
    let (user_id, tokens) = provisioned_session(&rig, "sessions@example.com").await;
    refuse_session_updates(&rig.pool).await;
    let resp = deactivate_by_patch(&rig, &user_id).await;
    assert_eq!(resp.status(), 500, "a session revocation that failed must not answer 200");

    // The IdP retries once the database recovers. The user is already inactive, and the
    // retry must still end the session.
    allow_session_updates(&rig.pool).await;
    let resp = deactivate_by_patch(&rig, &user_id).await;
    assert_eq!(resp.status(), 200, "the retry succeeds");
    assert_session_ended(&rig, &tokens.refresh_token).await;

    rig.shutdown().await;
    drop_scratch(&url, db).await;
}

/// The document this suite serves loads, checked with no database.
///
/// Every other test here reaches the document only after it has found a database, so in
/// a run without one they skip before it is built and a load-time refusal of it reports
/// as a pass (`2b843cd27`: 12 tests red for a session under a green preflight).
/// This one needs nothing but the loader, so that refusal cannot hide.
///
/// This document is deserialised with `from_value` and never meets `finish_load`.
/// `from_value` ignores a top-level key it does not know, so what can refuse it is a
/// field it does not send becoming required, or a field changing type.
#[test]
fn the_document_loads_without_a_database() {
    empty_schema();
}

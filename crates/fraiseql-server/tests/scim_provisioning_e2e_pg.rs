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

use std::sync::Arc;

use fraiseql_core::{db::postgres::PostgresAdapter, schema::CompiledSchema};
use fraiseql_server::{
    Server,
    server_config::{ServerConfig, hs256::Hs256Config, scim::ScimServerConfig},
};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};
use sqlx::PgPool;

const HS256_SECRET: &str = "p16-scim-hs256-secret-32-bytes!!";
const SECRET_ENV: &str = "FRAISEQL_TEST_P16_SCIM_HS256_SECRET";
const ADMIN_TOKEN: &str = "p16-scim-admin-token";
const PATCH_OP: &str = "urn:ietf:params:scim:api:messages:2.0:PatchOp";

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

fn scim_config() -> ServerConfig {
    ServerConfig {
        cors_enabled: false,
        admin_token: Some(ADMIN_TOKEN.to_string()),
        scim: Some(ScimServerConfig {
            enabled:  true,
            base_url: "/scim/v2".to_string(),
        }),
        auth_hs256: Some(Hs256Config {
            secret_env: SECRET_ENV.to_string(),
            issuer:     Some("https://api.example.com".to_string()),
            audience:   Some("fraiseql".to_string()),
        }),
        ..ServerConfig::default()
    }
}

fn empty_schema() -> CompiledSchema {
    serde_json::from_value(json!({
        "version": "2.0.0",
        "types": [],
        "queries": [],
        "mutations": [],
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
    std::env::set_var(SECRET_ENV, HS256_SECRET);
    let pool = scratch_pool(url, db).await;
    let scratch_url = with_database(url, db);

    let mut config = scim_config();
    config.database_url.clone_from(&scratch_url);

    let adapter = Arc::new(PostgresAdapter::new(&scratch_url).await.expect("PostgresAdapter::new"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();

    let server = Box::pin(Server::new(config, empty_schema(), adapter, Some(pool.clone())))
        .await
        .expect("[scim] with a pool and an admin token must boot");
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
    let sessions = fraiseql_auth::PostgresSessionStore::with_hs256_secret(
        rig.pool.clone(),
        HS256_SECRET.as_bytes().to_vec(),
    );
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

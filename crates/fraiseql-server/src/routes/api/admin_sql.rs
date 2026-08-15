//! `POST /api/v1/admin/sql` — the operator SQL console (#962).
//!
//! Studio's SQL tab. This is the only endpoint on the server that executes SQL
//! FraiseQL did not generate, which makes the containment the feature rather than
//! an add-on to it:
//!
//! | Control | Enforced by | Not by |
//! |---|---|---|
//! | read-only for `admin_readonly_token` | the transaction's `READ ONLY` mode | reading the SQL |
//! | rollback by default | `ROLLBACK` on the same transaction | trusting the statement |
//! | one statement only | the extended query protocol's Parse | splitting on `;` |
//! | statement timeout | `SET LOCAL statement_timeout` | a client-side deadline |
//! | row cap | stopping the read and saying so | `LIMIT` appended to the text |
//! | RLS preview | the session variables the executor would set | a `WHERE tenant = …` rewrite |
//!
//! Every execution is written to the audit ledger before the response is built,
//! including the ones that were refused and the ones that failed.

use std::collections::HashMap;

use axum::{
    Json,
    extract::{Extension, State},
};
use fraiseql_core::{
    db::{AdminSqlOutcome, AdminSqlRequest, traits::DatabaseAdapter},
    security::{AuthenticatedUser, SecurityContext},
    types::UserId,
};
use serde::{Deserialize, Serialize};

use crate::{
    middleware::{AdminCaller, AdminPrivilege},
    routes::{
        api::types::{ApiError, ApiResponse},
        graphql::AppState,
    },
    server_config::AdminSqlConfig,
};

/// How much of the statement is copied into the audit entry.
///
/// [`AuditEntry::context`](fraiseql_auth::audit::logger::AuditEntry) is bounded at
/// 2 KB and carries the other fields too, so a long statement is truncated. The
/// entry always also carries the SHA-256 of the *whole* text, so a truncated
/// record still identifies exactly which statement ran.
const AUDIT_SQL_PREVIEW_BYTES: usize = 1024;

/// Router state for the console: the app state it reads the schema and adapter
/// from, plus the section that bounds it.
///
/// A separate state rather than a field on [`AppState`]: the console is mounted
/// by one call site under three conditions, and the config it enforces should
/// not be reachable — or forgettable — from handlers that are not it.
#[derive(Clone)]
pub struct AdminSqlState<A: DatabaseAdapter> {
    /// The server's shared state; the console reads the executor's adapter and
    /// the compiled schema's session-variable mappings from it.
    pub app:    AppState<A>,
    /// The `[admin_sql]` section, already validated at boot.
    pub config: AdminSqlConfig,
}

/// The identity to preview under.
///
/// Whatever the schema's `session_variables` map from a JWT is resolved from
/// these fields, by the *same* function the executor uses on a real request
/// ([`fraiseql_core::runtime::resolve_session_variables`]).
/// That is the whole point: a preview computed by a second implementation is a
/// preview of that implementation.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ImpersonateClaims {
    /// The `sub` claim — the previewed principal.
    pub user_id:   String,
    /// The tenant to preview as, when the deployment is multi-tenant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    /// Roles carried on the previewed token.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles:     Vec<String>,
    /// Scopes carried on the previewed token.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes:    Vec<String>,
    /// Any further claims the schema's session variables read.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub claims:    HashMap<String, serde_json::Value>,
}

/// Request body for `POST /api/v1/admin/sql`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdminSqlRequestBody {
    /// The statement to run. Exactly one; see the module docs.
    pub sql: String,

    /// Commit instead of rolling back.
    ///
    /// Defaults to `false`, which is what makes the console a preview. Refused
    /// for a read-only token, and refused entirely when `[admin_sql]
    /// allow_commit = false`.
    #[serde(default)]
    pub commit: bool,

    /// Lower the row cap for this request. Never raises it above `[admin_sql]
    /// max_rows`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_rows: Option<usize>,

    /// Lower the statement timeout for this request. Never raises it above
    /// `[admin_sql] statement_timeout_ms`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub statement_timeout_ms: Option<u32>,

    /// Run under a previewed identity's session variables instead of as the
    /// pool's role (the "as admin" toggle, inverted).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub impersonate: Option<ImpersonateClaims>,
}

/// Response body for `POST /api/v1/admin/sql`.
///
/// Reports the bounds that were *applied*, not the ones that were asked for: an
/// operator who requested a 60-second timeout on a server capped at 30 needs to
/// see 30, or they will read a cancellation as a hung query.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AdminSqlResponse {
    /// Column names, in result order.
    pub columns:              Vec<String>,
    /// Rows, in result order, each aligned to `columns`.
    pub rows:                 Vec<Vec<serde_json::Value>>,
    /// `true` when the row cap cut the result short.
    pub truncated:            bool,
    /// Rows the statement reported affected, when PostgreSQL reported a count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows_affected:        Option<u64>,
    /// Whether the transaction was committed. `false` means nothing persisted.
    pub committed:            bool,
    /// Whether the transaction ran `READ ONLY`.
    pub read_only:            bool,
    /// The timeout actually applied, in milliseconds.
    pub statement_timeout_ms: u32,
    /// The row cap actually applied.
    pub max_rows:             usize,
}

/// Run one operator-supplied statement.
///
/// # Errors
///
/// * `400 Bad Request` — empty statement, or a bound requested as `0` (PostgreSQL reads a `0`
///   timeout as *no* timeout, so accepting it would silently remove the control), or an
///   impersonation claim in the reserved `fraiseql.` namespace.
/// * `403 Forbidden` — `commit` requested by a read-only token, or with `allow_commit = false`.
/// * `500 Internal Server Error` — anything PostgreSQL rejected, including the write refused by the
///   read-only transaction and the statement the timeout cancelled. The database's own message is
///   returned: an operator debugging their own SQL needs it, and this endpoint is already the most
///   privileged one on the server.
///
/// Requires an admin token; which one decides whether writes are possible at all.
pub async fn admin_sql_handler<A: DatabaseAdapter + 'static>(
    State(state): State<AdminSqlState<A>>,
    caller: Option<Extension<AdminCaller>>,
    Json(body): Json<AdminSqlRequestBody>,
) -> Result<Json<ApiResponse<AdminSqlResponse>>, ApiError> {
    // The caller is established by `admin_dual_auth_middleware`, which is the only
    // way to reach this route. Its absence means the route was mounted without
    // that layer — a wiring mistake, refused rather than defaulted, because the
    // only available default is the powerful one.
    let Some(Extension(caller)) = caller else {
        return Err(ApiError::new(
            "Admin SQL console reached without an authenticated admin privilege",
            "FORBIDDEN",
        ));
    };
    let privilege = caller.privilege;

    let prepared = prepare(&state.config, privilege, &body);
    let result = match prepared {
        Ok(request) => execute(&state, request).await,
        Err(refusal) => Err(refusal),
    };

    audit_execution(privilege, &caller.peer_ip, &body, result.as_ref());

    result.map(|(applied, outcome)| {
        ApiResponse::success(AdminSqlResponse {
            columns:              outcome.columns,
            rows:                 outcome.rows,
            truncated:            outcome.truncated,
            rows_affected:        outcome.rows_affected,
            committed:            outcome.committed,
            read_only:            applied.read_only,
            statement_timeout_ms: applied.statement_timeout_ms,
            max_rows:             applied.max_rows,
        })
    })
}

/// The bounds that will actually be applied, after the request's asks are
/// clamped against the section's ceilings.
#[derive(Debug, Clone, Copy)]
struct AppliedBounds {
    read_only:            bool,
    statement_timeout_ms: u32,
    max_rows:             usize,
}

/// Turn a request body into an [`AdminSqlRequest`], or refuse it.
///
/// Every refusal happens here, before anything touches the database, so the
/// audit entry for a refused request records the same fields as one for an
/// executed request.
fn prepare(
    config: &AdminSqlConfig,
    privilege: AdminPrivilege,
    body: &AdminSqlRequestBody,
) -> Result<(AppliedBounds, AdminSqlRequestPlan), ApiError> {
    if body.sql.trim().is_empty() {
        return Err(ApiError::validation_error("sql must not be empty"));
    }

    let read_only = privilege == AdminPrivilege::ReadOnly;

    if body.commit {
        if !config.allow_commit {
            return Err(ApiError::new(
                "This server's SQL console is preview-only: [admin_sql] allow_commit = false, \
                 so a statement can be run and read but never committed",
                "FORBIDDEN",
            ));
        }
        if read_only {
            return Err(ApiError::new(
                "A read-only admin token cannot commit. Its transaction runs READ ONLY, so a \
                 commit would persist nothing while reporting that it had; authenticate with \
                 admin_token to make a change",
                "FORBIDDEN",
            ));
        }
    }

    // A requested bound may only tighten the configured one. Zero is refused
    // rather than clamped in both cases: PostgreSQL reads `statement_timeout = 0`
    // as "no timeout", so accepting it would turn the strictest-looking request
    // into the one with no limit at all.
    let statement_timeout_ms = match body.statement_timeout_ms {
        Some(0) => {
            return Err(ApiError::validation_error(
                "statement_timeout_ms must be greater than 0 (PostgreSQL reads 0 as no timeout)",
            ));
        },
        Some(requested) => requested.min(config.statement_timeout_ms),
        None => config.statement_timeout_ms,
    };
    let max_rows = match body.max_rows {
        Some(0) => {
            return Err(ApiError::validation_error("max_rows must be greater than 0"));
        },
        Some(requested) => requested.min(config.max_rows),
        None => config.max_rows,
    };

    Ok((
        AppliedBounds {
            read_only,
            statement_timeout_ms,
            max_rows,
        },
        AdminSqlRequestPlan {
            sql:         body.sql.clone(),
            commit:      body.commit,
            impersonate: body.impersonate.clone(),
        },
    ))
}

/// The parts of a prepared request that still need the schema to resolve.
struct AdminSqlRequestPlan {
    sql:         String,
    commit:      bool,
    impersonate: Option<ImpersonateClaims>,
}

/// Resolve the previewed session variables and run the statement.
async fn execute<A: DatabaseAdapter + 'static>(
    state: &AdminSqlState<A>,
    (bounds, plan): (AppliedBounds, AdminSqlRequestPlan),
) -> Result<(AppliedBounds, AdminSqlOutcome), ApiError> {
    let executor = state.app.executor();
    let session_vars = match plan.impersonate {
        Some(ref claims) => impersonated_session_vars(executor.schema(), claims)?,
        None => Vec::new(),
    };

    let request = AdminSqlRequest {
        sql: plan.sql,
        read_only: bounds.read_only,
        commit: plan.commit,
        statement_timeout_ms: bounds.statement_timeout_ms,
        max_rows: bounds.max_rows,
        session_vars,
    };

    executor
        .adapter()
        .execute_admin_sql(&request)
        .await
        .map(|outcome| (bounds, outcome))
        .map_err(classify_database_error)
}

/// SQLSTATE for a write attempted in a `READ ONLY` transaction.
const PG_READ_ONLY_SQL_TRANSACTION: &str = "25006";
/// SQLSTATE for a statement the server cancelled — here, `statement_timeout`.
const PG_QUERY_CANCELED: &str = "57014";

/// Give the database's refusal the HTTP status it means.
///
/// This reads the **server's answer**, not the operator's SQL, so it is not the
/// text-parsing this endpoint refuses to do: PostgreSQL has already decided, and
/// translating `25006` into a 500 would tell an operator their read-only token
/// hit a server bug when it in fact worked exactly as configured.
///
/// The database's own message is passed through. This endpoint is reachable only
/// with an admin credential and its whole purpose is running SQL an operator
/// wrote; withholding the reason it failed would make it useless. `pg_detail`
/// takes only `message`, never `DETAIL` — which is where PostgreSQL puts row
/// values (the #911 leak).
fn classify_database_error(e: fraiseql_core::error::FraiseQLError) -> ApiError {
    match e {
        fraiseql_core::error::FraiseQLError::Unsupported { message } => {
            ApiError::new(format!("Unsupported: {message}"), "UNSUPPORTED_OPERATION")
        },
        fraiseql_core::error::FraiseQLError::Database {
            ref message,
            sql_state: Some(ref state),
        } if state == PG_READ_ONLY_SQL_TRANSACTION => ApiError::new(
            format!(
                "Refused by the database: this statement writes, and a read-only admin token \
                 runs it in a READ ONLY transaction ({message})"
            ),
            "FORBIDDEN",
        ),
        fraiseql_core::error::FraiseQLError::Database {
            ref message,
            sql_state: Some(ref state),
        } if state == PG_QUERY_CANCELED => ApiError::new(
            format!("Statement cancelled by statement_timeout ({message})"),
            "TIMEOUT",
        ),
        other => ApiError::internal_error(other.to_string()),
    }
}

/// Build the session variables a real request from `claims` would carry.
///
/// Goes through the schema's own `session_variables` mappings and
/// [`fraiseql_core::runtime::resolve_session_variables`]
/// — the function the executor calls — so a preview cannot differ from the thing
/// it previews. A deployment with no session-variable mappings gets an empty
/// list, and impersonating on it changes nothing: there is no RLS identity to
/// set, and pretending otherwise would show an operator a filtered view that the
/// database is not applying.
///
/// # Errors
///
/// A claim in the reserved `fraiseql.` namespace is refused by name. The token
/// extractor strips that namespace precisely so a client cannot write it; an
/// operator endpoint that accepted it would be the hole the extractor closes.
fn impersonated_session_vars(
    schema: &fraiseql_core::schema::CompiledSchema,
    claims: &ImpersonateClaims,
) -> Result<Vec<(String, String)>, ApiError> {
    const RESERVED_PREFIX: &str = "fraiseql.";

    if let Some(reserved) = claims.claims.keys().find(|k| k.starts_with(RESERVED_PREFIX)) {
        return Err(ApiError::validation_error(format!(
            "impersonate.claims may not set '{reserved}': the `{RESERVED_PREFIX}` namespace is \
             reserved for values the server derives, and is stripped from real tokens for the \
             same reason"
        )));
    }

    let user = AuthenticatedUser {
        user_id:      UserId::new(claims.user_id.clone()),
        scopes:       claims.scopes.clone(),
        // A preview is a preview of *now*; an expiry in the past would make the
        // context read as expired to anything that inspects it later.
        expires_at:   chrono::Utc::now() + chrono::Duration::minutes(5),
        email:        None,
        display_name: None,
        extra_claims: claims.claims.clone(),
    };

    let mut context =
        SecurityContext::from_user(&user, format!("admin-sql-{}", uuid::Uuid::new_v4()));
    context.roles.clone_from(&claims.roles);
    for (key, value) in &claims.claims {
        context = context.with_attribute(key.clone(), value.clone());
    }
    if let Some(ref tenant) = claims.tenant_id {
        context = context.with_tenant(tenant.clone());
    }

    fraiseql_core::runtime::resolve_session_variables(&schema.session_variables, &context).map_err(
        |e| ApiError::validation_error(format!("impersonation could not be resolved: {e}")),
    )
}

/// Write one entry to the audit ledger for this request.
///
/// Called on **every** path — executed, failed and refused — from the single
/// point in [`admin_sql_handler`] between deciding the outcome and building the
/// response, so there is no branch that can return without a record. A ledger
/// that holds only successful executions answers "what did the operator run?"
/// with the subset that worked.
///
/// The entry names the statement (truncated, with the full text's SHA-256), the
/// privilege that authenticated, whether a commit was asked for, and the peer.
fn audit_execution(
    privilege: AdminPrivilege,
    peer_ip: &str,
    body: &AdminSqlRequestBody,
    result: Result<&(AppliedBounds, AdminSqlOutcome), &ApiError>,
) {
    use fraiseql_auth::audit::logger::{AuditEntry, AuditEventType, SecretType, get_audit_logger};
    use sha2::{Digest as _, Sha256};

    let digest = hex::encode(Sha256::digest(body.sql.as_bytes()));
    let preview: String = body.sql.chars().take(AUDIT_SQL_PREVIEW_BYTES).collect();
    let privilege = match privilege {
        AdminPrivilege::ReadOnly => "admin_readonly_token",
        AdminPrivilege::ReadWrite => "admin_token",
    };
    let committed = result.is_ok_and(|(_, outcome)| outcome.committed);
    let impersonating = body.impersonate.as_ref().map_or("none", |c| c.user_id.as_str());

    get_audit_logger().log_entry(AuditEntry {
        event_type:    AuditEventType::AdminSqlExecution,
        secret_type:   SecretType::AdminToken,
        subject:       Some(privilege.to_string()),
        operation:     "admin_sql".to_string(),
        success:       result.is_ok(),
        error_message: result.err().map(ToString::to_string),
        context:       Some(format!(
            "peer_ip={peer_ip} sha256={digest} commit_requested={} committed={committed} \
             impersonate={impersonating} sql={preview}",
            body.commit
        )),
        chain_hash:    None,
    });
}

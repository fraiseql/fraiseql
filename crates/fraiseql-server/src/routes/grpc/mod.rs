//! gRPC transport — row-shaped view queries via protobuf wire encoding.
//!
//! This module implements a tonic gRPC service that accepts protobuf
//! requests, translates filters into a WHERE clause via
//! `GenericWhereGenerator`, calls `DatabaseAdapter::execute_row_query()`,
//! and encodes the resulting `ColumnValue` rows into protobuf responses.
//!
//! The service is built dynamically from the compiled schema's descriptor pool
//! at server startup — no generated Rust protobuf code is needed.

pub mod handler;
pub mod streaming;

use std::{convert::Infallible, sync::Arc};

use fraiseql_core::{
    db::{SupportsMutations, traits::DatabaseAdapter},
    schema::CompiledSchema,
    security::{OidcValidator, SecurityContext},
};
use fraiseql_error::FraiseQLError;
use handler::{RpcDispatchTable, build_dispatch_table};
use prost::Message as _;
use prost_reflect::DescriptorPool;
use tonic::{body::Body as TonicBody, server::NamedService};
use tracing::{Instrument as _, debug, info, info_span, warn};

use crate::middleware::RateLimiter;

// ---------------------------------------------------------------------------
// Service bundle returned by `build_grpc_service()`
// ---------------------------------------------------------------------------

/// Bundle of services produced by [`build_grpc_service()`].
///
/// Contains the dynamic gRPC service, optional descriptor bytes for
/// reflection, and the fully-qualified service name.
pub struct GrpcServices<A: DatabaseAdapter> {
    /// The dynamic gRPC service that dispatches RPCs.
    pub service:                     DynamicGrpcService<A>,
    /// Raw `FileDescriptorSet` bytes for building reflection at serve time.
    /// Present when `GrpcConfig.reflection` is true.
    pub reflection_descriptor_bytes: Option<Vec<u8>>,
    /// Fully-qualified service name (e.g., `"fraiseql.v1.FraiseQLService"`).
    pub service_name:                String,
}

// ---------------------------------------------------------------------------
// Dynamic gRPC service
// ---------------------------------------------------------------------------

/// A dynamically-built tonic gRPC service that routes requests to row-shaped
/// view queries based on the compiled schema and protobuf descriptors.
///
/// Unlike generated tonic services, this service is constructed at runtime from
/// a [`DescriptorPool`] loaded from the `descriptor.binpb` file produced by
/// `fraiseql-cli generate-proto`.
pub struct DynamicGrpcService<A: DatabaseAdapter> {
    /// The **configured** executor — every read and every write (#1330, #1351).
    ///
    /// Supplied by the caller rather than built here: `Executor::new` would use
    /// `RuntimeConfig::default()`, so the `Authorizer`, the RLS policy and the
    /// `before:mutation` gate would all be absent — the transport would converge
    /// at the chokepoint and find half the gates missing, which is #1333's shape.
    executor:          Arc<fraiseql_core::runtime::Executor<A>>,
    /// Compiled schema (for type lookups during request processing).
    schema:            Arc<CompiledSchema>,
    /// RPC method → operation metadata dispatch table.
    dispatch:          Arc<RpcDispatchTable>,
    /// Protobuf descriptor pool (for decoding/encoding dynamic messages).
    pool:              Arc<DescriptorPool>,
    /// Fully-qualified service name (e.g., `"fraiseql.v1.FraiseQLService"`).
    service_name:      Arc<str>,
    /// Optional OIDC validator for JWT authentication.
    /// When present, incoming requests must carry a valid `authorization`
    /// metadata header (`Bearer <jwt>`). The validated token is converted
    /// into a [`SecurityContext`] that drives RLS WHERE clause injection.
    oidc_validator:    Option<Arc<OidcValidator>>,
    /// Optional shared rate limiter (same instance used by GraphQL/REST).
    /// When present, requests are throttled per-IP and per-user before dispatch.
    rate_limiter:      Option<Arc<RateLimiter>>,
    /// The enriched-identity resolver (#1336). `Some` exactly when
    /// `[identity.enrichment].enabled`, and passed in for the same reason
    /// `executor` is: this transport is mounted by an embedder, and anything it
    /// builds for itself is a copy that drifts from the one the deployment configured.
    #[cfg(feature = "auth")]
    identity_resolver: Option<Arc<crate::identity::IdentityResolver>>,
}

impl<A: DatabaseAdapter> Clone for DynamicGrpcService<A> {
    fn clone(&self) -> Self {
        Self {
            executor: Arc::clone(&self.executor),
            schema: Arc::clone(&self.schema),
            dispatch: Arc::clone(&self.dispatch),
            pool: Arc::clone(&self.pool),
            service_name: Arc::clone(&self.service_name),
            oidc_validator: self.oidc_validator.as_ref().map(Arc::clone),
            rate_limiter: self.rate_limiter.as_ref().map(Arc::clone),
            #[cfg(feature = "auth")]
            identity_resolver: self.identity_resolver.as_ref().map(Arc::clone),
        }
    }
}

impl<A: DatabaseAdapter> NamedService for DynamicGrpcService<A> {
    const NAME: &'static str = "fraiseql.v1.FraiseQLService";
}

impl<A: DatabaseAdapter + SupportsMutations + Clone + Send + Sync + 'static> DynamicGrpcService<A> {
    /// Handle a unary gRPC request.
    ///
    /// When an [`OidcValidator`] is configured, the handler extracts the
    /// `authorization` HTTP header (gRPC metadata), validates the Bearer JWT,
    /// and builds a [`SecurityContext`].  Unauthenticated requests are
    /// rejected with `UNAUTHENTICATED` (gRPC status 16).
    ///
    /// The resulting `SecurityContext` is threaded through to
    /// [`handler::execute_grpc_read`], which hands it to the engine — where it
    /// drives the RLS policy, the operation `Authorizer` and every other gate a
    /// read faces (#1351).
    async fn handle_request(
        &self,
        method: &str,
        req: http::Request<TonicBody>,
    ) -> http::Response<TonicBody> {
        use http_body_util::BodyExt as _;

        let Some(op) = self.dispatch.get(method) else {
            return grpc_error_response(
                tonic::Code::Unimplemented,
                &format!("Method not found: {method}"),
            );
        };

        // ── Auth interceptor ──────────────────────────────────────────
        // Extract headers before any `.await` so the non-Sync request body
        // is not held across the token-validation await point.
        let auth_header = req
            .headers()
            .get(http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .map(String::from);
        let request_id = req
            .headers()
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("grpc")
            .to_string();

        // Extract client IP for rate limiting (x-forwarded-for → x-real-ip → fallback).
        let client_ip = req
            .headers()
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').next())
            .map(|s| s.trim().to_string())
            .or_else(|| {
                req.headers().get("x-real-ip").and_then(|v| v.to_str().ok()).map(String::from)
            })
            .unwrap_or_else(|| "unknown".to_string());

        let security_context: Option<SecurityContext> =
            match self.authenticate(auth_header, request_id).await {
                Ok(ctx) => ctx,
                Err(resp) => return resp,
            };

        // Record user_id on the tracing span (set by `call()`).
        if let Some(ref ctx) = security_context {
            tracing::Span::current().record("user_id", ctx.user_id.as_str());
        }

        // ── Rate limiting ─────────────────────────────────────────────
        if let Some(ref limiter) = self.rate_limiter {
            // Per-user limit if authenticated, per-IP otherwise.
            let result = if let Some(ref ctx) = security_context {
                limiter.check_user_limit(ctx.user_id.as_str()).await
            } else {
                limiter.check_ip_limit(&client_ip).await
            };

            if !result.allowed {
                let user_id = security_context.as_ref().map(|c| c.user_id.as_str());
                warn!(
                    ip = %client_ip,
                    user_id = ?user_id,
                    retry_after_secs = result.retry_after_secs,
                    method = %method,
                    "gRPC rate limit exceeded"
                );
                return grpc_error_response(tonic::Code::ResourceExhausted, "Rate limit exceeded");
            }
        }

        // Collect the body bytes.
        let body_bytes: bytes::Bytes = match req.into_body().collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(e) => {
                return grpc_error_response(
                    tonic::Code::Internal,
                    &format!("Failed to read request body: {e}"),
                );
            },
        };

        // Skip the gRPC frame header (1 byte compression flag + 4 bytes length).
        if body_bytes.len() < 5 {
            return grpc_error_response(tonic::Code::InvalidArgument, "Request body too short");
        }
        let msg_bytes = &body_bytes[5..];

        // Find the request message descriptor.
        let Some(service_desc) = self.pool.get_service_by_name(&self.service_name) else {
            return grpc_error_response(tonic::Code::Internal, "Service descriptor not found");
        };

        let method_name = method.rsplit('/').next().unwrap_or(method);
        let Some(method_desc) = service_desc.methods().find(|m| m.name() == method_name) else {
            return grpc_error_response(
                tonic::Code::Unimplemented,
                &format!("Method not found: {method_name}"),
            );
        };

        let request_desc = method_desc.input();
        let request_msg = match prost_reflect::DynamicMessage::decode(request_desc, msg_bytes) {
            Ok(m) => m,
            Err(e) => {
                return grpc_error_response(
                    tonic::Code::InvalidArgument,
                    &format!("Failed to decode request: {e}"),
                );
            },
        };

        // Dispatch based on RPC kind.
        //
        // Server-streaming RPCs return early with a streaming body;
        // unary RPCs continue to the framing code below.
        if let handler::RpcKind::ServerStream {
            columns,
            row_descriptor,
        } = &op.kind
        {
            let Some(type_def) = self.schema.find_type(&op.type_name) else {
                return grpc_error_response(
                    tonic::Code::Internal,
                    &format!("Type '{}' not found in schema", op.type_name),
                );
            };

            let batch_size = self.schema.grpc_config.as_ref().map_or(500, |c| c.stream_batch_size);

            debug!(method = %method, batch_size, "Starting gRPC server-streaming response");

            let body_stream = streaming::build_streaming_body(
                Arc::clone(&self.executor),
                op.operation_name.clone(),
                columns.clone(),
                row_descriptor.clone(),
                type_def,
                &request_msg,
                security_context.as_ref(),
                batch_size,
            )
            .await;

            let body = http_body_util::StreamBody::new(body_stream);
            let mut response = http::Response::new(TonicBody::new(body));
            response
                .headers_mut()
                .insert("content-type", http::HeaderValue::from_static("application/grpc"));
            return response;
        }

        let response_msg = match &op.kind {
            handler::RpcKind::Query {
                returns_list,
                columns,
                row_descriptor,
            } => {
                // Look up the type definition.
                let Some(type_def) = self.schema.find_type(&op.type_name) else {
                    return grpc_error_response(
                        tonic::Code::Internal,
                        &format!("Type '{}' not found in schema", op.type_name),
                    );
                };

                let read = match handler::execute_grpc_read(
                    self.executor.as_ref(),
                    &op.operation_name,
                    columns,
                    *returns_list,
                    &request_msg,
                    type_def,
                    security_context.as_ref(),
                )
                .await
                {
                    Ok(read) => read,
                    Err(FraiseQLError::Validation { message, .. }) => {
                        return grpc_error_response(tonic::Code::InvalidArgument, &message);
                    },
                    Err(FraiseQLError::Unsupported { message }) => {
                        return grpc_error_response(tonic::Code::Unimplemented, &message);
                    },
                    // #1351: the gates the engine now applies to this arm refuse with
                    // `Authorization`. Mapped to `PermissionDenied` rather than falling
                    // into `Internal` below — a refused caller must not be told the
                    // server broke, and a client cannot retry its way out of a 403.
                    Err(FraiseQLError::Authorization { message, .. }) => {
                        return grpc_error_response(tonic::Code::PermissionDenied, &message);
                    },
                    Err(e) => return grpc_error_response(tonic::Code::Internal, &e.to_string()),
                };

                debug!(
                    method = %method,
                    row_count = read.rows.len(),
                    "gRPC query returned results"
                );

                // The engine narrows the projection when field-level RBAC withholds a
                // field, and `ColumnValue`s are positional — so the response is encoded
                // with the columns the read *used*, never the ones the table holds.
                handler::encode_response(
                    read.rows,
                    &read.columns,
                    *returns_list,
                    row_descriptor,
                    &op.response_descriptor,
                )
            },
            handler::RpcKind::ServerStream { .. } => {
                // Defense in depth: streaming RPCs are routed through a
                // separate handler upstream of this match. If a refactor ever
                // breaks that invariant, fail closed with an error response
                // instead of panicking the worker.
                return grpc_error_response(
                    tonic::Code::Internal,
                    "ServerStream RPC reached unary handler",
                );
            },
            handler::RpcKind::Mutation { .. } => {
                // Under a camelCase GraphQL surface, reverse object-valued arg keys
                // to canonical snake_case before the SQL call — same contract as the
                // GraphQL/REST mutation paths (#456).
                let recase_input_keys = self.schema.naming_convention
                    == fraiseql_core::schema::NamingConvention::CamelCase;
                // #1330: the mutation **name**, not `RpcKind::Mutation`'s SQL
                // function name. The dispatch table resolved the name at startup
                // and then carried only `sql_source`; the chokepoint needs the
                // name to find the mutation's gates, arguments and return type.
                // The principal is the one this request already authenticated —
                // it was in scope here all along and simply was not passed.
                let result = match handler::execute_grpc_mutation(
                    &self.executor,
                    &op.operation_name,
                    &request_msg,
                    recase_input_keys,
                    security_context.as_ref(),
                )
                .await
                {
                    Ok(r) => r,
                    Err(FraiseQLError::Validation { message, .. }) => {
                        return grpc_error_response(tonic::Code::InvalidArgument, &message);
                    },
                    Err(FraiseQLError::Unsupported { message }) => {
                        return grpc_error_response(tonic::Code::Unimplemented, &message);
                    },
                    Err(e) => return grpc_error_response(tonic::Code::Internal, &e.to_string()),
                };

                debug!(method = %method, success = result.success, "gRPC mutation completed");

                handler::encode_mutation_response(&result, &op.response_descriptor)
            },
        };

        // Serialize to protobuf bytes with gRPC framing.
        let response_bytes = response_msg.encode_to_vec();
        let mut framed = Vec::with_capacity(5 + response_bytes.len());
        framed.push(0); // no compression
        framed.extend_from_slice(
            &(u32::try_from(response_bytes.len()).unwrap_or(u32::MAX)).to_be_bytes(),
        );
        framed.extend_from_slice(&response_bytes);

        let mut response = http::Response::new(TonicBody::new(axum::body::Body::from(framed)));
        response
            .headers_mut()
            .insert("content-type", http::HeaderValue::from_static("application/grpc"));
        // gRPC trailers: status OK
        response
            .headers_mut()
            .insert("grpc-status", http::HeaderValue::from_static("0"));
        response
    }
}

impl<A: DatabaseAdapter + SupportsMutations + Clone + Send + Sync + 'static> DynamicGrpcService<A> {
    /// Extract and validate a Bearer JWT token.
    ///
    /// Returns `Ok(Some(SecurityContext))` when the token is valid,
    /// `Ok(None)` when no OIDC validator is configured (auth disabled), or
    /// `Err(response)` with gRPC `UNAUTHENTICATED` when auth is required but
    /// the token is missing or invalid.
    ///
    /// The caller pre-extracts `auth_header` and `request_id` from the HTTP
    /// request *before* any `.await`, so that `http::Request<TonicBody>` (which
    /// is not `Sync`) need not be held across the token-validation await point.
    // Reason: the `Err` variant IS the protocol's rejection value — it is returned to
    // the client verbatim. Boxing it to shrink the `Result` would add an allocation
    // on every rejection and force each `?` site to unbox what it is about to return.
    #[allow(clippy::result_large_err)]
    async fn authenticate(
        &self,
        auth_header: Option<String>,
        request_id: String,
    ) -> std::result::Result<Option<SecurityContext>, http::Response<TonicBody>> {
        let Some(validator) = self.oidc_validator.as_ref() else {
            return Ok(None); // Auth not configured — allow anonymous access.
        };

        let token = match auth_header.as_deref() {
            Some(h) if h.starts_with("Bearer ") => h[7..].to_string(),
            Some(_) => {
                debug!("gRPC request has invalid Authorization header format");
                return Err(grpc_error_response(
                    tonic::Code::Unauthenticated,
                    "Invalid Authorization header format",
                ));
            },
            None => {
                if validator.is_required() {
                    debug!("gRPC request missing required Authorization header");
                    return Err(grpc_error_response(
                        tonic::Code::Unauthenticated,
                        "Authentication required",
                    ));
                }
                return Ok(None);
            },
        };

        match validator.validate_token(&token).await {
            Ok(user) => {
                debug!(user_id = %user.user_id, "gRPC user authenticated");
                Self::principal_from_user(
                    #[cfg(feature = "auth")]
                    self.identity_resolver.as_deref(),
                    &user,
                    request_id,
                )
                .await
                .map(Some)
            },
            Err(e) => {
                warn!(error = %e, "gRPC token validation failed");
                Err(grpc_error_response(tonic::Code::Unauthenticated, "Invalid or expired token"))
            },
        }
    }

    /// Turn a **validated** token's user into the principal this request dispatches
    /// with — the shared context build plus enrichment (#1336).
    ///
    /// Split out from [`authenticate`](Self::authenticate) because everything above it
    /// is JWKS machinery that needs a live key endpoint, while everything in it is the
    /// producer logic that has twice been wrong on this transport. Tests drive it
    /// directly, the way MCP's `call_tool_authenticated` is driven — and it takes the
    /// resolver rather than `&self` so driving it needs no descriptor pool, no dispatch
    /// table and no adapter, none of which it reads.
    ///
    /// `build_security_context` rather than `SecurityContext::from_user`: #858's fix
    /// never reached gRPC, so `tenant_id` stayed unset and `attributes` stayed empty —
    /// the JWT's `org_id` never became a tenant and every `SessionVariableSource::Jwt`
    /// mapping resolved to nothing. It is also a *prerequisite* for the enrichment
    /// call below whenever the configured query binds anything other than `$sub`:
    /// `claims_for_binding` reads `attributes`, so against an empty map a query binding
    /// `$org_id` fails its parameter and denies every subject. A `$sub`-only query
    /// would have masked that — which is why the two are tested separately below.
    ///
    /// # Errors
    ///
    /// `PERMISSION_DENIED` when the identity is denied, `UNAVAILABLE` when resolution
    /// fails transiently — the gRPC spellings of the same two answers every other
    /// transport gives, with the same generic bodies.
    async fn principal_from_user(
        #[cfg(feature = "auth")] identity_resolver: Option<&crate::identity::IdentityResolver>,
        user: &fraiseql_core::security::AuthenticatedUser,
        request_id: String,
    ) -> std::result::Result<SecurityContext, http::Response<TonicBody>> {
        let ctx =
            crate::extractors::build_security_context(user, request_id).with_transport("grpc");
        // Shadowed rather than declared `mut` up front: without `auth` there is no
        // resolver and nothing mutates it, and an unconditional `mut` warns in that
        // arm — the arm `--all-features` never builds.
        #[cfg(feature = "auth")]
        let ctx = {
            let mut ctx = ctx;
            match crate::identity::resolve_request_identity(identity_resolver, Some(&mut ctx)).await
            {
                crate::identity::EnrichmentOutcome::Proceed => ctx,
                crate::identity::EnrichmentOutcome::Denied => {
                    return Err(grpc_error_response(
                        tonic::Code::PermissionDenied,
                        crate::identity::EnrichmentOutcome::DENIED_MESSAGE,
                    ));
                },
                crate::identity::EnrichmentOutcome::Unavailable => {
                    return Err(grpc_error_response(
                        tonic::Code::Unavailable,
                        crate::identity::EnrichmentOutcome::UNAVAILABLE_MESSAGE,
                    ));
                },
            }
        };
        Ok(ctx)
    }
}

/// Build an HTTP response with a gRPC error status.
fn grpc_error_response(code: tonic::Code, message: &str) -> http::Response<TonicBody> {
    let mut response = http::Response::new(TonicBody::empty());
    response
        .headers_mut()
        .insert("content-type", http::HeaderValue::from_static("application/grpc"));
    response
        .headers_mut()
        .insert("grpc-status", http::HeaderValue::from(code as i32));
    if let Ok(msg) = http::HeaderValue::from_str(message) {
        response.headers_mut().insert("grpc-message", msg);
    }
    response
}

/// Implement the [`tower::Service`] trait for routing gRPC requests.
impl<A: DatabaseAdapter + SupportsMutations + Clone + Send + Sync + 'static>
    tower::Service<http::Request<TonicBody>> for DynamicGrpcService<A>
{
    type Error = Infallible;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;
    type Response = http::Response<TonicBody>;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<TonicBody>) -> Self::Future {
        let svc = self.clone();
        let method = req.uri().path().to_string();

        Box::pin(async move {
            let span = info_span!(
                "grpc_request",
                method = %method,
                grpc.status = tracing::field::Empty,
                user_id = tracing::field::Empty,
            );
            let response = svc.handle_request(&method, req).instrument(span.clone()).await;

            // Record the gRPC status code on the span.
            let grpc_status = response
                .headers()
                .get("grpc-status")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("unknown");
            span.record("grpc.status", grpc_status);

            Ok(response)
        })
    }
}

// ---------------------------------------------------------------------------
// Service construction
// ---------------------------------------------------------------------------

/// Build a [`DynamicGrpcService`] from a compiled schema and descriptor file.
///
/// Returns `None` if gRPC is not configured or not enabled.
/// Returns `Some(GrpcServices)` on success, containing the dynamic service,
/// an optional reflection service, and the service name.
///
/// # Errors
///
/// Returns an error if the descriptor file is invalid or the dispatch table
/// cannot be built.
pub fn build_grpc_service<
    A: DatabaseAdapter + SupportsMutations + Clone + Send + Sync + 'static,
>(
    schema: Arc<CompiledSchema>,
    executor: Arc<fraiseql_core::runtime::Executor<A>>,
    oidc_validator: Option<Arc<OidcValidator>>,
    rate_limiter: Option<Arc<RateLimiter>>,
    #[cfg(feature = "auth")] identity_resolver: Option<Arc<crate::identity::IdentityResolver>>,
) -> Result<Option<GrpcServices<A>>, FraiseQLError> {
    let grpc_config = match schema.grpc_config.as_ref() {
        Some(cfg) if cfg.enabled => cfg,
        _ => return Ok(None),
    };

    // Load the FileDescriptorSet from the descriptor file.
    let descriptor_path = &grpc_config.descriptor_path;
    let descriptor_bytes = std::fs::read(descriptor_path).map_err(|e| {
        FraiseQLError::validation(format!(
            "Failed to read gRPC descriptor file '{descriptor_path}': {e}"
        ))
    })?;

    let pool = DescriptorPool::decode(descriptor_bytes.as_slice()).map_err(|e| {
        FraiseQLError::validation(format!(
            "Failed to decode gRPC descriptor file '{descriptor_path}': {e}"
        ))
    })?;

    // Find the service name. Convention: first service in the descriptor pool.
    let service_name =
        pool.services().next().map(|s| s.full_name().to_string()).ok_or_else(|| {
            FraiseQLError::validation("No gRPC service found in descriptor pool".to_string())
        })?;

    info!(
        service = %service_name,
        descriptor_path = %descriptor_path,
        "Building gRPC dispatch table"
    );

    let dispatch = build_dispatch_table(&schema, &service_name, &pool)?;

    info!(
        service = %service_name,
        rpc_count = dispatch.len(),
        "gRPC dispatch table built"
    );

    for (method, op) in &dispatch {
        match &op.kind {
            handler::RpcKind::Query {
                columns,
                returns_list,
                ..
            } => {
                debug!(
                    method = %method,
                    columns = columns.len(),
                    list = returns_list,
                    "Registered gRPC query RPC"
                );
            },
            handler::RpcKind::ServerStream { columns, .. } => {
                debug!(
                    method = %method,
                    columns = columns.len(),
                    "Registered gRPC server-streaming RPC"
                );
            },
            handler::RpcKind::Mutation { function_name } => {
                debug!(
                    method = %method,
                    function = %function_name,
                    "Registered gRPC mutation RPC"
                );
            },
        }
    }

    if oidc_validator.is_some() {
        info!("gRPC transport: OIDC authentication enabled");
    }
    if rate_limiter.is_some() {
        info!("gRPC transport: rate limiting enabled");
    }

    // Preserve descriptor bytes for reflection service (built at serve time).
    let reflection_descriptor_bytes = if grpc_config.reflection {
        info!("gRPC server reflection enabled");
        Some(descriptor_bytes)
    } else {
        None
    };

    let service = DynamicGrpcService {
        executor,
        schema,
        dispatch: Arc::new(dispatch),
        pool: Arc::new(pool),
        service_name: service_name.clone().into(),
        oidc_validator,
        rate_limiter,
        #[cfg(feature = "auth")]
        identity_resolver,
    };

    Ok(Some(GrpcServices {
        service,
        reflection_descriptor_bytes,
        service_name,
    }))
}

#[cfg(test)]
mod tests;

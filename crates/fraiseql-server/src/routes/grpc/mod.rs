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

use std::convert::Infallible;
use std::sync::Arc;

use fraiseql_core::db::traits::DatabaseAdapter;
use fraiseql_core::schema::CompiledSchema;
use fraiseql_core::security::{OidcValidator, SecurityContext};
use fraiseql_error::FraiseQLError;
use prost_reflect::DescriptorPool;
use tonic::body::Body as TonicBody;
use tonic::server::NamedService;
use tracing::{debug, info, info_span, warn, Instrument as _};

use crate::middleware::RateLimiter;

use handler::{RpcDispatchTable, build_dispatch_table};

// ---------------------------------------------------------------------------
// Service bundle returned by `build_grpc_service()`
// ---------------------------------------------------------------------------

/// Bundle of services produced by [`build_grpc_service()`].
///
/// Contains the dynamic gRPC service, optional descriptor bytes for
/// reflection, and the fully-qualified service name.
pub struct GrpcServices<A: DatabaseAdapter> {
    /// The dynamic gRPC service that dispatches RPCs.
    pub service: DynamicGrpcService<A>,
    /// Raw `FileDescriptorSet` bytes for building reflection at serve time.
    /// Present when `GrpcConfig.reflection` is true.
    pub reflection_descriptor_bytes: Option<Vec<u8>>,
    /// Fully-qualified service name (e.g., `"fraiseql.v1.FraiseQLService"`).
    pub service_name: String,
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
    /// Shared database adapter for executing row queries.
    adapter: Arc<A>,
    /// Compiled schema (for type lookups during request processing).
    schema: Arc<CompiledSchema>,
    /// RPC method → operation metadata dispatch table.
    dispatch: Arc<RpcDispatchTable>,
    /// Protobuf descriptor pool (for decoding/encoding dynamic messages).
    pool: Arc<DescriptorPool>,
    /// Fully-qualified service name (e.g., `"fraiseql.v1.FraiseQLService"`).
    service_name: Arc<str>,
    /// Optional OIDC validator for JWT authentication.
    /// When present, incoming requests must carry a valid `authorization`
    /// metadata header (`Bearer <jwt>`). The validated token is converted
    /// into a [`SecurityContext`] that drives RLS WHERE clause injection.
    oidc_validator: Option<Arc<OidcValidator>>,
    /// Optional shared rate limiter (same instance used by GraphQL/REST).
    /// When present, requests are throttled per-IP and per-user before dispatch.
    rate_limiter: Option<Arc<RateLimiter>>,
}

impl<A: DatabaseAdapter> Clone for DynamicGrpcService<A> {
    fn clone(&self) -> Self {
        Self {
            adapter:        Arc::clone(&self.adapter),
            schema:         Arc::clone(&self.schema),
            dispatch:       Arc::clone(&self.dispatch),
            pool:           Arc::clone(&self.pool),
            service_name:   Arc::clone(&self.service_name),
            oidc_validator: self.oidc_validator.as_ref().map(Arc::clone),
            rate_limiter:   self.rate_limiter.as_ref().map(Arc::clone),
        }
    }
}

impl<A: DatabaseAdapter> NamedService for DynamicGrpcService<A> {
    const NAME: &'static str = "fraiseql.v1.FraiseQLService";
}

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> DynamicGrpcService<A> {
    /// Handle a unary gRPC request.
    ///
    /// When an [`OidcValidator`] is configured, the handler extracts the
    /// `authorization` HTTP header (gRPC metadata), validates the Bearer JWT,
    /// and builds a [`SecurityContext`].  Unauthenticated requests are
    /// rejected with `UNAUTHENTICATED` (gRPC status 16).
    ///
    /// The resulting `SecurityContext` is threaded through to
    /// [`handler::execute_grpc_query`] where it drives RLS WHERE clause
    /// injection.
    async fn handle_request(
        &self,
        method: &str,
        req: http::Request<TonicBody>,
    ) -> http::Response<TonicBody> {
        use http_body_util::BodyExt as _;

        let op = match self.dispatch.get(method) {
            Some(op) => op,
            None => return grpc_error_response(tonic::Code::Unimplemented, &format!("Method not found: {method}")),
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
                req.headers()
                    .get("x-real-ip")
                    .and_then(|v| v.to_str().ok())
                    .map(String::from)
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
                limiter.check_user_limit(&ctx.user_id).await
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
                return grpc_error_response(
                    tonic::Code::ResourceExhausted,
                    "Rate limit exceeded",
                );
            }
        }

        // Collect the body bytes.
        let body_bytes: bytes::Bytes = match req.into_body().collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(e) => return grpc_error_response(tonic::Code::Internal, &format!("Failed to read request body: {e}")),
        };

        // Skip the gRPC frame header (1 byte compression flag + 4 bytes length).
        if body_bytes.len() < 5 {
            return grpc_error_response(tonic::Code::InvalidArgument, "Request body too short");
        }
        let msg_bytes = &body_bytes[5..];

        // Find the request message descriptor.
        let service_desc = match self.pool.get_service_by_name(&self.service_name) {
            Some(s) => s,
            None => return grpc_error_response(tonic::Code::Internal, "Service descriptor not found"),
        };

        let method_name = method.rsplit('/').next().unwrap_or(method);
        let method_desc = match service_desc.methods().find(|m| m.name() == method_name) {
            Some(m) => m,
            None => return grpc_error_response(tonic::Code::Unimplemented, &format!("Method not found: {method_name}")),
        };

        let request_desc = method_desc.input();
        let request_msg = match prost_reflect::DynamicMessage::decode(request_desc, msg_bytes) {
            Ok(m) => m,
            Err(e) => return grpc_error_response(tonic::Code::InvalidArgument, &format!("Failed to decode request: {e}")),
        };

        // Dispatch based on RPC kind.
        //
        // Server-streaming RPCs return early with a streaming body;
        // unary RPCs continue to the framing code below.
        if let handler::RpcKind::ServerStream { view_name, columns, row_descriptor } = &op.kind {
            let type_def = match self.schema.find_type(&op.type_name) {
                Some(t) => t.clone(),
                None => return grpc_error_response(tonic::Code::Internal, &format!("Type '{}' not found in schema", op.type_name)),
            };

            let batch_size = self
                .schema
                .grpc_config
                .as_ref()
                .map_or(500, |c| c.stream_batch_size);

            debug!(method = %method, batch_size, "Starting gRPC server-streaming response");

            let body_stream = streaming::build_streaming_body(
                Arc::clone(&self.adapter),
                view_name.clone(),
                columns.clone(),
                row_descriptor.clone(),
                type_def,
                &request_msg,
                security_context.as_ref(),
                batch_size,
            );

            let body = http_body_util::StreamBody::new(body_stream);
            let mut response = http::Response::new(TonicBody::new(body));
            response.headers_mut().insert(
                "content-type",
                http::HeaderValue::from_static("application/grpc"),
            );
            return response;
        }

        let response_msg = match &op.kind {
            handler::RpcKind::Query { view_name, returns_list, columns, row_descriptor } => {
                // Look up the type definition.
                let type_def = match self.schema.find_type(&op.type_name) {
                    Some(t) => t,
                    None => return grpc_error_response(tonic::Code::Internal, &format!("Type '{}' not found in schema", op.type_name)),
                };

                let rows = match handler::execute_grpc_query(
                    self.adapter.as_ref(),
                    view_name,
                    columns,
                    *returns_list,
                    &request_msg,
                    type_def,
                    security_context.as_ref(),
                ).await {
                    Ok(rows) => rows,
                    Err(FraiseQLError::Validation { message, .. }) => {
                        return grpc_error_response(tonic::Code::InvalidArgument, &message);
                    },
                    Err(FraiseQLError::Unsupported { message }) => {
                        return grpc_error_response(tonic::Code::Unimplemented, &message);
                    },
                    Err(e) => return grpc_error_response(tonic::Code::Internal, &e.to_string()),
                };

                debug!(method = %method, row_count = rows.len(), "gRPC query returned results");

                handler::encode_response(rows, columns, *returns_list, row_descriptor, &op.response_descriptor)
            },
            handler::RpcKind::ServerStream { .. } => {
                // Handled above — unreachable.
                unreachable!("ServerStream handled above");
            },
            handler::RpcKind::Mutation { function_name } => {
                let result = match handler::execute_grpc_mutation(
                    self.adapter.as_ref(),
                    function_name,
                    &request_msg,
                ).await {
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
        use prost::Message as _;
        let response_bytes = response_msg.encode_to_vec();
        let mut framed = Vec::with_capacity(5 + response_bytes.len());
        framed.push(0); // no compression
        framed.extend_from_slice(&(u32::try_from(response_bytes.len()).unwrap_or(u32::MAX)).to_be_bytes());
        framed.extend_from_slice(&response_bytes);

        let mut response = http::Response::new(TonicBody::new(axum::body::Body::from(framed)));
        response.headers_mut().insert(
            "content-type",
            http::HeaderValue::from_static("application/grpc"),
        );
        // gRPC trailers: status OK
        response.headers_mut().insert(
            "grpc-status",
            http::HeaderValue::from_static("0"),
        );
        response
    }
}

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> DynamicGrpcService<A> {
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
    async fn authenticate(
        &self,
        auth_header: Option<String>,
        request_id: String,
    ) -> std::result::Result<Option<SecurityContext>, http::Response<TonicBody>> {
        let validator = match self.oidc_validator.as_ref() {
            Some(v) => v,
            None => return Ok(None), // Auth not configured — allow anonymous access.
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
                Ok(Some(SecurityContext::from_user(user, request_id)))
            },
            Err(e) => {
                warn!(error = %e, "gRPC token validation failed");
                Err(grpc_error_response(
                    tonic::Code::Unauthenticated,
                    "Invalid or expired token",
                ))
            },
        }
    }
}

/// Build an HTTP response with a gRPC error status.
fn grpc_error_response(code: tonic::Code, message: &str) -> http::Response<TonicBody> {
    let mut response = http::Response::new(TonicBody::empty());
    response.headers_mut().insert(
        "content-type",
        http::HeaderValue::from_static("application/grpc"),
    );
    response.headers_mut().insert(
        "grpc-status",
        http::HeaderValue::from(code as i32),
    );
    if let Ok(msg) = http::HeaderValue::from_str(message) {
        response.headers_mut().insert("grpc-message", msg);
    }
    response
}

/// Implement the [`tower::Service`] trait for routing gRPC requests.
impl<A: DatabaseAdapter + Clone + Send + Sync + 'static>
    tower::Service<http::Request<TonicBody>> for DynamicGrpcService<A>
{
    type Response = http::Response<TonicBody>;
    type Error = Infallible;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

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
pub fn build_grpc_service<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    schema: Arc<CompiledSchema>,
    adapter: Arc<A>,
    oidc_validator: Option<Arc<OidcValidator>>,
    rate_limiter: Option<Arc<RateLimiter>>,
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
    let service_name = pool
        .services()
        .next()
        .map(|s| s.full_name().to_string())
        .ok_or_else(|| {
            FraiseQLError::validation(
                "No gRPC service found in descriptor pool".to_string(),
            )
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
            handler::RpcKind::Query { view_name, columns, returns_list, .. } => {
                debug!(
                    method = %method,
                    view = %view_name,
                    columns = columns.len(),
                    list = returns_list,
                    "Registered gRPC query RPC"
                );
            },
            handler::RpcKind::ServerStream { view_name, columns, .. } => {
                debug!(
                    method = %method,
                    view = %view_name,
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
        adapter,
        schema,
        dispatch: Arc::new(dispatch),
        pool: Arc::new(pool),
        service_name: service_name.clone().into(),
        oidc_validator,
        rate_limiter,
    };

    Ok(Some(GrpcServices {
        service,
        reflection_descriptor_bytes,
        service_name,
    }))
}

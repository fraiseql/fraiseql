//! gRPC transport — row-shaped view queries via protobuf wire encoding.
//!
//! This module implements a tonic gRPC service that accepts protobuf
//! requests, translates filters into a WHERE clause via
//! [`GenericWhereGenerator`], calls [`DatabaseAdapter::execute_row_query()`],
//! and encodes the resulting [`ColumnValue`] rows into protobuf responses.
//!
//! The service is built dynamically from the compiled schema's descriptor pool
//! at server startup — no generated Rust protobuf code is needed.

pub mod handler;

use std::convert::Infallible;
use std::sync::Arc;

use fraiseql_core::db::traits::DatabaseAdapter;
use fraiseql_core::schema::CompiledSchema;
use fraiseql_error::FraiseQLError;
use prost_reflect::DescriptorPool;
use tonic::body::Body as TonicBody;
use tonic::server::NamedService;
use tracing::{debug, info};

use handler::{RpcDispatchTable, build_dispatch_table};

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
}

impl<A: DatabaseAdapter> Clone for DynamicGrpcService<A> {
    fn clone(&self) -> Self {
        Self {
            adapter:      Arc::clone(&self.adapter),
            schema:       Arc::clone(&self.schema),
            dispatch:     Arc::clone(&self.dispatch),
            pool:         Arc::clone(&self.pool),
            service_name: Arc::clone(&self.service_name),
        }
    }
}

impl<A: DatabaseAdapter> NamedService for DynamicGrpcService<A> {
    const NAME: &'static str = "fraiseql.v1.FraiseQLService";
}

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> DynamicGrpcService<A> {
    /// Handle a unary gRPC request.
    ///
    /// Decodes the request bytes, dispatches to the appropriate query handler,
    /// executes the query, and encodes the response.
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
            Ok(svc.handle_request(&method, req).await)
        })
    }
}

// ---------------------------------------------------------------------------
// Service construction
// ---------------------------------------------------------------------------

/// Build a [`DynamicGrpcService`] from a compiled schema and descriptor file.
///
/// Returns `None` if gRPC is not configured or not enabled.
/// Returns `Some(service, service_name)` on success.
///
/// # Errors
///
/// Returns an error if the descriptor file is invalid or the dispatch table
/// cannot be built.
pub fn build_grpc_service<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    schema: Arc<CompiledSchema>,
    adapter: Arc<A>,
) -> Result<Option<(DynamicGrpcService<A>, String)>, FraiseQLError> {
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
            handler::RpcKind::Mutation { function_name } => {
                debug!(
                    method = %method,
                    function = %function_name,
                    "Registered gRPC mutation RPC"
                );
            },
        }
    }

    let service = DynamicGrpcService {
        adapter,
        schema,
        dispatch: Arc::new(dispatch),
        pool: Arc::new(pool),
        service_name: service_name.clone().into(),
    };

    Ok(Some((service, service_name)))
}

# Phase 9: Interceptors & Custom Handlers

## Objective

Implement a request/response interceptor system and custom handler support that allows extending the FraiseQL runtime with user-defined logic. This includes before/after hooks for GraphQL operations, custom resolvers, and extension points for middleware.

## Dependencies

- Phase 1: Configuration system (TOML parsing)
- Phase 2: Core runtime (HTTP server, middleware)
- Phase 5: Auth runtime (user context)

---

## Section 9.0: Testing Seams and Security Architecture

### 9.0.1 Testing Architecture

All interceptor types are trait-based for easy testing:

```
┌─────────────────────────────────────────────────────────────────┐
│                    InterceptorPipeline                          │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │
│  │HttpRequestInt│  │OperationInt  │  │FieldIntercepr│          │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘          │
└─────────┼─────────────────┼─────────────────┼──────────────────┘
          │                 │                 │
          ▼                 ▼                 ▼
    ┌───────────┐     ┌───────────┐     ┌───────────┐
    │ Handler   │     │ Handler   │     │ Resolver  │
    │  (trait)  │     │  (trait)  │     │  (trait)  │
    └───────────┘     └───────────┘     └───────────┘
```

### 9.0.2 WASM Security Model

**Critical: WASM introduces significant security concerns that must be addressed:**

```rust
// src/scripting/security.rs - WASM Security Configuration
use std::time::Duration;

/// Security configuration for WASM modules
#[derive(Debug, Clone)]
pub struct WasmSecurityConfig {
    /// Maximum memory a module can allocate
    pub max_memory_bytes: u64,

    /// Maximum execution time per call
    pub max_execution_time: Duration,

    /// Maximum stack depth
    pub max_stack_depth: u32,

    /// Whether to allow network access (via host functions)
    pub allow_network: bool,

    /// Whether to allow file system access (via host functions)
    pub allow_filesystem: bool,

    /// Allowed host functions (whitelist)
    pub allowed_host_functions: Vec<String>,

    /// Maximum number of concurrent WASM invocations
    pub max_concurrent_invocations: u32,

    /// Whether to validate WASM module on load
    pub validate_on_load: bool,

    /// Resource limits per invocation
    pub resource_limits: ResourceLimits,
}

impl Default for WasmSecurityConfig {
    fn default() -> Self {
        Self {
            max_memory_bytes: 64 * 1024 * 1024, // 64MB
            max_execution_time: Duration::from_secs(5),
            max_stack_depth: 128,
            allow_network: false,  // Deny by default
            allow_filesystem: false,  // Deny by default
            allowed_host_functions: vec![
                // Only safe, read-only functions
                "log".to_string(),
                "get_context_field".to_string(),
                "hash_string".to_string(),
            ],
            max_concurrent_invocations: 10,
            validate_on_load: true,
            resource_limits: ResourceLimits::default(),
        }
    }
}

/// Per-invocation resource limits
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    /// Maximum fuel (for deterministic execution time)
    pub max_fuel: u64,
    /// Maximum memory pages (64KB each)
    pub max_memory_pages: u32,
    /// Maximum table elements
    pub max_table_elements: u32,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_fuel: 100_000_000, // ~100ms of execution
            max_memory_pages: 1024, // 64MB
            max_table_elements: 10_000,
        }
    }
}

/// WASM module validation result
#[derive(Debug)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub imports: Vec<ImportInfo>,
    pub exports: Vec<ExportInfo>,
}

#[derive(Debug)]
pub struct ImportInfo {
    pub module: String,
    pub name: String,
    pub kind: ImportKind,
}

#[derive(Debug)]
pub enum ImportKind {
    Function,
    Memory,
    Table,
    Global,
}

#[derive(Debug)]
pub struct ExportInfo {
    pub name: String,
    pub kind: ExportKind,
}

#[derive(Debug)]
pub enum ExportKind {
    Function,
    Memory,
    Table,
    Global,
}

/// Validates a WASM module against security policy
pub fn validate_wasm_module(
    wasm_bytes: &[u8],
    config: &WasmSecurityConfig,
) -> ValidationResult {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut imports = Vec::new();
    let mut exports = Vec::new();

    // Parse module using wasmparser
    let parser = wasmparser::Parser::new(0);

    for payload in parser.parse_all(wasm_bytes) {
        match payload {
            Ok(wasmparser::Payload::ImportSection(reader)) => {
                for import in reader {
                    if let Ok(import) = import {
                        let import_name = format!("{}.{}", import.module, import.name);

                        // Check against whitelist
                        if !config.allowed_host_functions.contains(&import_name)
                            && !config.allowed_host_functions.contains(&import.name.to_string())
                        {
                            errors.push(format!(
                                "Disallowed import: {}. Only whitelisted functions are allowed.",
                                import_name
                            ));
                        }

                        imports.push(ImportInfo {
                            module: import.module.to_string(),
                            name: import.name.to_string(),
                            kind: match import.ty {
                                wasmparser::TypeRef::Func(_) => ImportKind::Function,
                                wasmparser::TypeRef::Memory(_) => ImportKind::Memory,
                                wasmparser::TypeRef::Table(_) => ImportKind::Table,
                                wasmparser::TypeRef::Global(_) => ImportKind::Global,
                                _ => ImportKind::Function,
                            },
                        });
                    }
                }
            }
            Ok(wasmparser::Payload::ExportSection(reader)) => {
                for export in reader {
                    if let Ok(export) = export {
                        exports.push(ExportInfo {
                            name: export.name.to_string(),
                            kind: match export.kind {
                                wasmparser::ExternalKind::Func => ExportKind::Function,
                                wasmparser::ExternalKind::Memory => ExportKind::Memory,
                                wasmparser::ExternalKind::Table => ExportKind::Table,
                                wasmparser::ExternalKind::Global => ExportKind::Global,
                                _ => ExportKind::Function,
                            },
                        });
                    }
                }
            }
            Ok(wasmparser::Payload::MemorySection(reader)) => {
                for memory in reader {
                    if let Ok(memory) = memory {
                        // Check memory limits
                        if memory.initial > config.resource_limits.max_memory_pages as u64 {
                            errors.push(format!(
                                "Memory initial size {} exceeds limit {}",
                                memory.initial,
                                config.resource_limits.max_memory_pages
                            ));
                        }
                        if let Some(max) = memory.maximum {
                            if max > config.resource_limits.max_memory_pages as u64 {
                                warnings.push(format!(
                                    "Memory maximum {} exceeds recommended limit {}",
                                    max,
                                    config.resource_limits.max_memory_pages
                                ));
                            }
                        }
                    }
                }
            }
            Err(e) => {
                errors.push(format!("Parse error: {}", e));
            }
            _ => {}
        }
    }

    // Check for required exports
    let required_exports = ["before", "after"];
    for required in required_exports {
        if !exports.iter().any(|e| e.name == required) {
            warnings.push(format!(
                "Missing recommended export '{}'. Module may not function correctly.",
                required
            ));
        }
    }

    ValidationResult {
        is_valid: errors.is_empty(),
        errors,
        warnings,
        imports,
        exports,
    }
}
```

### 9.0.3 Mock Interceptor Implementations

```rust
// src/lifecycle/mock.rs - Mock interceptors for testing
use super::{
    FieldInterceptor, HttpRequestInterceptor, HttpResponseInterceptor,
    InterceptorResult, OperationInterceptor,
};
use crate::context::{FieldContext, OperationContext, RequestContext};
use crate::error::InterceptorError;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Mutex;

/// Mock HTTP request interceptor for testing
pub struct MockHttpRequestInterceptor {
    pub name: String,
    pub calls: Mutex<Vec<RequestContextSnapshot>>,
    pub result: Mutex<InterceptorResult<()>>,
    pub should_modify: Mutex<Option<Box<dyn Fn(&mut RequestContext) + Send + Sync>>>,
}

#[derive(Debug, Clone)]
pub struct RequestContextSnapshot {
    pub request_id: String,
    pub path: String,
    pub method: String,
    pub is_authenticated: bool,
}

impl MockHttpRequestInterceptor {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            calls: Mutex::new(Vec::new()),
            result: Mutex::new(InterceptorResult::Continue(())),
            should_modify: Mutex::new(None),
        }
    }

    /// Configure to abort with error
    pub fn abort_with(self, error: InterceptorError) -> Self {
        *self.result.lock().unwrap() = InterceptorResult::Abort(error);
        self
    }

    /// Configure to modify context
    pub fn modify_with<F: Fn(&mut RequestContext) + Send + Sync + 'static>(self, f: F) -> Self {
        *self.should_modify.lock().unwrap() = Some(Box::new(f));
        self
    }

    /// Get call count
    pub fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }

    /// Assert interceptor was called
    pub fn assert_called(&self) {
        assert!(self.call_count() > 0, "Expected interceptor to be called");
    }

    /// Assert interceptor was called with specific path
    pub fn assert_called_with_path(&self, path: &str) {
        let calls = self.calls.lock().unwrap();
        assert!(
            calls.iter().any(|c| c.path == path),
            "Expected call with path '{}', got: {:?}",
            path,
            calls.iter().map(|c| &c.path).collect::<Vec<_>>()
        );
    }
}

#[async_trait]
impl HttpRequestInterceptor for MockHttpRequestInterceptor {
    fn name(&self) -> &str {
        &self.name
    }

    async fn intercept(&self, ctx: &mut RequestContext) -> InterceptorResult<()> {
        // Record call
        self.calls.lock().unwrap().push(RequestContextSnapshot {
            request_id: ctx.request_id.to_string(),
            path: ctx.path.clone(),
            method: ctx.method.clone(),
            is_authenticated: ctx.is_authenticated(),
        });

        // Apply modification if configured
        if let Some(modifier) = &*self.should_modify.lock().unwrap() {
            modifier(ctx);
        }

        // Return configured result
        match &*self.result.lock().unwrap() {
            InterceptorResult::Continue(_) => InterceptorResult::Continue(()),
            InterceptorResult::Return(_) => InterceptorResult::Return(()),
            InterceptorResult::Abort(e) => InterceptorResult::Abort(InterceptorError::Internal(
                format!("Mock abort: {:?}", e)
            )),
        }
    }
}

/// Mock operation interceptor for testing
pub struct MockOperationInterceptor {
    pub name: String,
    pub before_calls: Mutex<Vec<OperationSnapshot>>,
    pub after_calls: Mutex<Vec<(OperationSnapshot, Value)>>,
    pub before_result: Mutex<InterceptorResult<()>>,
    pub after_result: Mutex<Option<Value>>,
}

#[derive(Debug, Clone)]
pub struct OperationSnapshot {
    pub operation_type: String,
    pub operation_name: Option<String>,
    pub query: String,
}

impl MockOperationInterceptor {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            before_calls: Mutex::new(Vec::new()),
            after_calls: Mutex::new(Vec::new()),
            before_result: Mutex::new(InterceptorResult::Continue(())),
            after_result: Mutex::new(None),
        }
    }

    /// Configure to transform result
    pub fn transform_result(self, value: Value) -> Self {
        *self.after_result.lock().unwrap() = Some(value);
        self
    }

    /// Assert before was called
    pub fn assert_before_called(&self) {
        assert!(
            !self.before_calls.lock().unwrap().is_empty(),
            "Expected before() to be called"
        );
    }

    /// Assert after was called
    pub fn assert_after_called(&self) {
        assert!(
            !self.after_calls.lock().unwrap().is_empty(),
            "Expected after() to be called"
        );
    }
}

#[async_trait]
impl OperationInterceptor for MockOperationInterceptor {
    fn name(&self) -> &str {
        &self.name
    }

    async fn before(&self, ctx: &mut OperationContext) -> InterceptorResult<()> {
        self.before_calls.lock().unwrap().push(OperationSnapshot {
            operation_type: format!("{:?}", ctx.operation_type),
            operation_name: ctx.operation_name.clone(),
            query: ctx.query.clone(),
        });

        match &*self.before_result.lock().unwrap() {
            InterceptorResult::Continue(_) => InterceptorResult::Continue(()),
            InterceptorResult::Return(_) => InterceptorResult::Return(()),
            InterceptorResult::Abort(e) => InterceptorResult::Abort(InterceptorError::Internal(
                format!("Mock abort: {:?}", e)
            )),
        }
    }

    async fn after(&self, ctx: &OperationContext, result: Value) -> InterceptorResult<Value> {
        self.after_calls.lock().unwrap().push((
            OperationSnapshot {
                operation_type: format!("{:?}", ctx.operation_type),
                operation_name: ctx.operation_name.clone(),
                query: ctx.query.clone(),
            },
            result.clone(),
        ));

        if let Some(transformed) = &*self.after_result.lock().unwrap() {
            InterceptorResult::Continue(transformed.clone())
        } else {
            InterceptorResult::Continue(result)
        }
    }
}

/// Mock field interceptor for testing
pub struct MockFieldInterceptor {
    pub name: String,
    pub type_filter: String,
    pub field_filter: String,
    pub before_calls: Mutex<Vec<FieldSnapshot>>,
    pub after_calls: Mutex<Vec<(FieldSnapshot, Value)>>,
    pub mask_value: Mutex<Option<Value>>,
}

#[derive(Debug, Clone)]
pub struct FieldSnapshot {
    pub type_name: String,
    pub field_name: String,
    pub path: Vec<String>,
}

impl MockFieldInterceptor {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            type_filter: "*".to_string(),
            field_filter: "*".to_string(),
            before_calls: Mutex::new(Vec::new()),
            after_calls: Mutex::new(Vec::new()),
            mask_value: Mutex::new(None),
        }
    }

    pub fn for_field(mut self, type_name: &str, field_name: &str) -> Self {
        self.type_filter = type_name.to_string();
        self.field_filter = field_name.to_string();
        self
    }

    pub fn mask_with(self, value: Value) -> Self {
        *self.mask_value.lock().unwrap() = Some(value);
        self
    }

    pub fn assert_intercepted(&self, type_name: &str, field_name: &str) {
        let calls = self.after_calls.lock().unwrap();
        assert!(
            calls.iter().any(|(s, _)| s.type_name == type_name && s.field_name == field_name),
            "Expected field {}.{} to be intercepted",
            type_name, field_name
        );
    }
}

#[async_trait]
impl FieldInterceptor for MockFieldInterceptor {
    fn name(&self) -> &str {
        &self.name
    }

    fn applies_to(&self, type_name: &str, field_name: &str) -> bool {
        (self.type_filter == "*" || self.type_filter == type_name)
            && (self.field_filter == "*" || self.field_filter == field_name)
    }

    async fn before(&self, ctx: &mut FieldContext) -> InterceptorResult<()> {
        self.before_calls.lock().unwrap().push(FieldSnapshot {
            type_name: ctx.type_name.clone(),
            field_name: ctx.field_name.clone(),
            path: ctx.path.clone(),
        });
        InterceptorResult::Continue(())
    }

    async fn after(&self, ctx: &FieldContext, result: Value) -> InterceptorResult<Value> {
        self.after_calls.lock().unwrap().push((
            FieldSnapshot {
                type_name: ctx.type_name.clone(),
                field_name: ctx.field_name.clone(),
                path: ctx.path.clone(),
            },
            result.clone(),
        ));

        if let Some(mask) = &*self.mask_value.lock().unwrap() {
            InterceptorResult::Continue(mask.clone())
        } else {
            InterceptorResult::Continue(result)
        }
    }
}

/// Mock field resolver for testing
pub struct MockFieldResolver {
    pub name: String,
    pub calls: Mutex<Vec<FieldSnapshot>>,
    pub return_value: Mutex<Value>,
}

impl MockFieldResolver {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            calls: Mutex::new(Vec::new()),
            return_value: Mutex::new(Value::Null),
        }
    }

    pub fn returning(self, value: Value) -> Self {
        *self.return_value.lock().unwrap() = value;
        self
    }
}

#[async_trait::async_trait]
impl crate::custom::registry::FieldResolver for MockFieldResolver {
    fn name(&self) -> &str {
        &self.name
    }

    async fn resolve(&self, ctx: &FieldContext) -> Result<Value, InterceptorError> {
        self.calls.lock().unwrap().push(FieldSnapshot {
            type_name: ctx.type_name.clone(),
            field_name: ctx.field_name.clone(),
            path: ctx.path.clone(),
        });
        Ok(self.return_value.lock().unwrap().clone())
    }
}
```

## Crate: `fraiseql-interceptors`

```
crates/fraiseql-interceptors/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── config.rs              # Interceptor configuration
│   ├── context.rs             # Request/operation context
│   ├── lifecycle.rs           # Request lifecycle hooks
│   ├── graphql/
│   │   ├── mod.rs             # GraphQL interceptors
│   │   ├── query.rs           # Query interceptors
│   │   ├── mutation.rs        # Mutation interceptors
│   │   └── subscription.rs    # Subscription interceptors
│   ├── http/
│   │   ├── mod.rs             # HTTP interceptors
│   │   ├── request.rs         # Request interceptors
│   │   └── response.rs        # Response interceptors
│   ├── custom/
│   │   ├── mod.rs             # Custom handlers
│   │   ├── resolver.rs        # Custom field resolvers
│   │   ├── endpoint.rs        # Custom HTTP endpoints
│   │   └── registry.rs        # Handler registry
│   ├── scripting/
│   │   ├── mod.rs             # Scripting engine
│   │   ├── wasm.rs            # WASM runtime
│   │   └── rhai.rs            # Rhai scripting (optional)
│   └── error.rs
└── tests/
    ├── lifecycle_test.rs
    ├── graphql_test.rs
    └── custom_handler_test.rs
```

---

## Step 1: Configuration Types

### 1.1 Interceptor Configuration

```rust
// src/config.rs
use serde::Deserialize;
use std::collections::HashMap;

/// Top-level interceptors configuration
#[derive(Debug, Clone, Deserialize)]
pub struct InterceptorsConfig {
    /// HTTP request/response interceptors
    #[serde(default)]
    pub http: HttpInterceptorsConfig,

    /// GraphQL operation interceptors
    #[serde(default)]
    pub graphql: GraphQLInterceptorsConfig,

    /// Custom handlers
    #[serde(default)]
    pub handlers: HandlersConfig,

    /// Scripting configuration (WASM/Rhai)
    #[serde(default)]
    pub scripting: Option<ScriptingConfig>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct HttpInterceptorsConfig {
    /// Before request processing
    #[serde(default)]
    pub before_request: Vec<InterceptorRef>,

    /// After response is ready
    #[serde(default)]
    pub after_response: Vec<InterceptorRef>,

    /// On error
    #[serde(default)]
    pub on_error: Vec<InterceptorRef>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct GraphQLInterceptorsConfig {
    /// Before query execution
    #[serde(default)]
    pub before_query: Vec<InterceptorRef>,

    /// After query execution
    #[serde(default)]
    pub after_query: Vec<InterceptorRef>,

    /// Before mutation execution
    #[serde(default)]
    pub before_mutation: Vec<InterceptorRef>,

    /// After mutation execution
    #[serde(default)]
    pub after_mutation: Vec<InterceptorRef>,

    /// Before field resolution
    #[serde(default)]
    pub before_field: Vec<FieldInterceptorRef>,

    /// After field resolution
    #[serde(default)]
    pub after_field: Vec<FieldInterceptorRef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InterceptorRef {
    /// Handler name or path
    pub handler: String,

    /// Execution priority (lower = earlier)
    #[serde(default)]
    pub priority: i32,

    /// Condition for running (optional)
    #[serde(default)]
    pub condition: Option<String>,

    /// Configuration passed to handler
    #[serde(default)]
    pub config: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FieldInterceptorRef {
    /// Handler name or path
    pub handler: String,

    /// Type name filter (e.g., "User", "*" for all)
    #[serde(default = "default_type_filter")]
    pub type_name: String,

    /// Field name filter (e.g., "email", "*" for all)
    #[serde(default = "default_field_filter")]
    pub field_name: String,

    /// Configuration passed to handler
    #[serde(default)]
    pub config: HashMap<String, serde_json::Value>,
}

fn default_type_filter() -> String { "*".to_string() }
fn default_field_filter() -> String { "*".to_string() }

#[derive(Debug, Clone, Default, Deserialize)]
pub struct HandlersConfig {
    /// Custom field resolvers
    #[serde(default)]
    pub resolvers: HashMap<String, ResolverConfig>,

    /// Custom HTTP endpoints
    #[serde(default)]
    pub endpoints: HashMap<String, EndpointConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResolverConfig {
    /// Type name
    pub type_name: String,

    /// Field name
    pub field_name: String,

    /// Handler reference
    pub handler: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EndpointConfig {
    /// HTTP path
    pub path: String,

    /// HTTP method(s)
    pub methods: Vec<String>,

    /// Handler reference
    pub handler: String,

    /// Auth required
    #[serde(default)]
    pub auth_required: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "engine", rename_all = "lowercase")]
pub enum ScriptingConfig {
    Wasm(WasmConfig),
    Rhai(RhaiConfig),
}

#[derive(Debug, Clone, Deserialize)]
pub struct WasmConfig {
    /// Directory containing .wasm files
    pub modules_dir: String,

    /// Memory limit in MB
    #[serde(default = "default_wasm_memory")]
    pub memory_limit_mb: u32,

    /// Execution timeout in ms
    #[serde(default = "default_wasm_timeout")]
    pub timeout_ms: u64,
}

fn default_wasm_memory() -> u32 { 64 }
fn default_wasm_timeout() -> u64 { 5000 }

#[derive(Debug, Clone, Deserialize)]
pub struct RhaiConfig {
    /// Directory containing .rhai scripts
    pub scripts_dir: String,

    /// Maximum operations per execution
    #[serde(default = "default_rhai_operations")]
    pub max_operations: u64,
}

fn default_rhai_operations() -> u64 { 100_000 }
```

---

## Step 2: Request Context

### 2.1 Context Types

```rust
// src/context.rs
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

/// Request context available to all interceptors
#[derive(Debug, Clone)]
pub struct RequestContext {
    /// Unique request ID
    pub request_id: Uuid,

    /// Request start time
    pub started_at: std::time::Instant,

    /// Authenticated user (if any)
    pub user: Option<AuthUser>,

    /// Request headers (filtered)
    pub headers: HashMap<String, String>,

    /// Request path
    pub path: String,

    /// Request method
    pub method: String,

    /// Request body (for POST/PUT)
    pub body: Option<Value>,

    /// Custom data store (for passing data between interceptors)
    pub extensions: Arc<parking_lot::RwLock<HashMap<String, Value>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUser {
    pub id: String,
    pub email: Option<String>,
    pub roles: Vec<String>,
    pub claims: HashMap<String, Value>,
}

impl RequestContext {
    pub fn new(request_id: Uuid, path: String, method: String) -> Self {
        Self {
            request_id,
            started_at: std::time::Instant::now(),
            user: None,
            headers: HashMap::new(),
            path,
            method,
            body: None,
            extensions: Arc::new(parking_lot::RwLock::new(HashMap::new())),
        }
    }

    /// Get a value from extensions
    pub fn get_extension<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        let extensions = self.extensions.read();
        extensions.get(key).and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Set a value in extensions
    pub fn set_extension<T: Serialize>(&self, key: &str, value: T) {
        let mut extensions = self.extensions.write();
        if let Ok(v) = serde_json::to_value(value) {
            extensions.insert(key.to_string(), v);
        }
    }

    /// Check if user has a specific role
    pub fn has_role(&self, role: &str) -> bool {
        self.user.as_ref().map(|u| u.roles.contains(&role.to_string())).unwrap_or(false)
    }

    /// Check if user is authenticated
    pub fn is_authenticated(&self) -> bool {
        self.user.is_some()
    }

    /// Get elapsed time since request started
    pub fn elapsed(&self) -> std::time::Duration {
        self.started_at.elapsed()
    }
}

/// GraphQL operation context
#[derive(Debug, Clone)]
pub struct OperationContext {
    /// Parent request context
    pub request: RequestContext,

    /// Operation type
    pub operation_type: OperationType,

    /// Operation name (if named)
    pub operation_name: Option<String>,

    /// GraphQL query string
    pub query: String,

    /// Variables
    pub variables: HashMap<String, Value>,

    /// Selection set (simplified)
    pub selections: Vec<FieldSelection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationType {
    Query,
    Mutation,
    Subscription,
}

#[derive(Debug, Clone)]
pub struct FieldSelection {
    pub type_name: String,
    pub field_name: String,
    pub alias: Option<String>,
    pub arguments: HashMap<String, Value>,
    pub selections: Vec<FieldSelection>,
}

/// Field resolution context
#[derive(Debug, Clone)]
pub struct FieldContext {
    /// Parent operation context
    pub operation: OperationContext,

    /// Current type name
    pub type_name: String,

    /// Current field name
    pub field_name: String,

    /// Field alias (if any)
    pub alias: Option<String>,

    /// Field arguments
    pub arguments: HashMap<String, Value>,

    /// Parent object value
    pub parent: Value,

    /// Current path in the response
    pub path: Vec<String>,
}
```

---

## Step 3: Lifecycle Hooks

### 3.1 Interceptor Trait

```rust
// src/lifecycle.rs
use async_trait::async_trait;
use serde_json::Value;

use crate::context::{FieldContext, OperationContext, RequestContext};
use crate::error::InterceptorError;

/// Result that can modify the flow
#[derive(Debug)]
pub enum InterceptorResult<T> {
    /// Continue with the value
    Continue(T),
    /// Short-circuit and return this response
    Return(T),
    /// Abort with an error
    Abort(InterceptorError),
}

impl<T> InterceptorResult<T> {
    pub fn is_continue(&self) -> bool {
        matches!(self, InterceptorResult::Continue(_))
    }

    pub fn into_value(self) -> Result<T, InterceptorError> {
        match self {
            InterceptorResult::Continue(v) | InterceptorResult::Return(v) => Ok(v),
            InterceptorResult::Abort(e) => Err(e),
        }
    }
}

/// HTTP request interceptor
#[async_trait]
pub trait HttpRequestInterceptor: Send + Sync {
    fn name(&self) -> &str;

    async fn intercept(
        &self,
        ctx: &mut RequestContext,
    ) -> InterceptorResult<()>;
}

/// HTTP response interceptor
#[async_trait]
pub trait HttpResponseInterceptor: Send + Sync {
    fn name(&self) -> &str;

    async fn intercept(
        &self,
        ctx: &RequestContext,
        response: Value,
    ) -> InterceptorResult<Value>;
}

/// GraphQL operation interceptor (query/mutation)
#[async_trait]
pub trait OperationInterceptor: Send + Sync {
    fn name(&self) -> &str;

    async fn before(
        &self,
        ctx: &mut OperationContext,
    ) -> InterceptorResult<()>;

    async fn after(
        &self,
        ctx: &OperationContext,
        result: Value,
    ) -> InterceptorResult<Value>;
}

/// Field resolver interceptor
#[async_trait]
pub trait FieldInterceptor: Send + Sync {
    fn name(&self) -> &str;

    /// Check if this interceptor applies to the field
    fn applies_to(&self, type_name: &str, field_name: &str) -> bool;

    async fn before(
        &self,
        ctx: &mut FieldContext,
    ) -> InterceptorResult<()>;

    async fn after(
        &self,
        ctx: &FieldContext,
        result: Value,
    ) -> InterceptorResult<Value>;
}

/// Error interceptor
#[async_trait]
pub trait ErrorInterceptor: Send + Sync {
    fn name(&self) -> &str;

    async fn handle(
        &self,
        ctx: &RequestContext,
        error: InterceptorError,
    ) -> InterceptorError;
}
```

---

## Step 4: Interceptor Registry

### 4.1 Registry Implementation

```rust
// src/custom/registry.rs
use std::collections::HashMap;
use std::sync::Arc;

use crate::config::InterceptorsConfig;
use crate::error::InterceptorError;
use crate::lifecycle::{
    ErrorInterceptor, FieldInterceptor, HttpRequestInterceptor,
    HttpResponseInterceptor, OperationInterceptor,
};

/// Registry of all interceptors and handlers
pub struct InterceptorRegistry {
    /// HTTP request interceptors (ordered by priority)
    pub http_request: Vec<Arc<dyn HttpRequestInterceptor>>,

    /// HTTP response interceptors (ordered by priority)
    pub http_response: Vec<Arc<dyn HttpResponseInterceptor>>,

    /// Query interceptors (ordered by priority)
    pub query: Vec<Arc<dyn OperationInterceptor>>,

    /// Mutation interceptors (ordered by priority)
    pub mutation: Vec<Arc<dyn OperationInterceptor>>,

    /// Field interceptors
    pub field: Vec<Arc<dyn FieldInterceptor>>,

    /// Error interceptors
    pub error: Vec<Arc<dyn ErrorInterceptor>>,

    /// Custom resolvers by type.field
    pub resolvers: HashMap<String, Arc<dyn FieldResolver>>,

    /// Custom endpoints by path
    pub endpoints: HashMap<String, Arc<dyn EndpointHandler>>,
}

/// Custom field resolver
#[async_trait::async_trait]
pub trait FieldResolver: Send + Sync {
    fn name(&self) -> &str;

    async fn resolve(
        &self,
        ctx: &crate::context::FieldContext,
    ) -> Result<serde_json::Value, InterceptorError>;
}

/// Custom HTTP endpoint handler
#[async_trait::async_trait]
pub trait EndpointHandler: Send + Sync {
    fn name(&self) -> &str;

    async fn handle(
        &self,
        ctx: &crate::context::RequestContext,
        body: Option<serde_json::Value>,
    ) -> Result<EndpointResponse, InterceptorError>;
}

#[derive(Debug)]
pub struct EndpointResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: serde_json::Value,
}

impl InterceptorRegistry {
    pub fn new() -> Self {
        Self {
            http_request: Vec::new(),
            http_response: Vec::new(),
            query: Vec::new(),
            mutation: Vec::new(),
            field: Vec::new(),
            error: Vec::new(),
            resolvers: HashMap::new(),
            endpoints: HashMap::new(),
        }
    }

    /// Register an HTTP request interceptor
    pub fn register_http_request(&mut self, interceptor: Arc<dyn HttpRequestInterceptor>) {
        self.http_request.push(interceptor);
    }

    /// Register an HTTP response interceptor
    pub fn register_http_response(&mut self, interceptor: Arc<dyn HttpResponseInterceptor>) {
        self.http_response.push(interceptor);
    }

    /// Register a query interceptor
    pub fn register_query(&mut self, interceptor: Arc<dyn OperationInterceptor>) {
        self.query.push(interceptor);
    }

    /// Register a mutation interceptor
    pub fn register_mutation(&mut self, interceptor: Arc<dyn OperationInterceptor>) {
        self.mutation.push(interceptor);
    }

    /// Register a field interceptor
    pub fn register_field(&mut self, interceptor: Arc<dyn FieldInterceptor>) {
        self.field.push(interceptor);
    }

    /// Register an error interceptor
    pub fn register_error(&mut self, interceptor: Arc<dyn ErrorInterceptor>) {
        self.error.push(interceptor);
    }

    /// Register a custom resolver
    pub fn register_resolver(
        &mut self,
        type_name: &str,
        field_name: &str,
        resolver: Arc<dyn FieldResolver>,
    ) {
        let key = format!("{}.{}", type_name, field_name);
        self.resolvers.insert(key, resolver);
    }

    /// Register a custom endpoint
    pub fn register_endpoint(&mut self, path: &str, handler: Arc<dyn EndpointHandler>) {
        self.endpoints.insert(path.to_string(), handler);
    }

    /// Get resolver for a field
    pub fn get_resolver(&self, type_name: &str, field_name: &str) -> Option<&Arc<dyn FieldResolver>> {
        let key = format!("{}.{}", type_name, field_name);
        self.resolvers.get(&key)
    }

    /// Get endpoint handler
    pub fn get_endpoint(&self, path: &str) -> Option<&Arc<dyn EndpointHandler>> {
        self.endpoints.get(path)
    }

    /// Get field interceptors that apply to a field
    pub fn get_field_interceptors(
        &self,
        type_name: &str,
        field_name: &str,
    ) -> Vec<&Arc<dyn FieldInterceptor>> {
        self.field
            .iter()
            .filter(|i| i.applies_to(type_name, field_name))
            .collect()
    }
}

impl Default for InterceptorRegistry {
    fn default() -> Self {
        Self::new()
    }
}
```

---

## Step 5: Built-in Interceptors

### 5.1 Logging Interceptor

```rust
// src/graphql/query.rs (example built-in interceptors)
use async_trait::async_trait;
use serde_json::Value;
use tracing::{debug, info, warn};

use crate::context::OperationContext;
use crate::error::InterceptorError;
use crate::lifecycle::{InterceptorResult, OperationInterceptor};

/// Logs all GraphQL operations
pub struct LoggingInterceptor {
    log_variables: bool,
    log_response: bool,
}

impl LoggingInterceptor {
    pub fn new(log_variables: bool, log_response: bool) -> Self {
        Self {
            log_variables,
            log_response,
        }
    }
}

#[async_trait]
impl OperationInterceptor for LoggingInterceptor {
    fn name(&self) -> &str {
        "logging"
    }

    async fn before(&self, ctx: &mut OperationContext) -> InterceptorResult<()> {
        let operation_name = ctx.operation_name.as_deref().unwrap_or("anonymous");

        if self.log_variables && !ctx.variables.is_empty() {
            info!(
                operation_type = ?ctx.operation_type,
                operation_name = %operation_name,
                variables = ?ctx.variables,
                "GraphQL operation started"
            );
        } else {
            info!(
                operation_type = ?ctx.operation_type,
                operation_name = %operation_name,
                "GraphQL operation started"
            );
        }

        InterceptorResult::Continue(())
    }

    async fn after(&self, ctx: &OperationContext, result: Value) -> InterceptorResult<Value> {
        let operation_name = ctx.operation_name.as_deref().unwrap_or("anonymous");
        let elapsed = ctx.request.elapsed();

        if self.log_response {
            debug!(
                operation_name = %operation_name,
                duration_ms = elapsed.as_millis(),
                response = %result,
                "GraphQL operation completed"
            );
        } else {
            info!(
                operation_name = %operation_name,
                duration_ms = elapsed.as_millis(),
                "GraphQL operation completed"
            );
        }

        InterceptorResult::Continue(result)
    }
}

/// Validates that mutations require authentication
pub struct AuthRequiredInterceptor {
    require_for_mutations: bool,
    require_for_queries: bool,
}

impl AuthRequiredInterceptor {
    pub fn new(require_for_mutations: bool, require_for_queries: bool) -> Self {
        Self {
            require_for_mutations,
            require_for_queries,
        }
    }
}

#[async_trait]
impl OperationInterceptor for AuthRequiredInterceptor {
    fn name(&self) -> &str {
        "auth_required"
    }

    async fn before(&self, ctx: &mut OperationContext) -> InterceptorResult<()> {
        let requires_auth = match ctx.operation_type {
            crate::context::OperationType::Mutation => self.require_for_mutations,
            crate::context::OperationType::Query => self.require_for_queries,
            crate::context::OperationType::Subscription => self.require_for_mutations,
        };

        if requires_auth && !ctx.request.is_authenticated() {
            warn!(
                operation_type = ?ctx.operation_type,
                operation_name = ?ctx.operation_name,
                "Unauthorized operation attempt"
            );

            return InterceptorResult::Abort(InterceptorError::Unauthorized(
                "Authentication required".to_string()
            ));
        }

        InterceptorResult::Continue(())
    }

    async fn after(&self, _ctx: &OperationContext, result: Value) -> InterceptorResult<Value> {
        InterceptorResult::Continue(result)
    }
}

/// Rate limits operations per user/IP
pub struct RateLimitInterceptor {
    max_requests: u32,
    window_seconds: u64,
    cache: std::sync::Arc<crate::cache::CacheProvider>,
}

impl RateLimitInterceptor {
    pub fn new(
        max_requests: u32,
        window_seconds: u64,
        cache: std::sync::Arc<dyn crate::cache::CacheProvider>,
    ) -> Self {
        Self {
            max_requests,
            window_seconds,
            cache,
        }
    }
}

#[async_trait]
impl OperationInterceptor for RateLimitInterceptor {
    fn name(&self) -> &str {
        "rate_limit"
    }

    async fn before(&self, ctx: &mut OperationContext) -> InterceptorResult<()> {
        // Get identifier (user ID or IP)
        let identifier = ctx.request.user.as_ref()
            .map(|u| u.id.clone())
            .or_else(|| ctx.request.headers.get("x-forwarded-for").cloned())
            .unwrap_or_else(|| "anonymous".to_string());

        let key = format!("ratelimit:graphql:{}", identifier);

        // Increment counter
        match self.cache.incr(&key, 1).await {
            Ok(count) => {
                // Set TTL on first request
                if count == 1 {
                    let _ = self.cache.expire(
                        &key,
                        std::time::Duration::from_secs(self.window_seconds)
                    ).await;
                }

                if count > self.max_requests as i64 {
                    warn!(
                        identifier = %identifier,
                        count = count,
                        limit = self.max_requests,
                        "Rate limit exceeded"
                    );

                    return InterceptorResult::Abort(InterceptorError::RateLimited {
                        limit: self.max_requests,
                        window: self.window_seconds,
                    });
                }
            }
            Err(e) => {
                // Log but don't block on cache errors
                warn!(error = %e, "Rate limit cache error");
            }
        }

        InterceptorResult::Continue(())
    }

    async fn after(&self, _ctx: &OperationContext, result: Value) -> InterceptorResult<Value> {
        InterceptorResult::Continue(result)
    }
}
```

### 5.2 Field Masking Interceptor

```rust
// src/graphql/field.rs
use async_trait::async_trait;
use serde_json::Value;

use crate::context::FieldContext;
use crate::lifecycle::{FieldInterceptor, InterceptorResult};

/// Masks sensitive fields based on user roles
pub struct FieldMaskingInterceptor {
    /// Fields to mask: type.field -> required roles
    rules: std::collections::HashMap<String, Vec<String>>,
    mask_value: Value,
}

impl FieldMaskingInterceptor {
    pub fn new(rules: std::collections::HashMap<String, Vec<String>>) -> Self {
        Self {
            rules,
            mask_value: Value::String("***REDACTED***".to_string()),
        }
    }

    pub fn with_mask_value(mut self, value: Value) -> Self {
        self.mask_value = value;
        self
    }
}

#[async_trait]
impl FieldInterceptor for FieldMaskingInterceptor {
    fn name(&self) -> &str {
        "field_masking"
    }

    fn applies_to(&self, type_name: &str, field_name: &str) -> bool {
        let key = format!("{}.{}", type_name, field_name);
        self.rules.contains_key(&key)
    }

    async fn before(&self, _ctx: &mut FieldContext) -> InterceptorResult<()> {
        InterceptorResult::Continue(())
    }

    async fn after(&self, ctx: &FieldContext, result: Value) -> InterceptorResult<Value> {
        let key = format!("{}.{}", ctx.type_name, ctx.field_name);

        if let Some(required_roles) = self.rules.get(&key) {
            // Check if user has any of the required roles
            let has_access = ctx.operation.request.user.as_ref()
                .map(|u| required_roles.iter().any(|r| u.roles.contains(r)))
                .unwrap_or(false);

            if !has_access {
                return InterceptorResult::Continue(self.mask_value.clone());
            }
        }

        InterceptorResult::Continue(result)
    }
}

/// Validates field arguments
pub struct ArgumentValidationInterceptor {
    validators: std::collections::HashMap<String, Box<dyn ArgumentValidator>>,
}

#[async_trait]
pub trait ArgumentValidator: Send + Sync {
    fn validate(&self, value: &Value) -> Result<(), String>;
}

impl ArgumentValidationInterceptor {
    pub fn new() -> Self {
        Self {
            validators: std::collections::HashMap::new(),
        }
    }

    pub fn add_validator<V: ArgumentValidator + 'static>(
        &mut self,
        type_field_arg: &str,  // e.g., "Query.users.limit"
        validator: V,
    ) {
        self.validators.insert(type_field_arg.to_string(), Box::new(validator));
    }
}

#[async_trait]
impl FieldInterceptor for ArgumentValidationInterceptor {
    fn name(&self) -> &str {
        "argument_validation"
    }

    fn applies_to(&self, _type_name: &str, _field_name: &str) -> bool {
        true  // Check all fields
    }

    async fn before(&self, ctx: &mut FieldContext) -> InterceptorResult<()> {
        for (arg_name, arg_value) in &ctx.arguments {
            let key = format!("{}.{}.{}", ctx.type_name, ctx.field_name, arg_name);

            if let Some(validator) = self.validators.get(&key) {
                if let Err(e) = validator.validate(arg_value) {
                    return InterceptorResult::Abort(
                        crate::error::InterceptorError::Validation(format!(
                            "Invalid argument '{}': {}",
                            arg_name, e
                        ))
                    );
                }
            }
        }

        InterceptorResult::Continue(())
    }

    async fn after(&self, _ctx: &FieldContext, result: Value) -> InterceptorResult<Value> {
        InterceptorResult::Continue(result)
    }
}

// Example validators
pub struct MaxValueValidator {
    max: i64,
}

impl MaxValueValidator {
    pub fn new(max: i64) -> Self {
        Self { max }
    }
}

impl ArgumentValidator for MaxValueValidator {
    fn validate(&self, value: &Value) -> Result<(), String> {
        if let Some(n) = value.as_i64() {
            if n > self.max {
                return Err(format!("Value {} exceeds maximum {}", n, self.max));
            }
        }
        Ok(())
    }
}
```

---

## Step 6: Custom Handlers

### 6.1 Custom Resolver Example

```rust
// src/custom/resolver.rs
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::context::FieldContext;
use crate::custom::registry::FieldResolver;
use crate::error::InterceptorError;

/// Example: Computed field resolver
pub struct ComputedFieldResolver {
    name: String,
    compute_fn: Box<dyn Fn(&Value) -> Value + Send + Sync>,
}

impl ComputedFieldResolver {
    pub fn new<F>(name: &str, compute_fn: F) -> Self
    where
        F: Fn(&Value) -> Value + Send + Sync + 'static,
    {
        Self {
            name: name.to_string(),
            compute_fn: Box::new(compute_fn),
        }
    }
}

#[async_trait]
impl FieldResolver for ComputedFieldResolver {
    fn name(&self) -> &str {
        &self.name
    }

    async fn resolve(&self, ctx: &FieldContext) -> Result<Value, InterceptorError> {
        Ok((self.compute_fn)(&ctx.parent))
    }
}

/// Example: External API field resolver
pub struct ExternalApiResolver {
    name: String,
    client: reqwest::Client,
    base_url: String,
}

impl ExternalApiResolver {
    pub fn new(name: &str, base_url: &str) -> Self {
        Self {
            name: name.to_string(),
            client: reqwest::Client::new(),
            base_url: base_url.to_string(),
        }
    }
}

#[async_trait]
impl FieldResolver for ExternalApiResolver {
    fn name(&self) -> &str {
        &self.name
    }

    async fn resolve(&self, ctx: &FieldContext) -> Result<Value, InterceptorError> {
        // Get parent ID
        let id = ctx.parent.get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| InterceptorError::Resolver("Missing parent ID".into()))?;

        // Call external API
        let url = format!("{}/{}/{}", self.base_url, ctx.type_name.to_lowercase(), id);

        let response = self.client.get(&url)
            .send()
            .await
            .map_err(|e| InterceptorError::External(format!("API call failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(InterceptorError::External(format!(
                "API returned {}",
                response.status()
            )));
        }

        let data: Value = response.json().await
            .map_err(|e| InterceptorError::External(format!("Invalid JSON: {}", e)))?;

        // Extract the field value
        let field_value = data.get(&ctx.field_name)
            .cloned()
            .unwrap_or(Value::Null);

        Ok(field_value)
    }
}

/// Example: Aggregation resolver (e.g., count, sum)
pub struct AggregationResolver {
    name: String,
    pool: sqlx::PgPool,
}

impl AggregationResolver {
    pub fn new(name: &str, pool: sqlx::PgPool) -> Self {
        Self {
            name: name.to_string(),
            pool,
        }
    }
}

#[async_trait]
impl FieldResolver for AggregationResolver {
    fn name(&self) -> &str {
        &self.name
    }

    async fn resolve(&self, ctx: &FieldContext) -> Result<Value, InterceptorError> {
        // Example: resolve "orderCount" on User type
        if ctx.type_name == "User" && ctx.field_name == "orderCount" {
            let user_id = ctx.parent.get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| InterceptorError::Resolver("Missing user ID".into()))?;

            let (count,): (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM orders WHERE user_id = $1"
            )
            .bind(user_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| InterceptorError::Database(e.to_string()))?;

            return Ok(json!(count));
        }

        Ok(Value::Null)
    }
}
```

### 6.2 Custom Endpoint Example

```rust
// src/custom/endpoint.rs
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::context::RequestContext;
use crate::custom::registry::{EndpointHandler, EndpointResponse};
use crate::error::InterceptorError;

/// Health check endpoint
pub struct HealthEndpoint {
    checks: Vec<Box<dyn HealthCheck>>,
}

#[async_trait]
pub trait HealthCheck: Send + Sync {
    fn name(&self) -> &str;
    async fn check(&self) -> Result<bool, String>;
}

impl HealthEndpoint {
    pub fn new() -> Self {
        Self { checks: Vec::new() }
    }

    pub fn add_check<C: HealthCheck + 'static>(&mut self, check: C) {
        self.checks.push(Box::new(check));
    }
}

#[async_trait]
impl EndpointHandler for HealthEndpoint {
    fn name(&self) -> &str {
        "health"
    }

    async fn handle(
        &self,
        _ctx: &RequestContext,
        _body: Option<Value>,
    ) -> Result<EndpointResponse, InterceptorError> {
        let mut results = HashMap::new();
        let mut all_healthy = true;

        for check in &self.checks {
            let (healthy, message) = match check.check().await {
                Ok(true) => (true, "ok".to_string()),
                Ok(false) => {
                    all_healthy = false;
                    (false, "unhealthy".to_string())
                }
                Err(e) => {
                    all_healthy = false;
                    (false, e)
                }
            };

            results.insert(check.name().to_string(), json!({
                "healthy": healthy,
                "message": message,
            }));
        }

        let status = if all_healthy { 200 } else { 503 };

        Ok(EndpointResponse {
            status,
            headers: HashMap::new(),
            body: json!({
                "status": if all_healthy { "healthy" } else { "unhealthy" },
                "checks": results,
            }),
        })
    }
}

/// Webhook receiver endpoint (for custom webhook handling)
pub struct WebhookReceiverEndpoint {
    handler: Box<dyn Fn(&RequestContext, &Value) -> Result<Value, String> + Send + Sync>,
}

impl WebhookReceiverEndpoint {
    pub fn new<F>(handler: F) -> Self
    where
        F: Fn(&RequestContext, &Value) -> Result<Value, String> + Send + Sync + 'static,
    {
        Self {
            handler: Box::new(handler),
        }
    }
}

#[async_trait]
impl EndpointHandler for WebhookReceiverEndpoint {
    fn name(&self) -> &str {
        "webhook_receiver"
    }

    async fn handle(
        &self,
        ctx: &RequestContext,
        body: Option<Value>,
    ) -> Result<EndpointResponse, InterceptorError> {
        let body = body.unwrap_or(Value::Null);

        match (self.handler)(ctx, &body) {
            Ok(response) => Ok(EndpointResponse {
                status: 200,
                headers: HashMap::new(),
                body: response,
            }),
            Err(e) => Ok(EndpointResponse {
                status: 400,
                headers: HashMap::new(),
                body: json!({ "error": e }),
            }),
        }
    }
}

/// GraphQL introspection endpoint (custom schema serving)
pub struct SchemaEndpoint {
    schema_sdl: String,
}

impl SchemaEndpoint {
    pub fn new(schema_sdl: String) -> Self {
        Self { schema_sdl }
    }
}

#[async_trait]
impl EndpointHandler for SchemaEndpoint {
    fn name(&self) -> &str {
        "schema"
    }

    async fn handle(
        &self,
        _ctx: &RequestContext,
        _body: Option<Value>,
    ) -> Result<EndpointResponse, InterceptorError> {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "text/plain".to_string());

        Ok(EndpointResponse {
            status: 200,
            headers,
            body: Value::String(self.schema_sdl.clone()),
        })
    }
}
```

---

## Step 7: WASM Scripting Runtime

### 7.0 Implementation Strategy (IMPORTANT)

**WASM is complex and should be considered optional for v1.0.**

The WASM runtime introduces significant complexity:
- Memory management between host and guest
- Serialization overhead for context passing
- Security sandboxing requirements
- Debugging difficulties

**Recommended approach:**
1. **v1.0**: Ship native Rust interceptors only (trait-based)
2. **v1.1+**: Add WASM support for user-defined interceptors
3. **Alternative**: Consider Rhai scripting (simpler, embedded) for simple cases

**If WASM is required for v1.0:**
- Use `extism` crate instead of raw `wasmtime` (simpler host-guest interface)
- Pre-define a limited API surface (don't expose full context)
- Limit to specific interceptor points (e.g., field masking only)

```toml
# Cargo.toml - Recommended WASM dependency
[dependencies.extism]
version = "1.0"
optional = true  # Feature-gated

[features]
default = []
wasm = ["dep:extism"]
```

### 7.1 WASM Handler (Simplified with Extism)

```rust
// src/scripting/wasm.rs
// Using extism for simplified host-guest communication
use async_trait::async_trait;
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use wasmtime::{Config, Engine, Instance, Linker, Module, Store, TypedFunc};

use crate::config::WasmConfig;
use crate::context::{FieldContext, OperationContext, RequestContext};
use crate::error::InterceptorError;
use crate::lifecycle::{InterceptorResult, OperationInterceptor};

/// WASM-based interceptor
pub struct WasmInterceptor {
    name: String,
    engine: Engine,
    module: Module,
    memory_limit: u64,
    timeout_ms: u64,
}

impl WasmInterceptor {
    pub fn load(
        name: &str,
        wasm_path: &Path,
        config: &WasmConfig,
    ) -> Result<Self, InterceptorError> {
        let mut engine_config = Config::new();
        engine_config.consume_fuel(true);  // For timeout enforcement

        let engine = Engine::new(&engine_config).map_err(|e| {
            InterceptorError::Script(format!("Failed to create WASM engine: {}", e))
        })?;

        let module = Module::from_file(&engine, wasm_path).map_err(|e| {
            InterceptorError::Script(format!("Failed to load WASM module: {}", e))
        })?;

        Ok(Self {
            name: name.to_string(),
            engine,
            module,
            memory_limit: config.memory_limit_mb as u64 * 1024 * 1024,
            timeout_ms: config.timeout_ms,
        })
    }

    fn create_store(&self) -> Store<WasmState> {
        let mut store = Store::new(&self.engine, WasmState::default());
        store.set_fuel(self.timeout_ms * 1000).unwrap();  // Approximate fuel
        store
    }
}

#[derive(Default)]
struct WasmState {
    // Host state accessible to WASM
}

#[async_trait]
impl OperationInterceptor for WasmInterceptor {
    fn name(&self) -> &str {
        &self.name
    }

    async fn before(&self, ctx: &mut OperationContext) -> InterceptorResult<()> {
        let mut store = self.create_store();

        let linker = Linker::new(&self.engine);
        // TODO: Link host functions (logging, etc.)

        let instance = match linker.instantiate(&mut store, &self.module) {
            Ok(i) => i,
            Err(e) => {
                return InterceptorResult::Abort(InterceptorError::Script(format!(
                    "WASM instantiation failed: {}", e
                )));
            }
        };

        // Call the "before" export
        let before_fn: Option<TypedFunc<(i32,), i32>> = instance
            .get_typed_func(&mut store, "before")
            .ok();

        if let Some(func) = before_fn {
            // Serialize context to WASM memory
            // Call function
            // Handle result
            let _ = func;  // Placeholder
        }

        InterceptorResult::Continue(())
    }

    async fn after(&self, _ctx: &OperationContext, result: Value) -> InterceptorResult<Value> {
        // Similar pattern for "after" function
        InterceptorResult::Continue(result)
    }
}

/// WASM module manager
pub struct WasmManager {
    modules: std::collections::HashMap<String, Arc<WasmInterceptor>>,
    config: WasmConfig,
}

impl WasmManager {
    pub fn new(config: WasmConfig) -> Self {
        Self {
            modules: std::collections::HashMap::new(),
            config,
        }
    }

    pub fn load_all(&mut self) -> Result<(), InterceptorError> {
        let modules_dir = Path::new(&self.config.modules_dir);

        if !modules_dir.exists() {
            return Ok(());
        }

        for entry in std::fs::read_dir(modules_dir).map_err(|e| {
            InterceptorError::Script(format!("Failed to read modules dir: {}", e))
        })? {
            let entry = entry.map_err(|e| {
                InterceptorError::Script(format!("Failed to read entry: {}", e))
            })?;

            let path = entry.path();
            if path.extension().map(|e| e == "wasm").unwrap_or(false) {
                let name = path.file_stem()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default()
                    .to_string();

                let interceptor = WasmInterceptor::load(&name, &path, &self.config)?;
                self.modules.insert(name, Arc::new(interceptor));
            }
        }

        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<Arc<WasmInterceptor>> {
        self.modules.get(name).cloned()
    }
}
```

---

## Step 8: Comprehensive Error Types

### 8.1 Error Codes

```rust
// src/error.rs
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use thiserror::Error;

/// Interceptor error codes for structured error responses
/// Format: IC### where ### is a numeric code
///
/// Ranges:
/// - IC001-IC099: Configuration errors
/// - IC100-IC199: Authentication/Authorization errors
/// - IC200-IC299: Validation errors
/// - IC300-IC399: Execution errors (resolvers, scripts)
/// - IC400-IC499: Rate limiting errors
/// - IC500-IC599: WASM-specific errors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum InterceptorErrorCode {
    // Configuration errors (IC001-IC099)
    /// Missing configuration
    #[serde(rename = "IC001")]
    MissingConfiguration,
    /// Invalid configuration
    #[serde(rename = "IC002")]
    InvalidConfiguration,
    /// Interceptor not found
    #[serde(rename = "IC003")]
    InterceptorNotFound,
    /// Handler not found
    #[serde(rename = "IC004")]
    HandlerNotFound,
    /// Invalid interceptor chain
    #[serde(rename = "IC005")]
    InvalidChain,

    // Auth errors (IC100-IC199)
    /// Unauthorized (no auth)
    #[serde(rename = "IC100")]
    Unauthorized,
    /// Forbidden (insufficient permissions)
    #[serde(rename = "IC101")]
    Forbidden,
    /// Token expired
    #[serde(rename = "IC102")]
    TokenExpired,
    /// Invalid role
    #[serde(rename = "IC103")]
    InvalidRole,

    // Validation errors (IC200-IC299)
    /// Invalid argument value
    #[serde(rename = "IC200")]
    InvalidArgument,
    /// Missing required field
    #[serde(rename = "IC201")]
    MissingField,
    /// Value out of range
    #[serde(rename = "IC202")]
    OutOfRange,
    /// Invalid format
    #[serde(rename = "IC203")]
    InvalidFormat,
    /// Schema violation
    #[serde(rename = "IC204")]
    SchemaViolation,

    // Execution errors (IC300-IC399)
    /// Resolver failed
    #[serde(rename = "IC300")]
    ResolverFailed,
    /// External service error
    #[serde(rename = "IC301")]
    ExternalServiceError,
    /// Database error
    #[serde(rename = "IC302")]
    DatabaseError,
    /// Script execution error
    #[serde(rename = "IC303")]
    ScriptError,
    /// Timeout
    #[serde(rename = "IC304")]
    Timeout,
    /// Aborted by interceptor
    #[serde(rename = "IC305")]
    Aborted,
    /// Internal error
    #[serde(rename = "IC306")]
    InternalError,

    // Rate limiting errors (IC400-IC499)
    /// Rate limited
    #[serde(rename = "IC400")]
    RateLimited,
    /// Concurrent request limit
    #[serde(rename = "IC401")]
    ConcurrentLimitExceeded,
    /// Quota exceeded
    #[serde(rename = "IC402")]
    QuotaExceeded,

    // WASM-specific errors (IC500-IC599)
    /// WASM module load failed
    #[serde(rename = "IC500")]
    WasmLoadFailed,
    /// WASM validation failed
    #[serde(rename = "IC501")]
    WasmValidationFailed,
    /// WASM memory limit exceeded
    #[serde(rename = "IC502")]
    WasmMemoryLimitExceeded,
    /// WASM execution timeout
    #[serde(rename = "IC503")]
    WasmTimeout,
    /// WASM disallowed import
    #[serde(rename = "IC504")]
    WasmDisallowedImport,
    /// WASM sandbox violation
    #[serde(rename = "IC505")]
    WasmSandboxViolation,
}

impl InterceptorErrorCode {
    pub fn docs_url(&self) -> &'static str {
        match self {
            Self::MissingConfiguration => "https://fraiseql.dev/docs/errors/IC001",
            Self::InvalidConfiguration => "https://fraiseql.dev/docs/errors/IC002",
            Self::InterceptorNotFound => "https://fraiseql.dev/docs/errors/IC003",
            Self::HandlerNotFound => "https://fraiseql.dev/docs/errors/IC004",
            Self::InvalidChain => "https://fraiseql.dev/docs/errors/IC005",
            Self::Unauthorized => "https://fraiseql.dev/docs/errors/IC100",
            Self::Forbidden => "https://fraiseql.dev/docs/errors/IC101",
            Self::TokenExpired => "https://fraiseql.dev/docs/errors/IC102",
            Self::InvalidRole => "https://fraiseql.dev/docs/errors/IC103",
            Self::InvalidArgument => "https://fraiseql.dev/docs/errors/IC200",
            Self::MissingField => "https://fraiseql.dev/docs/errors/IC201",
            Self::OutOfRange => "https://fraiseql.dev/docs/errors/IC202",
            Self::InvalidFormat => "https://fraiseql.dev/docs/errors/IC203",
            Self::SchemaViolation => "https://fraiseql.dev/docs/errors/IC204",
            Self::ResolverFailed => "https://fraiseql.dev/docs/errors/IC300",
            Self::ExternalServiceError => "https://fraiseql.dev/docs/errors/IC301",
            Self::DatabaseError => "https://fraiseql.dev/docs/errors/IC302",
            Self::ScriptError => "https://fraiseql.dev/docs/errors/IC303",
            Self::Timeout => "https://fraiseql.dev/docs/errors/IC304",
            Self::Aborted => "https://fraiseql.dev/docs/errors/IC305",
            Self::InternalError => "https://fraiseql.dev/docs/errors/IC306",
            Self::RateLimited => "https://fraiseql.dev/docs/errors/IC400",
            Self::ConcurrentLimitExceeded => "https://fraiseql.dev/docs/errors/IC401",
            Self::QuotaExceeded => "https://fraiseql.dev/docs/errors/IC402",
            Self::WasmLoadFailed => "https://fraiseql.dev/docs/errors/IC500",
            Self::WasmValidationFailed => "https://fraiseql.dev/docs/errors/IC501",
            Self::WasmMemoryLimitExceeded => "https://fraiseql.dev/docs/errors/IC502",
            Self::WasmTimeout => "https://fraiseql.dev/docs/errors/IC503",
            Self::WasmDisallowedImport => "https://fraiseql.dev/docs/errors/IC504",
            Self::WasmSandboxViolation => "https://fraiseql.dev/docs/errors/IC505",
        }
    }

    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::Timeout
                | Self::RateLimited
                | Self::ConcurrentLimitExceeded
                | Self::ExternalServiceError
                | Self::WasmTimeout
        )
    }

    pub fn is_security_error(&self) -> bool {
        matches!(
            self,
            Self::Unauthorized
                | Self::Forbidden
                | Self::TokenExpired
                | Self::WasmDisallowedImport
                | Self::WasmSandboxViolation
        )
    }
}
```

### 8.2 Error Type with HTTP Response Mapping

```rust
// src/error.rs (continued)

#[derive(Error, Debug)]
pub enum InterceptorError {
    #[error("Configuration error: {message}")]
    Configuration {
        code: InterceptorErrorCode,
        message: String,
    },

    #[error("Unauthorized: {message}")]
    Unauthorized {
        code: InterceptorErrorCode,
        message: String,
    },

    #[error("Forbidden: {message}")]
    Forbidden {
        code: InterceptorErrorCode,
        message: String,
        required_roles: Option<Vec<String>>,
    },

    #[error("Validation error: {message}")]
    Validation {
        code: InterceptorErrorCode,
        message: String,
        field: Option<String>,
    },

    #[error("Rate limited: {limit} requests per {window} seconds")]
    RateLimited {
        limit: u32,
        window: u64,
    },

    #[error("Resolver error: {message}")]
    Resolver {
        code: InterceptorErrorCode,
        message: String,
        resolver_name: Option<String>,
    },

    #[error("External service error: {message}")]
    External {
        code: InterceptorErrorCode,
        message: String,
        service: Option<String>,
    },

    #[error("Database error: {message}")]
    Database {
        message: String,
    },

    #[error("Script error: {message}")]
    Script {
        code: InterceptorErrorCode,
        message: String,
        script_name: Option<String>,
    },

    #[error("Timeout: operation took too long")]
    Timeout,

    #[error("WASM error: {message}")]
    Wasm {
        code: InterceptorErrorCode,
        message: String,
        module_name: Option<String>,
    },

    #[error("Internal error: {message}")]
    Internal {
        message: String,
    },
}

impl InterceptorError {
    pub fn code(&self) -> InterceptorErrorCode {
        match self {
            Self::Configuration { code, .. } => *code,
            Self::Unauthorized { code, .. } => *code,
            Self::Forbidden { code, .. } => *code,
            Self::Validation { code, .. } => *code,
            Self::RateLimited { .. } => InterceptorErrorCode::RateLimited,
            Self::Resolver { code, .. } => *code,
            Self::External { code, .. } => *code,
            Self::Database { .. } => InterceptorErrorCode::DatabaseError,
            Self::Script { code, .. } => *code,
            Self::Timeout => InterceptorErrorCode::Timeout,
            Self::Wasm { code, .. } => *code,
            Self::Internal { .. } => InterceptorErrorCode::InternalError,
        }
    }

    // Convenience constructors
    pub fn unauthorized(message: &str) -> Self {
        Self::Unauthorized {
            code: InterceptorErrorCode::Unauthorized,
            message: message.to_string(),
        }
    }

    pub fn forbidden(message: &str, required_roles: Option<Vec<String>>) -> Self {
        Self::Forbidden {
            code: InterceptorErrorCode::Forbidden,
            message: message.to_string(),
            required_roles,
        }
    }

    pub fn invalid_argument(field: &str, message: &str) -> Self {
        Self::Validation {
            code: InterceptorErrorCode::InvalidArgument,
            message: message.to_string(),
            field: Some(field.to_string()),
        }
    }

    pub fn resolver_failed(name: &str, message: &str) -> Self {
        Self::Resolver {
            code: InterceptorErrorCode::ResolverFailed,
            message: message.to_string(),
            resolver_name: Some(name.to_string()),
        }
    }

    pub fn wasm_validation_failed(module: &str, errors: &[String]) -> Self {
        Self::Wasm {
            code: InterceptorErrorCode::WasmValidationFailed,
            message: errors.join("; "),
            module_name: Some(module.to_string()),
        }
    }

    pub fn wasm_disallowed_import(module: &str, import: &str) -> Self {
        Self::Wasm {
            code: InterceptorErrorCode::WasmDisallowedImport,
            message: format!("Disallowed import: {}", import),
            module_name: Some(module.to_string()),
        }
    }

    /// Convert to GraphQL error extensions
    pub fn to_extensions(&self) -> std::collections::HashMap<String, serde_json::Value> {
        let mut ext = std::collections::HashMap::new();
        let code = self.code();

        ext.insert("code".to_string(), serde_json::json!(code));
        ext.insert("docs_url".to_string(), serde_json::json!(code.docs_url()));

        if code.is_transient() {
            ext.insert("retryable".to_string(), serde_json::json!(true));
        }

        if let Self::RateLimited { limit, window } = self {
            ext.insert("retry_after_secs".to_string(), serde_json::json!(window));
            ext.insert("limit".to_string(), serde_json::json!(limit));
        }

        if let Self::Forbidden { required_roles: Some(roles), .. } = self {
            ext.insert("required_roles".to_string(), serde_json::json!(roles));
        }

        ext
    }
}

/// JSON error response body
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: ErrorBody,
}

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: InterceptorErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_secs: Option<u64>,
    pub docs_url: &'static str,
}

impl IntoResponse for InterceptorError {
    fn into_response(self) -> Response {
        let code = self.code();

        let (status, body) = match &self {
            InterceptorError::Configuration { message, .. } => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorBody {
                    code,
                    message: message.clone(),
                    field: None,
                    retry_after_secs: None,
                    docs_url: code.docs_url(),
                },
            ),
            InterceptorError::Unauthorized { message, .. } => (
                StatusCode::UNAUTHORIZED,
                ErrorBody {
                    code,
                    message: message.clone(),
                    field: None,
                    retry_after_secs: None,
                    docs_url: code.docs_url(),
                },
            ),
            InterceptorError::Forbidden { message, .. } => (
                StatusCode::FORBIDDEN,
                ErrorBody {
                    code,
                    message: message.clone(),
                    field: None,
                    retry_after_secs: None,
                    docs_url: code.docs_url(),
                },
            ),
            InterceptorError::Validation { message, field, .. } => (
                StatusCode::BAD_REQUEST,
                ErrorBody {
                    code,
                    message: message.clone(),
                    field: field.clone(),
                    retry_after_secs: None,
                    docs_url: code.docs_url(),
                },
            ),
            InterceptorError::RateLimited { window, .. } => (
                StatusCode::TOO_MANY_REQUESTS,
                ErrorBody {
                    code,
                    message: "Rate limit exceeded".to_string(),
                    field: None,
                    retry_after_secs: Some(*window),
                    docs_url: code.docs_url(),
                },
            ),
            InterceptorError::Resolver { message, .. } => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorBody {
                    code,
                    message: message.clone(),
                    field: None,
                    retry_after_secs: None,
                    docs_url: code.docs_url(),
                },
            ),
            InterceptorError::External { message, .. } => (
                StatusCode::BAD_GATEWAY,
                ErrorBody {
                    code,
                    message: message.clone(),
                    field: None,
                    retry_after_secs: None,
                    docs_url: code.docs_url(),
                },
            ),
            InterceptorError::Database { message } => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorBody {
                    code,
                    message: message.clone(),
                    field: None,
                    retry_after_secs: None,
                    docs_url: code.docs_url(),
                },
            ),
            InterceptorError::Script { message, .. } => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorBody {
                    code,
                    message: message.clone(),
                    field: None,
                    retry_after_secs: None,
                    docs_url: code.docs_url(),
                },
            ),
            InterceptorError::Timeout => (
                StatusCode::GATEWAY_TIMEOUT,
                ErrorBody {
                    code,
                    message: "Operation timed out".to_string(),
                    field: None,
                    retry_after_secs: Some(1),
                    docs_url: code.docs_url(),
                },
            ),
            InterceptorError::Wasm { message, .. } => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorBody {
                    code,
                    message: message.clone(),
                    field: None,
                    retry_after_secs: None,
                    docs_url: code.docs_url(),
                },
            ),
            InterceptorError::Internal { message } => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorBody {
                    code,
                    message: message.clone(),
                    field: None,
                    retry_after_secs: None,
                    docs_url: code.docs_url(),
                },
            ),
        };

        let mut response = (status, Json(ErrorResponse { error: body })).into_response();

        // Add Retry-After header for rate limiting
        if let InterceptorError::RateLimited { window, .. } = &self {
            response.headers_mut().insert(
                "Retry-After",
                window.to_string().parse().unwrap(),
            );
        }

        // Log security errors
        if code.is_security_error() {
            tracing::warn!(
                error_code = ?code,
                message = %self,
                "Security error occurred"
            );
        }

        response
    }
}
```

---

## Step 9: Integration with Runtime

### 9.1 Middleware Integration

```rust
// Example integration with Axum
use axum::{
    body::Body,
    http::{Request, Response},
    middleware::Next,
};
use std::sync::Arc;

use crate::context::RequestContext;
use crate::custom::registry::InterceptorRegistry;
use crate::lifecycle::InterceptorResult;

/// Axum middleware that runs HTTP interceptors
pub async fn interceptor_middleware(
    request: Request<Body>,
    next: Next,
    registry: Arc<InterceptorRegistry>,
) -> Result<Response<Body>, axum::http::StatusCode> {
    let request_id = uuid::Uuid::new_v4();
    let path = request.uri().path().to_string();
    let method = request.method().to_string();

    let mut ctx = RequestContext::new(request_id, path, method);

    // Extract headers
    for (name, value) in request.headers() {
        if let Ok(v) = value.to_str() {
            ctx.headers.insert(name.to_string(), v.to_string());
        }
    }

    // Run before_request interceptors
    for interceptor in &registry.http_request {
        match interceptor.intercept(&mut ctx).await {
            InterceptorResult::Continue(_) => continue,
            InterceptorResult::Return(_) => {
                // Build early response
                return Ok(Response::new(Body::empty()));
            }
            InterceptorResult::Abort(e) => {
                tracing::error!(error = %e, "Request interceptor aborted");
                return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    }

    // Store context for later use
    let mut request = request;
    request.extensions_mut().insert(ctx.clone());

    // Continue to handler
    let response = next.run(request).await;

    // Run after_response interceptors
    // (would need response body handling for modifications)

    Ok(response)
}
```

---

## Step 10: Comprehensive Unit Tests

### 10.1 Mock Interceptor Tests

```rust
// tests/mock_test.rs
use fraiseql_interceptors::{
    context::{RequestContext, OperationContext, OperationType},
    lifecycle::{MockHttpRequestInterceptor, MockOperationInterceptor, MockFieldInterceptor},
    error::InterceptorError,
};
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn test_mock_http_interceptor_records_calls() {
    let interceptor = MockHttpRequestInterceptor::new("test");

    let mut ctx = RequestContext::new(
        Uuid::new_v4(),
        "/graphql".to_string(),
        "POST".to_string(),
    );

    interceptor.intercept(&mut ctx).await;
    interceptor.intercept(&mut ctx).await;

    assert_eq!(interceptor.call_count(), 2);
    interceptor.assert_called();
    interceptor.assert_called_with_path("/graphql");
}

#[tokio::test]
async fn test_mock_http_interceptor_can_modify_context() {
    let interceptor = MockHttpRequestInterceptor::new("modifier")
        .modify_with(|ctx| {
            ctx.set_extension("custom_key", "custom_value");
        });

    let mut ctx = RequestContext::new(
        Uuid::new_v4(),
        "/graphql".to_string(),
        "POST".to_string(),
    );

    interceptor.intercept(&mut ctx).await;

    let value: Option<String> = ctx.get_extension("custom_key");
    assert_eq!(value, Some("custom_value".to_string()));
}

#[tokio::test]
async fn test_mock_operation_interceptor_records_before_and_after() {
    let interceptor = MockOperationInterceptor::new("logging");

    let request_ctx = RequestContext::new(
        Uuid::new_v4(),
        "/graphql".to_string(),
        "POST".to_string(),
    );

    let mut op_ctx = OperationContext {
        request: request_ctx,
        operation_type: OperationType::Query,
        operation_name: Some("GetUser".to_string()),
        query: "query GetUser { user { id } }".to_string(),
        variables: Default::default(),
        selections: vec![],
    };

    interceptor.before(&mut op_ctx).await;
    let result = json!({"data": {"user": {"id": "1"}}});
    interceptor.after(&op_ctx, result).await;

    interceptor.assert_before_called();
    interceptor.assert_after_called();
}

#[tokio::test]
async fn test_mock_operation_interceptor_transforms_result() {
    let transformed = json!({"data": {"user": {"id": "transformed"}}});
    let interceptor = MockOperationInterceptor::new("transformer")
        .transform_result(transformed.clone());

    let request_ctx = RequestContext::new(
        Uuid::new_v4(),
        "/graphql".to_string(),
        "POST".to_string(),
    );

    let op_ctx = OperationContext {
        request: request_ctx,
        operation_type: OperationType::Query,
        operation_name: None,
        query: "{ user { id } }".to_string(),
        variables: Default::default(),
        selections: vec![],
    };

    let original = json!({"data": {"user": {"id": "original"}}});
    let result = interceptor.after(&op_ctx, original).await;

    match result {
        fraiseql_interceptors::lifecycle::InterceptorResult::Continue(v) => {
            assert_eq!(v, transformed);
        }
        _ => panic!("Expected Continue"),
    }
}

#[tokio::test]
async fn test_mock_field_interceptor_applies_to_filter() {
    let interceptor = MockFieldInterceptor::new("email_masker")
        .for_field("User", "email");

    assert!(interceptor.applies_to("User", "email"));
    assert!(!interceptor.applies_to("User", "name"));
    assert!(!interceptor.applies_to("Post", "email"));
}

#[tokio::test]
async fn test_mock_field_interceptor_masks_value() {
    let interceptor = MockFieldInterceptor::new("ssn_masker")
        .for_field("User", "ssn")
        .mask_with(json!("***-**-****"));

    // ... field context setup and test
}
```

### 10.2 Built-in Interceptor Tests

```rust
// tests/builtin_test.rs
use fraiseql_interceptors::{
    graphql::query::{AuthRequiredInterceptor, LoggingInterceptor},
    context::{AuthUser, OperationContext, OperationType, RequestContext},
    lifecycle::{InterceptorResult, OperationInterceptor},
};
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn test_auth_required_interceptor_blocks_unauthenticated_mutations() {
    let interceptor = AuthRequiredInterceptor::new(true, false);

    let request_ctx = RequestContext::new(
        Uuid::new_v4(),
        "/graphql".to_string(),
        "POST".to_string(),
    );

    let mut op_ctx = OperationContext {
        request: request_ctx,
        operation_type: OperationType::Mutation,
        operation_name: Some("CreateUser".to_string()),
        query: "mutation CreateUser { createUser { id } }".to_string(),
        variables: Default::default(),
        selections: vec![],
    };

    let result = interceptor.before(&mut op_ctx).await;

    assert!(matches!(result, InterceptorResult::Abort(_)));
}

#[tokio::test]
async fn test_auth_required_interceptor_allows_authenticated_mutations() {
    let interceptor = AuthRequiredInterceptor::new(true, false);

    let mut request_ctx = RequestContext::new(
        Uuid::new_v4(),
        "/graphql".to_string(),
        "POST".to_string(),
    );

    request_ctx.user = Some(AuthUser {
        id: "user-1".to_string(),
        email: Some("user@example.com".to_string()),
        roles: vec!["user".to_string()],
        claims: Default::default(),
    });

    let mut op_ctx = OperationContext {
        request: request_ctx,
        operation_type: OperationType::Mutation,
        operation_name: Some("CreateUser".to_string()),
        query: "mutation CreateUser { createUser { id } }".to_string(),
        variables: Default::default(),
        selections: vec![],
    };

    let result = interceptor.before(&mut op_ctx).await;

    assert!(matches!(result, InterceptorResult::Continue(_)));
}

#[tokio::test]
async fn test_logging_interceptor_passes_through() {
    let interceptor = LoggingInterceptor::new(false, false);

    let request_ctx = RequestContext::new(
        Uuid::new_v4(),
        "/graphql".to_string(),
        "POST".to_string(),
    );

    let mut op_ctx = OperationContext {
        request: request_ctx,
        operation_type: OperationType::Query,
        operation_name: Some("GetUser".to_string()),
        query: "query GetUser { user { id } }".to_string(),
        variables: Default::default(),
        selections: vec![],
    };

    let before_result = interceptor.before(&mut op_ctx).await;
    assert!(matches!(before_result, InterceptorResult::Continue(_)));

    let result = json!({"data": {"user": {"id": "1"}}});
    let after_result = interceptor.after(&op_ctx, result.clone()).await;

    match after_result {
        InterceptorResult::Continue(v) => assert_eq!(v, result),
        _ => panic!("Expected Continue"),
    }
}
```

### 10.3 Registry Tests

```rust
// tests/registry_test.rs
use fraiseql_interceptors::{
    custom::registry::{InterceptorRegistry, MockFieldResolver},
    lifecycle::{MockHttpRequestInterceptor, MockOperationInterceptor, MockFieldInterceptor},
};
use serde_json::json;
use std::sync::Arc;

#[test]
fn test_registry_registers_http_interceptors() {
    let mut registry = InterceptorRegistry::new();

    let interceptor = Arc::new(MockHttpRequestInterceptor::new("test"));
    registry.register_http_request(interceptor);

    assert_eq!(registry.http_request.len(), 1);
}

#[test]
fn test_registry_registers_field_interceptors() {
    let mut registry = InterceptorRegistry::new();

    let interceptor = Arc::new(MockFieldInterceptor::new("masker"));
    registry.register_field(interceptor);

    assert_eq!(registry.field.len(), 1);
}

#[test]
fn test_registry_gets_field_interceptors_by_filter() {
    let mut registry = InterceptorRegistry::new();

    let email_masker = Arc::new(
        MockFieldInterceptor::new("email_masker")
            .for_field("User", "email")
    );
    let ssn_masker = Arc::new(
        MockFieldInterceptor::new("ssn_masker")
            .for_field("User", "ssn")
    );
    let all_fields = Arc::new(
        MockFieldInterceptor::new("logger")
        // No filter = applies to all
    );

    registry.register_field(email_masker);
    registry.register_field(ssn_masker);
    registry.register_field(all_fields);

    let email_interceptors = registry.get_field_interceptors("User", "email");
    assert_eq!(email_interceptors.len(), 2); // email_masker + logger

    let ssn_interceptors = registry.get_field_interceptors("User", "ssn");
    assert_eq!(ssn_interceptors.len(), 2); // ssn_masker + logger

    let name_interceptors = registry.get_field_interceptors("User", "name");
    assert_eq!(name_interceptors.len(), 1); // logger only
}

#[test]
fn test_registry_registers_custom_resolvers() {
    let mut registry = InterceptorRegistry::new();

    let resolver = Arc::new(
        MockFieldResolver::new("order_count")
            .returning(json!(42))
    );
    registry.register_resolver("User", "orderCount", resolver);

    let found = registry.get_resolver("User", "orderCount");
    assert!(found.is_some());

    let not_found = registry.get_resolver("User", "notAField");
    assert!(not_found.is_none());
}
```

### 10.4 WASM Security Tests

```rust
// tests/wasm_security_test.rs
use fraiseql_interceptors::scripting::security::{
    WasmSecurityConfig, ResourceLimits, validate_wasm_module,
};

#[test]
fn test_wasm_security_config_defaults() {
    let config = WasmSecurityConfig::default();

    assert_eq!(config.max_memory_bytes, 64 * 1024 * 1024);
    assert!(!config.allow_network);
    assert!(!config.allow_filesystem);
    assert!(config.allowed_host_functions.contains(&"log".to_string()));
}

#[test]
fn test_wasm_validation_detects_disallowed_imports() {
    let config = WasmSecurityConfig::default();

    // A minimal WASM module that imports a disallowed function
    // This is a placeholder - in real tests, use actual WASM bytes
    let wasm_bytes = include_bytes!("fixtures/disallowed_import.wasm");

    let result = validate_wasm_module(wasm_bytes, &config);

    assert!(!result.is_valid);
    assert!(result.errors.iter().any(|e| e.contains("Disallowed import")));
}

#[test]
fn test_wasm_validation_accepts_safe_module() {
    let config = WasmSecurityConfig::default();

    // A minimal WASM module with only allowed imports
    let wasm_bytes = include_bytes!("fixtures/safe_module.wasm");

    let result = validate_wasm_module(wasm_bytes, &config);

    assert!(result.is_valid);
    assert!(result.errors.is_empty());
}

#[test]
fn test_resource_limits_defaults() {
    let limits = ResourceLimits::default();

    assert_eq!(limits.max_fuel, 100_000_000);
    assert_eq!(limits.max_memory_pages, 1024);
    assert_eq!(limits.max_table_elements, 10_000);
}
```

### 10.5 Error Response Tests

```rust
// tests/error_test.rs
use fraiseql_interceptors::error::{InterceptorError, InterceptorErrorCode};
use axum::response::IntoResponse;
use axum::http::StatusCode;

#[tokio::test]
async fn test_error_code_to_http_status_mapping() {
    // Unauthorized -> 401
    let error = InterceptorError::unauthorized("Not logged in");
    assert_eq!(error.into_response().status(), StatusCode::UNAUTHORIZED);

    // Forbidden -> 403
    let error = InterceptorError::forbidden("Admin only", Some(vec!["admin".to_string()]));
    assert_eq!(error.into_response().status(), StatusCode::FORBIDDEN);

    // Validation -> 400
    let error = InterceptorError::invalid_argument("limit", "Must be positive");
    assert_eq!(error.into_response().status(), StatusCode::BAD_REQUEST);

    // Rate limited -> 429
    let error = InterceptorError::RateLimited { limit: 100, window: 60 };
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(response.headers().get("Retry-After").is_some());

    // Timeout -> 504
    let error = InterceptorError::Timeout;
    assert_eq!(error.into_response().status(), StatusCode::GATEWAY_TIMEOUT);
}

#[test]
fn test_error_code_transient_classification() {
    assert!(InterceptorErrorCode::Timeout.is_transient());
    assert!(InterceptorErrorCode::RateLimited.is_transient());
    assert!(InterceptorErrorCode::ExternalServiceError.is_transient());
    assert!(InterceptorErrorCode::WasmTimeout.is_transient());

    assert!(!InterceptorErrorCode::Unauthorized.is_transient());
    assert!(!InterceptorErrorCode::InvalidArgument.is_transient());
    assert!(!InterceptorErrorCode::WasmValidationFailed.is_transient());
}

#[test]
fn test_error_code_security_classification() {
    assert!(InterceptorErrorCode::Unauthorized.is_security_error());
    assert!(InterceptorErrorCode::Forbidden.is_security_error());
    assert!(InterceptorErrorCode::WasmDisallowedImport.is_security_error());
    assert!(InterceptorErrorCode::WasmSandboxViolation.is_security_error());

    assert!(!InterceptorErrorCode::Timeout.is_security_error());
    assert!(!InterceptorErrorCode::InvalidArgument.is_security_error());
}

#[test]
fn test_error_to_graphql_extensions() {
    let error = InterceptorError::RateLimited { limit: 100, window: 60 };
    let extensions = error.to_extensions();

    assert!(extensions.contains_key("code"));
    assert!(extensions.contains_key("retry_after_secs"));
    assert!(extensions.contains_key("limit"));
    assert!(extensions.contains_key("retryable"));
}
```

## Verification Commands

```bash
# Build the crate
cargo build -p fraiseql-interceptors

# Run tests
cargo nextest run -p fraiseql-interceptors

# Lint
cargo clippy -p fraiseql-interceptors -- -D warnings

# Test WASM support
cargo nextest run -p fraiseql-interceptors --features wasm
```

---

## Acceptance Criteria

- [ ] HTTP request/response interceptors work
- [ ] GraphQL query/mutation interceptors work
- [ ] Field interceptors work with type/field filtering
- [ ] Custom resolvers can override field resolution
- [ ] Custom endpoints can be registered and routed
- [ ] Built-in interceptors: logging, auth, rate limiting, field masking
- [ ] [PLACEHOLDER] WASM scripting runtime loads and executes modules - defer to v1.1
- [ ] Error interceptors can transform errors
- [ ] Context is properly passed through all hooks

---

## DO NOT

- Block the event loop in interceptors (use async)
- Allow untrusted WASM modules (sandbox properly)
- Expose internal errors to clients (use error interceptors)
- Skip authentication checks in custom resolvers
- Allow infinite loops in scripts (enforce timeouts)

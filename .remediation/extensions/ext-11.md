# FraiseQL Remediation Plan — Extension 11

**Assessor note**: This extension covers findings not addressed in extensions 1–10.
The following tracks (A–H) were exhausted in prior plans and are excluded here.
Benchmarking is out of scope (handled by `velocitybench`).

---

## Rapport d'étonnement — new findings

### What works well

- The recent batch of commits (`test(wire)`, `test(observers)`, `test(doctests)`) shows genuine
  momentum on test quality. 35 WhereOperator SQL compliance tests and 19 observer transport
  integration tests are the right kind of investment.
- The `in_memory.rs` switch from unbounded to bounded MPSC channel (latest diff) is a thoughtful
  correctness fix — the original unbounded channel was an unacknowledged footgun for backpressure
  tests. The diff also correctly removes the `#[allow(unused_variables)]` lint suppression.
- The `failover.rs` fix (replacing "In production, would transition state here" with a real state
  transition) is exactly the right kind of archaeology removal.
- The `child_span_id` fix (random UUID bytes instead of sequential counter) is cryptographically
  correct; the old counter-based span ID was not W3C-compliant.
- The `async_validators` rename from `MockEmail*`/`MockPhone*` to `EmailFormatValidator`/
  `PhoneE164Validator` is a genuine improvement: these are now real validators, not mocks.

### What surprised us (the bad and the ugly)

The items below are **not** covered by extensions 1–10.

---

## Track I — Security Correctness (Critical)

### I1 — MCP HTTP endpoint: `require_auth` flag is a no-op

**File**: `crates/fraiseql-server/src/mcp/handler.rs` and
`crates/fraiseql-server/src/server/routing.rs:349–370`

**Observation**:
`McpConfig` has a `require_auth: bool` field (default `true`, defined in
`crates/fraiseql-core/src/schema/config_types.rs:341`). Its documentation says "Require
authentication for MCP requests." However:

1. In `routing.rs`, the service is mounted unconditionally:
   ```rust
   app = app.nest_service(&mcp_cfg.path, mcp_service);
   ```
   The `mcp_cfg.require_auth` value is never read here.

2. In `handler.rs`, the `call_tool` implementation discards the request context entirely:
   ```rust
   fn call_tool(
       &self,
       request: CallToolRequestParams,
       _context: RequestContext<rmcp::RoleServer>,  // ← discarded
   ) -> ...
   ```
   No auth token is extracted, no JWT is validated, no scope is checked.

3. The `_config: McpConfig` field in `FraiseQLMcpService` is prefixed with `_`, confirming
   the config is intentionally ignored at runtime.

**Impact**: Any client that can reach the MCP HTTP endpoint (`/mcp` by default) can invoke
all exposed queries and mutations with no authentication — even when `require_auth = true`
is set in `fraiseql.toml`. This is a complete auth bypass for the MCP surface.

**Fix**: Either:
- Read `require_auth` in `routing.rs` and wrap the MCP service with the existing OIDC/JWT
  middleware before mounting, OR
- Gate `require_auth` inside `call_tool` by extracting the bearer token from `_context`
  and validating it against the OIDC validator.

**Related prior finding**: Extension 1 (E1) documented a GET handler OIDC bypass. This is
the same class of problem in the MCP subsystem.

---

### I2 — `escape_identifier` silently passes through unsafe SQL identifiers

**File**: `crates/fraiseql-core/src/db/projection_generator.rs:180–188`

**Observation**:
```rust
fn escape_identifier(field: &str) -> String {
    if !Self::is_safe_identifier(field) {
        // In production, would reject or sanitize more strictly
        // For now, pass through with warning logged at runtime
        return field.to_string();   // ← no warning logged, no error returned
    }
    field.to_string()
}
```

Both branches return `field.to_string()` unchanged. When an unsafe identifier is detected,
the function neither rejects, nor logs, nor escapes — it silently passes the raw value
through. The caller (`generate_projection_sql`) then interpolates it directly into SQL:

```rust
format!("'{}', \"{}\"->>'{}' ", safe_field, self.jsonb_column, safe_jsonb_key)
```

The comment "In production, would reject or sanitize more strictly" has been in the codebase
for multiple refactoring rounds without being resolved. The function name `escape_identifier`
is actively misleading — no escaping occurs.

**Impact**: If field names that fail `is_safe_identifier` ever reach this path (e.g., through
a schema where a field name contains characters outside `[a-zA-Z0-9_$]`), the raw name is
interpolated into SQL. The validator `is_safe_identifier` does allow `$` (dollar sign), which
is valid in PostgreSQL identifiers but unexpected. More critically, the silent pass-through
means the security guarantee ("only safe identifiers reach SQL") is not enforced at the
code level.

**Fix**: `escape_identifier` should return `Result<String, FraiseQLError>` and return
`Err(FraiseQLError::Validation { ... })` for unsafe identifiers. The call sites in
`generate_projection_sql` should propagate the error. The function should also be renamed
to `validate_identifier` to reflect that it does not escape.

---

## Track J — Correctness Gaps

### J1 — `create_validator_from_rule` discards regex compilation errors

**File**: `crates/fraiseql-core/src/validation/validators.rs:226–243`

**Observation**:
```rust
pub fn create_validator_from_rule(rule: &ValidationRule) -> Option<Box<dyn Validator>> {
    match rule {
        ValidationRule::Pattern { pattern, message } => {
            let msg = message.clone().unwrap_or_else(|| "Pattern mismatch".to_string());
            PatternValidator::new(pattern.clone(), msg)
                .ok()                   // ← regex error silently becomes None
                .map(|v| Box::new(v) as Box<dyn Validator>)
        },
        // ...
        _ => None,  // ← "unsupported rule type" is also None
    }
}
```

The function returns `None` for two distinct reasons:
- The rule type is not handled (`_ => None`): caller should log a warning.
- The regex pattern is invalid (`.ok()` drops the `regex::Error`): caller should propagate
  an error to the user, since this is a schema authoring mistake.

Callers receive `None` in both cases and cannot distinguish between them.

**Impact**: If a schema author writes an invalid regex in a `ValidationRule::Pattern`, the
validator is silently skipped. No error, no log. The schema compiles and the field is
effectively unvalidated at runtime.

**Fix**: Change the signature to `Result<Option<Box<dyn Validator>>, FraiseQLError>`:
- `Ok(Some(v))` — validator created successfully.
- `Ok(None)` — rule type not supported (caller may log a debug trace).
- `Err(e)` — regex compilation failed (caller must surface this to the user).

---

### J2 — Pool configuration not validated in `ServerConfig::validate()`

**File**: `crates/fraiseql-server/src/server_config.rs:480–579`

**Observation**:
`ServerConfig::validate()` checks metrics tokens, admin tokens, OIDC configuration, TLS
file paths, and SSL mode strings. It does not validate pool configuration:

```toml
pool_min_size = 50   # larger than max — silently accepted
pool_max_size = 10
pool_timeout_secs = 0  # immediate timeout — accepted
```

These values are passed directly to `sqlx::PgPoolOptions`:
```rust
// crates/fraiseql-server/src/main.rs:247–248
.min_connections(config.pool_min_size as u32)
.max_connections(config.pool_max_size as u32)
```

`sqlx` will accept `min > max` and quietly clamp or fail at first connection, not at startup.
A `pool_timeout_secs = 0` means every connection attempt times out immediately.

**Impact**: Misconfigured pool settings fail silently during initialization or under load,
not at startup with a clear error message. Operators debugging a "no connections available"
error will not be guided to the misconfigured `pool_min_size`/`pool_max_size`.

**Fix**: Add to `ServerConfig::validate()`:
```rust
if self.pool_min_size > self.pool_max_size {
    return Err(format!(
        "pool_min_size ({}) must not exceed pool_max_size ({})",
        self.pool_min_size, self.pool_max_size
    ));
}
if self.pool_timeout_secs == 0 {
    return Err("pool_timeout_secs must be greater than 0".to_string());
}
```

---

## Track K — Reliability

### K1 — Trusted documents manifest reload has no HTTP timeout

**File**: `crates/fraiseql-server/src/server/initialization.rs:331`

**Observation**:
The background task that periodically reloads the trusted documents manifest performs an
unbounded HTTP request:
```rust
match reqwest::get(&url).await {
```

`reqwest::get` uses a default client with no configured timeout. If the remote manifest
server stops responding (connection accepted but no data sent), this task will hang
indefinitely, holding a tokio thread.

Compare with `federation/health_checker.rs` where a `reqwest::Client` is constructed with
an explicit `timeout`:
```rust
let client = reqwest::Client::builder()
    // ... timeout is set
```

The manifest reload task uses the free `reqwest::get` shorthand, which bypasses any
timeout configuration.

**Impact**: A slow or malfunctioning manifest CDN can cause the manifest reload background
task to deadlock. In tokio's work-stealing executor this is not catastrophic, but it wastes
a thread and prevents the manifest from being updated.

**Fix**: Replace `reqwest::get(&url)` with a `reqwest::Client` that has a reasonable
timeout (e.g., 10 seconds), consistent with the federation health checker pattern already
present in the codebase.

---

### K2 — `CompiledSchema` has no format version field

**File**: `crates/fraiseql-core/src/schema/compiled.rs:57–150`

**Observation**:
`CompiledSchema` (the on-disk format of `schema.compiled.json`) has no `schema_format_version`,
`compiler_version`, or `fraiseql_version` field. There is no mechanism for the server to
detect that a compiled schema was produced by an incompatible version of `fraiseql-cli`.

If a schema is compiled with `fraiseql-cli` v2.0.0 and later loaded by `fraiseql-server`
v2.1.0 (which adds a mandatory field), the server silently ignores the missing field
(because every new field uses `#[serde(default)]`). The inverse is also possible: a v2.1.0
schema loaded by a v2.0.0 server will silently drop unknown fields.

The CLI does emit a `schema_version` string in its output format
(`crates/fraiseql-cli/src/output_schemas.rs:25`: `"schema_version": "1.0"`), but this is in
the CLI output wrapper, not in `CompiledSchema` itself.

**Impact**: In a multi-environment deployment (e.g., CI compiles with CLI v2.0, production
runs server v2.1), breaking changes in the compiled schema format will manifest as silent
runtime misbehavior rather than a clear "schema format version mismatch" error at startup.

**Fix**: Add a `schema_format_version: String` field to `CompiledSchema` with a non-default
value (e.g., `"2.0"`). The server should fail fast at startup if
`schema_format_version != EXPECTED_FORMAT_VERSION`. The CLI should write the field. This
is a small, targeted change with no behavioral impact for current deployments (matching
versions are the common case).

---

## Track L — Repository Hygiene: SDK Surface

### L1 — Root `.gitignore` `lib/` pattern silently excludes SDK source trees

**File**: `.gitignore:17`

**Observation**:
Line 17 of the root `.gitignore` contains:
```
lib/
```
This entry originates from the Python packaging conventions section (between `.eggs/` and
`lib64/`). However, git applies this pattern recursively — it matches **any** `lib/`
directory anywhere in the repository.

Consequence: the following SDK source directories are silently gitignored:
- `sdks/official/fraiseql-elixir/lib/` — the entire Elixir SDK implementation
- `sdks/official/fraiseql-dart/lib/` — the Dart SDK source
- `sdks/official/fraiseql-ruby/lib/fraiseql/` — the Ruby SDK source

Verified with `git check-ignore -v`:
```
.gitignore:17:lib/    sdks/official/fraiseql-elixir/lib/
.gitignore:17:lib/    sdks/official/fraiseql-dart/lib/
```

The `sdks/official/README.md` lists Elixir, Dart, and Ruby as "Beta" SDKs. The official
Elixir SDK has 5 test files in `sdks/official/fraiseql-elixir/test/` that are tracked,
but the implementation they test lives in `lib/` and is not tracked. These tests will
fail on a fresh clone.

**Impact**: New contributors cloning the repository and running Elixir SDK tests will
encounter failures that have no obvious cause. Any new implementation files added to
`sdks/official/fraiseql-elixir/lib/` will not be committed (no `git add` warning is shown
unless `--verbose` is used).

**Fix**: Scope the `lib/` gitignore entry to Python packaging directories:
```
# Python packaging convention (NOT SDK source directories)
build/lib/
dist/lib/
```
Or simply remove the bare `lib/` line — Python packaging artifacts are already excluded
by `dist/`, `build/`, `*.egg-info/`, and `.eggs/` entries elsewhere in the file.

After fixing, add explicit negative patterns if needed:
```
!sdks/**/lib/
```

---

### L2 — Deprecated community SDKs have no removal timeline

**Directory**: `sdks/community/`

**Observation**:
Nine community SDKs in `sdks/community/` carry `DEPRECATED.md` files:
Clojure, Dart, Elixir, Groovy, Kotlin, Node.js, Ruby, Scala, Swift.

The `DEPRECATED.md` files state the SDKs are not compatible with v2.0.0 but do not give
a removal date or a migration path beyond "use the standard GraphQL HTTP client." The SDKs
continue to accumulate `FEATURE_PARITY.md` files and tests that reference Phase/Cycle
naming (`Phase18Cycle12`), which are development archaeology markers.

Each deprecated SDK also has:
- A test suite that fails on a fresh clone (no CI job runs them).
- References to v1.x schema format in README files.
- Varying states of incompleteness (Groovy and Kotlin have tests referencing `Phase18*`).

**Impact**: The `sdks/community/` directory costs repository size and creates confusion
for developers evaluating FraiseQL. A contributor landing on the repo via GitHub search
may start using a deprecated Clojure or Kotlin SDK. The CI never validates them.

**Fix**: Establish a concrete removal policy in `sdks/community/README.md`, e.g.:
- Deprecated SDKs are removed in the next minor release after deprecation.
- Archive snapshots can be created as git tags (e.g., `sdk/clojure-v1-final`) before deletion.
Remove all 9 deprecated community SDKs from the main branch in a single cleanup commit.

---

## Track M — Arrow Flight Placeholder Data in Production Path

### M1 — `execute_placeholder_query` returns hardcoded fake data via a production code path

**File**: `crates/fraiseql-arrow/src/flight_server/convert.rs:134–175` and
`crates/fraiseql-arrow/src/flight_server/service.rs:542`

**Observation**:
`execute_placeholder_query` generates rows named `"Customer 1"`, `"Customer 2"`, etc. with
hardcoded timestamps, intended as "development/testing" data. The function's docstring says
"This function is only called when no database adapter is configured."

However, it is called at line 542 of `service.rs` as a production code path in `do_get`
when the database adapter is absent. Unlike the backup stubs (extension 1, F1) which
return `Ok(())` silently, this function returns convincing-looking fake data:

```rust
row.insert("customer_name".to_string(), json!(format!("Customer {}", i + 1)));
row.insert("total".to_string(), json!((i as f64 + 1.0) * 99.99));
```

A client consuming the Arrow Flight API without a database adapter configured would receive
this data, believe it is real, and potentially make decisions based on it.

This is similar in nature to the backup (F1) and syslog (F2) stubs identified in extension 1,
but potentially more harmful because the stub returns structured data that looks valid.

**Impact**: An Arrow Flight client receiving hardcoded `va_orders` data from a server
misconfigured without a database adapter has no indicator that the data is synthetic.
Error is preferable to misleading data.

**Fix**: Replace the `execute_placeholder_query` fallback with an explicit error:
```rust
return Err(Status::failed_precondition(
    "No database adapter configured. Arrow Flight requires a database connection."
));
```
The function `execute_placeholder_query` should be moved to `#[cfg(test)]` scope or
deleted entirely.

---

## Priority Order

| ID | Severity | Effort | Description |
|----|----------|--------|-------------|
| I1 | Critical | M | MCP HTTP endpoint auth bypass |
| I2 | High | S | `escape_identifier` silent pass-through |
| J1 | High | S | `create_validator_from_rule` drops errors |
| K1 | Medium | S | Manifest reload HTTP timeout missing |
| K2 | Medium | M | No `schema_format_version` in CompiledSchema |
| J2 | Medium | S | Pool config not validated |
| L1 | Medium | S | `lib/` gitignore blocks SDK source |
| M1 | Medium | S | Arrow Flight placeholder data in production |
| L2 | Low | M | Remove deprecated community SDKs |

Legend: S = hours, M = 1–3 days

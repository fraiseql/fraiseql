# Phase 8.1: TLS Support Implementation Plan

**Status**: Design & Planning
**Target**: Add rustls-based TLS support for secure Postgres connections
**Priority**: 🔴 Critical (v0.2.0 blocker)
**Timeline**: 1-2 weeks
**Effort**: Medium

---

## Objective

Enable secure TLS connections to Postgres, supporting:
- Server certificate validation (default)
- Custom CA certificates
- Optional client certificates
- Both TCP and Unix socket (latter uses cleartext)
- Backward compatible with existing `connect()` API

---

## Design Overview

### Current State

```rust
// Current API (v0.1.0)
let client = FraiseClient::connect("postgres://localhost/db").await?;
```

**Issue**: No TLS support, only cleartext TCP

### Proposed API

```rust
// New API (v0.2.0)
use fraiseql_wire::client::{FraiseClient, TlsConfig};

// Option 1: System CA certificates (most common)
let tls = TlsConfig::builder()
    .verify_hostname(true)
    .build()?;
let client = FraiseClient::connect_tls("postgres://localhost/db", tls).await?;

// Option 2: Custom CA certificate
let tls = TlsConfig::builder()
    .ca_cert_path("/path/to/ca.pem")?
    .verify_hostname(true)
    .build()?;
let client = FraiseClient::connect_tls("postgres://localhost/db", tls).await?;

// Option 3: Development/testing (skip verification)
let tls = TlsConfig::builder()
    .danger_accept_invalid_certs(true) // ⚠️ Development only
    .danger_accept_invalid_hostnames(true)
    .build()?;
let client = FraiseClient::connect_tls("postgres://localhost/db", tls).await?;
```

### Architecture

```
┌─────────────────────┐
│   FraiseClient      │
│  - connect()        │ ← Existing, plaintext only
│  - connect_tls()    │ ← NEW, TLS support
└──────────┬──────────┘
           │
           ├─ ConnectionConfig
           │  └─ transport: Transport (TCP/Unix)
           │     ├─ TCP(host, port, TlsConfig?)
           │     └─ Unix(path)
           │
           └─ Connection
              └─ socket: Socket
                 ├─ TcpStream + TlsStream
                 └─ UnixStream (no TLS)
```

### TLS Library Choice: rustls

**Why rustls?**
- Pure Rust (no OpenSSL/platform dependencies)
- Memory-safe (no unsafe code in rustls)
- Well-maintained and audited
- Cross-platform (Windows, macOS, Linux)
- Good certificate handling

**Dependencies to add:**
```toml
rustls = "0.21"
rustls-pemfile = "2.0"
webpki = "0.22"
webpki-roots = "0.25"  # System root certs
```

**No dependencies on:**
- OpenSSL (platform-specific)
- native-tls (complex platform handling)

---

## Implementation Plan

### Phase 8.1.1: TLS Configuration Module

**File**: `src/connection/tls.rs` (NEW)

```rust
pub struct TlsConfig {
    /// CA certificate path (None = use system roots)
    ca_cert_path: Option<String>,
    /// Whether to verify hostname matches certificate
    verify_hostname: bool,
    /// Whether to accept invalid certificates (dev only)
    danger_accept_invalid_certs: bool,
    /// Whether to accept invalid hostnames (dev only)
    danger_accept_invalid_hostnames: bool,
}

impl TlsConfig {
    pub fn builder() -> TlsConfigBuilder {
        TlsConfigBuilder::default()
    }

    /// Load CA certificate from file
    pub fn load_ca_cert(&self) -> Result<Vec<u8>> { ... }

    /// Build rustls ClientConfig
    pub fn build_client_config(&self) -> Result<rustls::ClientConfig> { ... }
}

pub struct TlsConfigBuilder {
    ca_cert_path: Option<String>,
    verify_hostname: bool,
    danger_accept_invalid_certs: bool,
    danger_accept_invalid_hostnames: bool,
}

impl TlsConfigBuilder {
    pub fn ca_cert_path(mut self, path: impl Into<String>) -> Self { ... }
    pub fn verify_hostname(mut self, verify: bool) -> Self { ... }
    pub fn danger_accept_invalid_certs(mut self, accept: bool) -> Self { ... }
    pub fn danger_accept_invalid_hostnames(mut self, accept: bool) -> Self { ... }
    pub fn build(self) -> Result<TlsConfig> { ... }
}
```

**Responsibilities:**
- Load CA certificate from file or use system roots
- Build rustls::ClientConfig
- Validate configuration (prevent invalid combinations)
- Support both custom CA and system certs

**Tests:**
- [ ] Builder API works
- [ ] Load CA from file
- [ ] Use system roots by default
- [ ] Validate dangerous flag combinations
- [ ] Error on missing CA file

---

### Phase 8.1.2: Transport & Connection Layer

**Files**:
- `src/connection/transport.rs` (NEW)
- `src/connection/socket.rs` (MODIFY)
- `src/client/mod.rs` (MODIFY)

#### Transport Enum

```rust
pub enum Transport {
    /// TCP connection with optional TLS
    Tcp {
        host: String,
        port: u16,
        tls: Option<TlsConfig>,
    },
    /// Unix socket (TLS not supported)
    Unix {
        path: String,
    },
}

impl Transport {
    /// Parse from connection string
    pub fn from_connection_string(conn_str: &str) -> Result<Self> { ... }
}
```

#### Socket Enum (TLS-aware)

```rust
pub enum Socket {
    /// Plain TCP socket
    Tcp(TcpStream),
    /// TLS-wrapped TCP socket
    TcpTls(TlsStream<TcpStream>),
    /// Unix domain socket
    Unix(UnixStream),
}

impl Socket {
    pub async fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> { ... }
    pub async fn write_all(&mut self, buf: &[u8]) -> io::Result<()> { ... }
}
```

#### Connection Flow with TLS

```rust
pub async fn connect_tls(
    conn_str: &str,
    tls: TlsConfig,
) -> Result<FraiseClient> {
    let config = ConnectionConfig::parse(conn_str)?;

    // Create transport with TLS config
    let transport = Transport::Tcp {
        host: config.host,
        port: config.port,
        tls: Some(tls),
    };

    // Establish connection
    let mut socket = Socket::connect(&transport).await?;

    // Perform TLS handshake (automatic with TlsStream)

    // Continue with Postgres startup (unchanged)
    let connection = Connection::startup(&mut socket, &config).await?;

    Ok(FraiseClient { connection })
}
```

**Tests:**
- [ ] TCP connection with TLS
- [ ] TLS handshake succeeds
- [ ] Certificate validation works
- [ ] Invalid certificate rejected
- [ ] Connection string parsing
- [ ] Unix socket still works (no TLS)

---

### Phase 8.1.3: Tests & Examples

#### Test Categories

**Unit Tests** (`tests/tls_unit.rs`):
- [ ] TlsConfig builder
- [ ] CA certificate loading
- [ ] Client config generation
- [ ] Configuration validation

**Integration Tests** (`tests/tls_integration.rs`):
- [ ] Connect with valid certificate
- [ ] Certificate verification works
- [ ] Hostname verification works
- [ ] Invalid certificate rejected
- [ ] Custom CA certificate works
- [ ] System roots work

**Security Tests** (`tests/tls_security.rs`):
- [ ] MITM protection (invalid cert rejected)
- [ ] Hostname verification enforced
- [ ] Dangerous flags require explicit opt-in
- [ ] No credential leakage in errors

#### Example Program

**File**: `examples/tls.rs`

```rust
use fraiseql_wire::client::{FraiseClient, TlsConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Production: Use system CA certificates
    let tls = TlsConfig::builder()
        .verify_hostname(true)
        .build()?;

    let client = FraiseClient::connect_tls(
        "postgres://localhost/db",
        tls
    ).await?;

    // Query works same as before
    // ...

    Ok(())
}
```

---

## Implementation Checklist

### Code Changes
- [ ] Add rustls dependency to Cargo.toml
- [ ] Create `src/connection/tls.rs` with TlsConfig
- [ ] Create `src/connection/transport.rs` with Transport enum
- [ ] Modify `src/connection/socket.rs` for TLS support
- [ ] Add `connect_tls()` method to FraiseClient
- [ ] Update connection string parser (optional `tls://` scheme)

### Tests
- [ ] TlsConfig builder tests
- [ ] Certificate loading tests
- [ ] Connection with TLS tests
- [ ] Certificate validation tests
- [ ] Hostname verification tests
- [ ] Error case tests
- [ ] Integration tests with real Postgres over TLS

### Documentation
- [ ] TlsConfig rustdoc
- [ ] `connect_tls()` rustdoc
- [ ] `examples/tls.rs` with comments
- [ ] Add TLS guide to CONTRIBUTING.md
- [ ] Update README.md with TLS note
- [ ] Add FAQ for common TLS issues

### Benchmarks
- [ ] TLS overhead measurement (< 5% acceptable)
- [ ] Connection setup time with vs without TLS
- [ ] Throughput impact

### CI/CD
- [ ] Add TLS integration tests to GitHub Actions
- [ ] Test with self-signed certificates
- [ ] Performance regression detection

---

## Security Considerations

### Default Security

✅ **Secure by default:**
- Hostname verification enabled by default
- System CA certificates used by default
- Dangerous flags require explicit opt-in with `danger_*` prefix

⚠️ **Development helpers:**
- `danger_accept_invalid_certs()` - for self-signed certs
- `danger_accept_invalid_hostnames()` - for internal testing
- Clear warning in names

### Attack Prevention

**Certificate Validation:**
- ✅ Verify certificate chain
- ✅ Verify hostname matches
- ✅ Prevent MITM attacks

**Credential Protection:**
- ✅ Error messages don't leak credentials
- ✅ Passwords never logged
- ✅ Certificate details safe to log

**Error Handling:**
- ✅ Clear error messages
- ✅ Actionable suggestions
- ✅ No system internals exposed

---

## Backward Compatibility

**Fully backward compatible:**
- Existing `connect()` method unchanged
- All existing APIs work as before
- New `connect_tls()` optional
- Unix sockets unaffected

**Migration path:**
1. v0.2.0: Add `connect_tls()`
2. v0.3.0: Consider deprecating TCP without TLS
3. v1.0.0: TCP could default to TLS

---

## Performance Impact

**Expected overhead:**
- TLS handshake: ~5-10ms per connection
- Per-row throughput: < 1% impact (crypto is fast)
- Memory: Minimal (rustls is efficient)

**Benchmarks to verify:**
- [ ] Handshake time: < 10ms
- [ ] Throughput vs plaintext: > 99%
- [ ] Memory impact: < 1MB additional

---

## Error Handling

**TLS-specific errors:**

```rust
pub enum Error {
    // ... existing variants

    // TLS errors
    TlsError(String), // rustls::Error converted to String
    CertificateNotFound(String),
    CertificateInvalid(String),
    HostnameMismatch {
        expected: String,
        found: String,
    },
    TlsHandshakeFailed(String),
}
```

**Example errors:**
- "Certificate not found at /path/to/ca.pem"
- "Certificate validation failed: self signed certificate"
- "Hostname 'localhost' doesn't match certificate 'prod-db.example.com'"
- "TLS handshake failed: connection reset by peer"

---

## Testing Strategy

### Local Testing

1. **With real Postgres + TLS:**
   ```bash
   # Start Postgres with TLS
   docker run -e POSTGRES_HOST_AUTH_METHOD=trust \
     -v /etc/ssl/certs:/certs \
     postgres:17-alpine

   # Run tests with TLS
   cargo test --test tls_integration
   ```

2. **With self-signed certs (dev):**
   ```bash
   # Generate self-signed cert
   openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem

   # Test with danger_accept_invalid_certs()
   cargo test --test tls_integration
   ```

3. **Error cases:**
   - Invalid CA path
   - Wrong CA certificate
   - Hostname mismatch
   - Connection refused

### CI Testing

GitHub Actions will:
- [ ] Test with Postgres 17 + TLS
- [ ] Test with self-signed certificates
- [ ] Verify certificate validation
- [ ] Benchmark TLS overhead

---

## Success Criteria

✅ **Functionality:**
- [ ] `FraiseClient::connect_tls()` works
- [ ] TLS handshake succeeds
- [ ] All queries work over TLS
- [ ] Certificate validation works
- [ ] Hostname verification works

✅ **Security:**
- [ ] Invalid certificates rejected
- [ ] MITM attacks prevented
- [ ] No credential leaks
- [ ] Secure by default

✅ **Quality:**
- [ ] > 90% test coverage
- [ ] Zero clippy warnings
- [ ] Complete rustdoc
- [ ] Performance < 5% overhead
- [ ] Backward compatible

✅ **Documentation:**
- [ ] API documentation
- [ ] Example program
- [ ] Integration guide
- [ ] Security guide
- [ ] Troubleshooting FAQ

---

## Next Steps (After Planning)

1. **8.1.1**: Create TlsConfig and Transport modules
2. **8.1.2**: Implement TLS socket connection
3. **8.1.3**: Write comprehensive tests
4. **8.1.4**: Create example and documentation
5. **8.1.5**: Performance benchmarking
6. **8.1.6**: Security review
7. **8.1.7**: PR and merge to main

---

## Related Issues & References

- Postgres TLS: https://www.postgresql.org/docs/current/ssl-tcp.html
- rustls docs: https://docs.rs/rustls/latest/rustls/
- Webpki: https://docs.rs/webpki/latest/webpki/
- PHASE_8_PLAN.md: Feature priority and design

---

**Ready to proceed with implementation! 🚀**

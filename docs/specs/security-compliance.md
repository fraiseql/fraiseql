# Security, Compliance, and SBOM Specification

**Status:** Stable
**Version**: 1.0
**Last Updated**: 2026-01-11

## Table of Contents

1. [Overview](#overview)
2. [Security Profiles](#security-profiles)
3. [Software Bill of Materials (SBOM) Generation](#software-bill-of-materials-sbom-generation)
4. [Security Headers](#security-headers)
5. [CSRF Protection](#csrf-protection)
6. [Token Revocation](#token-revocation)
7. [Rate Limiting](#rate-limiting)
8. [Field-Level Authorization](#field-level-authorization)
9. [Introspection Control](#introspection-control)
10. [Key Management Service (KMS)](#key-management-service-kms)
11. [Security Event Logging](#security-event-logging)
12. [Regulatory Compliance Summary](#regulatory-compliance-summary)
13. [Deployment Checklist](#deployment-checklist)
14. [Conclusion](#conclusion)

---

## Overview

FraiseQL provides comprehensive enterprise-grade security and compliance features, including three pre-configured security profiles, Software Bill of Materials (SBOM) generation, cryptographic security controls, and full NIS2/PCI-DSS compliance support. These features work together to provide defense-in-depth security for production GraphQL deployments.

### Key Capabilities

- **Three Security Profiles**: STANDARD, REGULATED, RESTRICTED for different compliance tiers
- **SBOM Generation**: Automatic CycloneDX 1.5 bill of materials for supply chain security
- **NIS2 Compliance**: Full support for EU NIS2 Directive requirements
- **Security Headers**: 11+ security headers (CSP, HSTS, X-Frame-Options, etc.)
- **CSRF Protection**: Token-based CSRF defense with multiple storage backends
- **Token Revocation**: Immediate logout with per-user and all-user revocation
- **Rate Limiting**: Operation-based and complexity-aware rate limiting
- **Field-Level Authorization**: Granular per-field access control
- **Introspection Control**: Schema introspection policies (DISABLED, AUTHENTICATED, ENABLED)
- **KMS Integration**: Multi-provider key management (AWS, GCP, Vault, Local)
- **Security Logging**: Centralized security event logging with multiple event types
- **Audit Logging**: Enterprise audit trail with cryptographic integrity

---

## Security Profiles

FraiseQL provides three pre-configured security profiles that bundle together related security settings appropriate for different compliance tiers. Security profiles enforce middleware configurations, rate limits, introspection policies, and audit levels.

### Profile Comparison

| Aspect | STANDARD | REGULATED | RESTRICTED |
|--------|----------|-----------|-----------|
| **Use Case** | Development, internal APIs | Financial services, healthcare | Government, military, critical infrastructure |
| **TLS** | Optional | **Required** | **Required (v1.3 min)** |
| **mTLS** | No | No | **Yes** |
| **Token Expiry** | 60 minutes | 15 minutes | 5 minutes |
| **Introspection** | AUTHENTICATED | **DISABLED** | **DISABLED** |
| **Rate Limit** | 100 req/min | 50 req/min | 10 req/min |
| **Audit Level** | STANDARD | **ENHANCED** | **VERBOSE** |
| **Field Audit** | Off | **On** | **On** |
| **Query Depth** | 15 levels | 10 levels | 10 levels |
| **Query Complexity** | Unlimited | Limited | **500 max** |
| **Body Size** | No limit | 1 MB | **512 KB** |

### STANDARD Profile (Default)

**Configuration**:

```python
from FraiseQL.security.profiles.definitions import get_profile

profile = get_profile("standard")
config = FraiseQLConfig(
    security_profile=profile,
    database_url="postgresql://localhost/fraiseql_db",
)
```text

**Settings**:

- TLS: Optional (development-friendly)
- mTLS: Not required
- Authentication: Optional (except for introspection)
- Token Expiry: 60 minutes
- Introspection Policy: AUTHENTICATED (requires authentication)
- Rate Limiting: 100 requests/minute per user
- Audit Level: STANDARD (basic event logging)
- Field-Level Audit: Disabled
- Query Depth Limit: 15 levels
- Query Complexity: Unlimited (auto-estimated)

**Use Cases**:

- Development environments
- Internal company APIs
- Testing and staging
- Learning and tutorials

**Security Considerations**:

- TLS is recommended (but not enforced) for production
- Introspection requires authentication to prevent schema disclosure
- Token expiry of 60 minutes is suitable for internal APIs
- Basic audit logging for compliance verification

### REGULATED Profile

**Configuration**:

```python
from FraiseQL.security.profiles.definitions import get_profile

profile = get_profile("regulated")
config = FraiseQLConfig(
    security_profile=profile,
    database_url="postgresql://localhost/fraiseql_db",
)
```text

**Settings**:

- TLS: **Required** (enforced at middleware layer)
- mTLS: Not required
- Authentication: Required for all operations
- Token Expiry: **15 minutes** (stricter than STANDARD)
- Introspection Policy: **DISABLED** (no schema exposure)
- Rate Limiting: **50 requests/minute** per user (more restrictive)
- Audit Level: **ENHANCED** (detailed event tracking)
- Field-Level Audit: **Enabled** (track field access)
- Query Depth Limit: **10 levels** (reduced from STANDARD)
- Query Complexity: Limited to 5000 estimated units
- Request Body Size: 1 MB limit

**Use Cases**:

- Financial services (banking, fintech, payment processing)
- Healthcare providers (HIPAA compliance)
- PCI-DSS compliance (requirement 6.3.2)
- EU GDPR-regulated systems
- SaaS platforms with regulated customers

**Additional Requirements**:

- Request validation: TLS 1.2 minimum
- Token validation: Check `exp` claim strictly
- CSRF protection: Enforce on all mutations
- Security headers: Strict CSP with limited directives
- Audit logging: Field access tracking for sensitive data

**Production Deployment**:

```bash
# Require HTTPS only
export FRAISEQL_SECURITY_PROFILE=regulated
export FRAISEQL_TLS_ENFORCE=true
export FRAISEQL_TLS_MIN_VERSION=1.2
export FRAISEQL_INTROSPECTION_POLICY=disabled
export FRAISEQL_AUDIT_LEVEL=enhanced
export FRAISEQL_AUDIT_FIELD_ACCESS=true
```text

### RESTRICTED Profile

**Configuration**:

```python
from FraiseQL.security.profiles.definitions import get_profile

profile = get_profile("restricted")
config = FraiseQLConfig(
    security_profile=profile,
    database_url="postgresql://localhost/fraiseql_db",
)
```text

**Settings**:

- TLS: **Required TLS 1.3 minimum** (cutting-edge security)
- mTLS: **Required** (mutual TLS - certificate-based client auth)
- Authentication: Required for all operations
- Token Expiry: **5 minutes** (very short-lived tokens)
- Introspection Policy: **DISABLED** (complete schema protection)
- Rate Limiting: **10 requests/minute** (highly restrictive)
- Audit Level: **VERBOSE** (comprehensive logging)
- Field-Level Audit: **Enabled** (all field access tracked)
- Query Depth Limit: **10 levels** (same as REGULATED)
- Query Complexity: **500 max** (very low limit)
- Request Body Size: **512 KB limit** (protective)
- CSRF: Strict validation with referrer checking

**Use Cases**:

- Government agencies (NSA/CISA standards)
- Military systems (DoD IL4+)
- Critical infrastructure (energy grid, water systems)
- Financial infrastructure (central banks)
- Classified/secret information systems

**Certificate Requirements**:

For mTLS, clients must provide valid X.509 certificates:

```python
# Server-side configuration
mTLS_config = {
    "ca_cert_path": "/etc/tls/ca.crt",           # CA certificate for client verification
    "verify_client_cert": True,
    "fail_if_no_peer_cert": True,
    "require_valid_cert": True,
}

# Client-side configuration required
client_config = {
    "cert_path": "/etc/tls/client.crt",          # Client certificate
    "key_path": "/etc/tls/client.key",           # Client private key
    "ca_cert_path": "/etc/tls/ca.crt",           # CA certificate for verification
    "verify_server_cert": True,                   # Always verify server
}
```text

**Production Deployment**:

```bash
# Maximum security configuration
export FRAISEQL_SECURITY_PROFILE=restricted
export FRAISEQL_TLS_ENFORCE=true
export FRAISEQL_TLS_MIN_VERSION=1.3
export FRAISEQL_MTLS_ENABLE=true
export FRAISEQL_MTLS_CA_CERT=/etc/tls/ca.crt
export FRAISEQL_INTROSPECTION_POLICY=disabled
export FRAISEQL_AUDIT_LEVEL=verbose
export FRAISEQL_AUDIT_FIELD_ACCESS=true
export FRAISEQL_QUERY_COMPLEXITY_MAX=500
export FRAISEQL_RATE_LIMIT_PER_MIN=10
```text

### Profile Enforcement

Security profiles are automatically enforced via middleware. Each request is validated against the profile's constraints:

```python
# Middleware enforcement order:
# 1. TLS validation (if profile requires)
# 2. mTLS client certificate validation (if enabled)
# 3. Authentication check (if profile requires)
# 4. Token expiry validation
# 5. Rate limiting check
# 6. Request body size validation
# 7. Query depth validation
# 8. Query complexity estimation
# 9. Introspection policy enforcement
# 10. CSRF token validation
# 11. Security header injection
# 12. Audit event logging
```text

**Error Handling**:

Requests that fail profile validation receive appropriate HTTP error responses:

```json
HTTP/1.1 403 Forbidden

{
  "errors": [{
    "message": "TLS required by security profile",
    "extensions": {
      "code": "SECURITY_PROFILE_VIOLATION",
      "policy": "restricted",
      "requirement": "tls_v1_3"
    }
  }]
}
```text

---

## Software Bill of Materials (SBOM) Generation

FraiseQL includes automated Software Bill of Materials (SBOM) generation for supply chain security compliance. The system generates CycloneDX 1.5-formatted SBOMs with cryptographic signatures, enabling compliance with NIS2, PCI-DSS, and other regulatory standards.

### SBOM Overview

**What is an SBOM?**

An SBOM is a structured list of all software components, libraries, and dependencies in an application. It includes:

- Component identifiers (name, version, package URL)
- License information (SPDX identifiers)
- Cryptographic hashes (SHA-256 for integrity)
- Vulnerability tracking data
- Supplier and author information

**Why SBOM?**

Modern software supply chain attacks target dependencies. An SBOM enables:

- ✅ **Vulnerability Tracking**: Monitor known CVEs in your dependencies
- ✅ **License Compliance**: Identify permissive vs. copyleft licenses
- ✅ **Supply Chain Security**: Prove integrity via cryptographic signatures
- ✅ **Regulatory Compliance**: Meet NIS2, PCI-DSS, NIST requirements
- ✅ **Incident Response**: Rapidly identify affected components after CVE disclosure

### SBOM Format: CycloneDX 1.5

FraiseQL generates SBOMs in **CycloneDX 1.5** format (OWASP standard):

```json
{
  "bomFormat": "CycloneDX",
  "specVersion": "1.5",
  "serialNumber": "urn:uuid:3e671687-395b-41f5-a30f-a58921a69b79",
  "version": 1,
  "metadata": {
    "timestamp": "2025-01-11T10:30:45+00:00",
    "tools": [{
      "vendor": "FraiseQL",
      "name": "SBOM Generator",
      "version": "1.8.3"
    }],
    "component": {
      "bom-ref": "pkg:python/FraiseQL@1.8.3",
      "type": "application",
      "name": "FraiseQL",
      "version": "1.8.3"
    }
  },
  "components": [
    {
      "bom-ref": "pkg:python/psycopg@3.1.18",
      "type": "library",
      "name": "psycopg",
      "version": "3.1.18",
      "purl": "pkg:python/psycopg@3.1.18",
      "licenses": [{
        "license": {
          "id": "BSD-3-Clause"
        }
      }],
      "hashes": [{
        "alg": "SHA-256",
        "content": "e4c7e8f5a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7"
      }],
      "supplier": {
        "name": "PostgreSQL Development Group"
      }
    }
    // ... more components ...
  ]
}
```text

### SBOM Generation

**Automatic Generation at Release**:

FraiseQL automatically generates and cryptographically signs SBOMs with each release:

```bash
# GitHub Actions workflow: .github/workflows/sbom-generation.yml
# Runs on every release
# Outputs:
# - sbom.json (CycloneDX JSON)
# - sbom.xml (CycloneDX XML)
# - sbom.json.sig (Cosign signature)
# - sbom.json.sha256 (SHA-256 checksum)
```text

**Manual Generation**:

```bash
# Generate SBOM for current environment
FraiseQL sbom generate \
  --format cyclonedx \      # "cyclonedx" or "spdx"
  --output sbom.json        # Output file
  --include-dev             # Include dev dependencies
  --copyleft-check          # Warn about GPL/AGPL licenses

# Verify SBOM signature (using Cosign)
cosign verify-blob \
  --signature sbom.json.sig \
  --certificate sbom.json.cert \
  sbom.json
```text

**Programmatic Generation**:

```python
from FraiseQL.sbom.sbom_generator import SBOMGenerator

generator = SBOMGenerator()

# Generate SBOM
sbom_dict = generator.generate_sbom(
    include_dev_dependencies=True,
    detect_copyleft_licenses=True,
)

# Export as JSON
import json
with open("sbom.json", "w") as f:
    json.dump(sbom_dict, f, indent=2)

# Verify completeness
validation = generator.validate_sbom_completeness(sbom_dict)
if not validation.is_complete:
    print(f"Missing components: {validation.missing_components}")
```text

### SBOM Security: Cryptographic Signing

All published SBOMs are cryptographically signed using **Cosign keyless signing** (Sigstore):

```bash
# Generated artifacts:
# - sbom.json (SBOM content)
# - sbom.json.sig (Detached signature)
# - sbom.json.sha256 (SHA-256 hash)

# Verify SBOM authenticity:
cosign verify-blob \
  --signature sbom.json.sig \
  --certificate sbom.json.cert \
  sbom.json

# Verify SHA-256 integrity:
sha256sum -c sbom.json.sha256
```text

**Security Properties**:

- ✅ **Keyless Identity**: Signature tied to OIDC identity (GitHub Actions)
- ✅ **Tamper Detection**: Any modification breaks signature
- ✅ **Transparency Log**: Signature logged in Rekor transparency log
- ✅ **No Key Management**: No private keys to secure
- ✅ **Auditability**: All signatures publicly verifiable

### SBOM Components

**Dependency Discovery**:

SBOMs include all components in dependency tree:

```python
# Direct dependencies (in pyproject.toml or requirements.txt)

- psycopg 3.1.18
- graphql-core 3.2.0
- pydantic 2.5.0
- starlette 0.36.0

# Transitive dependencies (dependencies of dependencies)

- typing-extensions 4.9.0
- pydantic-core 2.14.2
- anyio 4.1.0
- sniffio 1.3.0

# Total: 150+ dependencies in typical deployment
```text

**Component Fields**:

Each component in SBOM includes:

```json
{
  "bom-ref": "pkg:python/psycopg@3.1.18",      // Unique identifier
  "type": "library",                            // library, application, framework, etc.
  "name": "psycopg",                            // Component name
  "version": "3.1.18",                          // Version
  "purl": "pkg:python/psycopg@3.1.18",          // Package URL (standard format)
  "licenses": [{
    "license": { "id": "BSD-3-Clause" }         // SPDX identifier
  }],
  "hashes": [{
    "alg": "SHA-256",                           // Algorithm
    "content": "e4c7e8f5..."                    // Hex-encoded hash
  }],
  "supplier": {                                 // Supplier information
    "name": "PostgreSQL Development Group"
  },
  "author": {                                   // Author if different from supplier
    "name": "Individual Developer"
  },
  "external-references": [{                     // External links
    "type": "website",
    "url": "https://www.psycopg.org/"
  }]
}
```text

### License Compliance

**SPDX License Identifiers**:

Each component includes SPDX license identifier(s):

```python
# Permissive licenses (generally safe)

- MIT
- Apache-2.0
- BSD-3-Clause
- BSD-2-Clause
- ISC

# Copyleft licenses (may have restrictions)

- GPL-2.0-only
- GPL-3.0-only
- AGPL-3.0-only (network copyleft)
- LGPL-2.1-or-later
```text

**Copyleft Detection**:

FraiseQL automatically warns about copyleft licenses:

```bash
FraiseQL sbom generate --copyleft-check

# Output:
# ⚠️  WARNING: Copyleft licenses detected:
#   - package-name (v1.2.3): GPL-3.0-only
#     Note: GPL-3.0 requires source code distribution of modifications
#     Action: Review license compatibility with your project's license
```text

**FraiseQL Core License**:

- **License**: MIT (permissive, enterprise-friendly)
- **Implication**: FraiseQL itself has no license restrictions
- **Your SBOM**: Will inherit MIT for FraiseQL component
- **Your Dependencies**: Check for incompatible copyleft licenses

### Compliance Standards Supported

**Regulatory Support**:

FraiseQL's SBOM generation supports compliance with:

1. **US Executive Order 14028** (Software Supply Chain Security)
   - Requirement: Provide SBOM to government customers
   - Format: CycloneDX 1.5 ✅
   - Signing: Cryptographic verification ✅
   - Update: Every release ✅

2. **EU NIS2 Directive** (Directive 2022/2555)
   - Effective: October 2024
   - Requirement: Supply chain transparency
   - FraiseQL Compliance:
     - ✅ SBOM generation (CycloneDX 1.5)
     - ✅ Cryptographic signing (Cosign)
     - ✅ Vulnerability tracking (via PURL)
     - ✅ Audit trail (release notes + SBOM history)

3. **EU Cyber Resilience Act** (CRA)
   - Phased Implementation: 2025-2027
   - Requirement: Explicit SBOM requirement
   - FraiseQL Status: ✅ Fully compliant

4. **PCI-DSS 4.0** (Payment Card Industry)
   - Requirement 6.3.2: Security testing of open-source software
   - Effective: March 31, 2025
   - FraiseQL Compliance:
     - ✅ SBOM provides component list
     - ✅ Vulnerability tracking integration
     - ✅ License monitoring

5. **ISO 27001:2022** (Information Security Management)
   - Control 5.21: Supply chain management
   - FraiseQL Compliance:
     - ✅ SBOM as supply chain documentation
     - ✅ Cryptographic integrity verification
     - ✅ Change tracking (version history)

6. **NIST SP 800-161** (Supply Chain Risk Management)
   - Practice: Software Supply Chain Risk Management
   - FraiseQL Compliance:
     - ✅ Component inventory (SBOM)
     - ✅ Vulnerability monitoring
     - ✅ Supplier transparency

---

## Security Headers

FraiseQL automatically injects security headers into all GraphQL responses. These headers protect against common web attacks and inform browsers about security policies.

### Security Headers Reference

#### Content-Security-Policy (CSP)

Restricts what content the browser can load from external sources.

**Strict Configuration** (production):

```text
Content-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'
```text

**Key Directives**:

- `default-src 'none'`: Block all external content by default
- `script-src 'self'`: Only allow inline scripts from same origin
- `style-src 'self'`: Only allow stylesheets from same origin
- `img-src 'self' data:`: Allow images from same origin or data URIs
- `connect-src 'self'`: Only allow connections to same origin
- `frame-ancestors 'none'`: Prevent embedding in frames (API security)

**Development Configuration** (permissive):

```text
Content-Security-Policy: default-src *; script-src * 'unsafe-inline' 'unsafe-eval'; style-src * 'unsafe-inline'
```text

**CSP Violation Reporting**:

```python
# Configure webhook to receive CSP violations
config = FraiseQLConfig(
    csp_report_uri="https://security.example.com/csp-violations",
    csp_report_only=False,  # True = report only, False = enforce
)
```text

#### HTTP Strict-Transport-Security (HSTS)

Forces browser to use HTTPS for all future connections.

**Header**:

```text
Strict-Transport-Security: max-age=31536000; includeSubDomains; preload
```text

**Directives**:

- `max-age=31536000`: Cache policy for 1 year (31,536,000 seconds)
- `includeSubDomains`: Apply to all subdomains
- `preload`: Allow browser to preload domain in hardcoded list

#### X-Content-Type-Options

Prevents MIME type sniffing (browser guessing content type).

```text
X-Content-Type-Options: nosniff
```text

Ensures `Content-Type: application/json` is respected and not misinterpreted.

#### X-Frame-Options

Prevents clickjacking attacks by controlling iframe embedding.

```text
X-Frame-Options: DENY
```text

**Options**:

- `DENY`: Never allow embedding in frames
- `SAMEORIGIN`: Allow embedding only from same origin
- `ALLOW-FROM uri`: Allow embedding from specific URI (deprecated, use CSP instead)

#### X-XSS-Protection

Legacy header for older browser XSS protection (modern browsers use CSP).

```text
X-XSS-Protection: 1; mode=block
```text

#### Referrer-Policy

Controls how much referrer information to send to external sites.

```text
Referrer-Policy: strict-origin-when-cross-origin
```text

**Policies**:

- `strict-origin-when-cross-origin`: Send origin only for cross-origin requests
- `no-referrer`: Never send referrer
- `same-origin`: Send full referrer only for same-origin

#### Permissions-Policy (Feature-Policy)

Restricts browser features and APIs.

```text
Permissions-Policy:
  geolocation=(),
  microphone=(),
  camera=(),
  payment=(),
  usb=(),
  magnetometer=(),
  gyroscope=(),
  accelerometer=()
```text

#### Cross-Origin Policies

Control cross-origin requests and embedding.

```text
Cross-Origin-Embedder-Policy: require-corp
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Resource-Policy: same-site
```text

### Security Header Configuration

**Automatic Injection**:

FraiseQL automatically injects security headers based on security profile:

```python
from FraiseQL.security.profiles.definitions import get_profile

# STANDARD profile
config = FraiseQLConfig(security_profile=get_profile("standard"))
# Headers:
# - CSP (permissive)
# - HSTS (recommended)
# - X-Content-Type-Options: nosniff
# - X-Frame-Options: SAMEORIGIN

# REGULATED profile
config = FraiseQLConfig(security_profile=get_profile("regulated"))
# Headers (stricter):
# - CSP (strict, only 'self' allowed)
# - HSTS (enforced, max-age=31536000)
# - X-Frame-Options: DENY
# - X-XSS-Protection: 1; mode=block

# RESTRICTED profile (maximum security)
config = FraiseQLConfig(security_profile=get_profile("restricted"))
# Headers (maximum):
# - CSP (very strict, no inline scripts)
# - HSTS (enforced + preload)
# - X-Frame-Options: DENY
# - All feature-policy restrictions enabled
```text

**Custom Configuration**:

```python
from FraiseQL.security.security_headers import SecurityHeadersConfig

headers_config = SecurityHeadersConfig(
    csp_directives={
        'default-src': ["'none'"],
        'script-src': ["'self'"],
        'connect-src': ["'self'", "https://api.example.com"],
    },
    hsts_max_age=31536000,
    hsts_include_subdomains=True,
    hsts_preload=True,
    x_frame_options="DENY",
    x_content_type_options="nosniff",
    referrer_policy="strict-origin-when-cross-origin",
    permissions_policy={
        'geolocation': [],
        'camera': [],
        'microphone': [],
    },
)

config = FraiseQLConfig(
    security_headers_config=headers_config,
)
```text

---

## CSRF Protection

FraiseQL provides comprehensive Cross-Site Request Forgery (CSRF) protection with multiple validation strategies.

### CSRF Token Generation

**Token Structure**:

- Cryptographically secure random 32 bytes
- Base64 URL-safe encoding
- HMAC-SHA256 signature with timestamp
- Configurable expiry (default 1 hour)

**Token Example**:

```text
Token: NrVy3e5K_J2x8aB9cDmQpRsT1uVwXyZa
HMAC: e4c7e8f5a1b2c3d4e5f6a7b8c9d0e1f2
```text

### Storage Methods

**Cookie-Based** (recommended for SPAs):

```python
config = FraiseQLConfig(
    csrf_token_storage="cookie",
    csrf_cookie_secure=True,          # HTTPS only
    csrf_cookie_httponly=True,        # Not accessible to JavaScript
    csrf_cookie_samesite="strict",    # Prevent cross-site cookie sends
    csrf_cookie_domain=".example.com",
    csrf_cookie_path="/graphql",
)
```text

**Session-Based** (server-side storage):

```python
config = FraiseQLConfig(
    csrf_token_storage="session",
    # Requires session middleware
)
```text

**Header-Based** (custom header):

```python
config = FraiseQLConfig(
    csrf_token_storage="header",
    csrf_header_name="X-CSRF-Token",
)
```text

### CSRF Validation

**Automatic Validation**:

FraiseQL automatically validates CSRF tokens on all mutations:

```python
# Request with CSRF token
POST /graphql HTTP/1.1
Content-Type: application/json
X-CSRF-Token: NrVy3e5K_J2x8aB9cDmQpRsT1uVwXyZa

{
  "query": "mutation { createUser(name: \"Alice\") { id } }"
}
```text

**Validation Checks**:

1. Extract token from cookie/header/variable
2. Validate HMAC signature
3. Check token expiry
4. Verify session binding (if session-based)
5. Validate referrer header (optional)
6. Check trusted origins whitelist

**Error Response** (invalid token):

```json
HTTP/1.1 403 Forbidden

{
  "errors": [{
    "message": "CSRF token validation failed",
    "extensions": {
      "code": "CSRF_TOKEN_INVALID",
      "reason": "Token expired"
    }
  }]
}
```text

### Exemptions

**Query Exemption** (no CSRF for queries):

```python
# Queries don't require CSRF (read-only, safe)
POST /graphql HTTP/1.1

{
  "query": "query { users { id name } }"
}
// No X-CSRF-Token header needed
```text

**Path Exemptions**:

```python
csrf_config = CSRFConfig(
    exempt_paths=["/health", "/metrics", "/docs"],
    exempt_methods=["GET", "HEAD", "OPTIONS"],  # Default
)
```text

**Mutation Exemption** (advanced):

```python
# Can disable CSRF for specific operations if needed
csrf_config = CSRFConfig(
    exempt_operations=["IntrospectionQuery"],  # If using persisted queries
)
```text

### CSRF in SPAs

**Single-Page Application Flow**:

```javascript
// 1. Get initial page (token in cookie)
fetch('/').then(response => {
  // Token automatically set in secure, httponly cookie
})

// 2. Make GraphQL mutation
const mutation = `mutation { createUser(name: "Alice") { id } }`;
fetch('/graphql', {
  method: 'POST',
  headers: {
    'Content-Type': 'application/json',
    // Token from cookie automatically sent (SameSite=Strict)
  },
  body: JSON.stringify({ query: mutation }),
})
```text

**Production Configuration**:

```python
csrf_config = CSRFConfig(
    enabled=True,
    cookie_secure=True,           # HTTPS only
    cookie_httponly=True,         # JS cannot access
    cookie_samesite="strict",     # Most restrictive
    check_referrer=True,          # Additional check
    check_origin=True,            # Validate Origin header
    trusted_origins=[
        "https://app.example.com",
        "https://www.example.com",
    ],
    token_expiry=3600,            # 1 hour
)
```text

---

## Token Revocation

FraiseQL provides immediate token revocation capabilities for logout, session management, and security incident response.

### Revocation Methods

**Single Token Revocation** (logout):

```python
from FraiseQL.auth.token_revocation import TokenRevocationService

service = TokenRevocationService(
    store="postgresql",  # or "memory" for development
    database_url="postgresql://localhost/fraiseql_db"
)

# Revoke token by JTI (JWT ID)
await service.revoke_token(
    jti="token-id-from-claims",
    reason="User logout"
)
```text

**All User Tokens** (force logout all sessions):

```python
# Revoke all tokens for a user
await service.revoke_all_user_tokens(
    user_id="user-123",
    reason="Security incident detected"
)
```text

**Revocation Store Implementations**:

1. **InMemoryRevocationStore** (development):
   - Fast but non-persistent
   - Lost on application restart
   - Single process only

2. **PostgreSQLRevocationStore** (production):
   - Persistent across restarts
   - Shared across multiple instances
   - Automatic cleanup of expired entries

**Database Schema**:

```sql
CREATE TABLE token_revocation (
    jti VARCHAR(255) PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    revoked_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    expires_at TIMESTAMP WITH TIME ZONE NOT NULL,
    reason VARCHAR(255),

    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX idx_token_revocation_user ON token_revocation(user_id);
CREATE INDEX idx_token_revocation_expires ON token_revocation(expires_at);
```text

### Logout Flow

**Request**:

```bash
POST /auth/logout HTTP/1.1
Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...

{
  "token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."
}
```text

**Processing**:

1. Extract JWT from Authorization header
2. Decode JWT (without verifying signature, for jti claim)
3. Extract `jti` claim
4. Store revocation record with expiry = original token's `exp` claim
5. Return success response

**Response**:

```json
HTTP/1.1 200 OK

{
  "message": "Logged out successfully",
  "revoked_at": "2025-01-11T10:30:45Z"
}
```text

### Token Validation on Requests

**Authentication Middleware**:

```python
# On every authenticated request:
# 1. Extract JWT from Authorization header
# 2. Verify signature using public key
# 3. Extract jti claim
# 4. Check revocation store: is jti revoked?
# 5. If revoked: reject request (401 Unauthorized)
# 6. If not revoked: proceed with request
```text

**Performance**:

- In-memory store: < 1µs (sub-microsecond)
- PostgreSQL store: 5-15ms (one database query)
- Caching recommended for high-throughput APIs

---

## Rate Limiting

FraiseQL provides sophisticated rate limiting based on GraphQL operations and query complexity.

### Rate Limiting Strategies

**Fixed Window** (traditional):

- Time divided into fixed buckets (minute, hour, day)
- Count requests in current window
- Simple but susceptible to burst at window boundaries

**Sliding Window** (recommended):

- Requests in past N seconds
- More accurate burst detection
- Slightly more overhead

**Token Bucket** (smoothing):

- Tokens generated at constant rate
- Each request consumes tokens
- Burst capacity configurable
- Smooths traffic patterns

**FraiseQL Default**: Sliding window with 1-minute window

### Configuration by Operation Type

```python
rate_limit_config = RateLimitConfig(
    strategies={
        "query": {
            "limit": 100,           # 100 queries/minute
            "window": 60,           # 1 minute
            "strategy": "sliding_window",
        },
        "mutation": {
            "limit": 20,            # 20 mutations/minute (stricter)
            "window": 60,
            "strategy": "sliding_window",
        },
        "subscription": {
            "limit": 10,            # 10 subscriptions
            "window": 60,
            "strategy": "token_bucket",  # Smooth out subscriptions
        },
    }
)
```text

### Complexity-Based Rate Limiting

Automatically estimates query complexity and applies stricter limits to expensive queries:

```python
# Query complexity auto-estimation
# Simple query (5 fields): ~10 units
# Moderate query (20 fields, 1-2 levels): ~50 units
# Complex query (100+ fields, nested): ~500+ units

rate_limit_config = RateLimitConfig(
    complexity_limits={
        "low": {      # 0-50 complexity units
            "limit": 200,
            "window": 60,
        },
        "medium": {   # 50-200 complexity units
            "limit": 100,
            "window": 60,
        },
        "high": {     # 200-500 complexity units
            "limit": 20,
            "window": 60,
        },
        "critical": { # 500+ complexity units
            "limit": 5,
            "window": 60,
        },
    }
)
```text

### Response Headers

FraiseQL includes rate limit information in response headers:

```text
HTTP/1.1 200 OK
X-RateLimit-Limit: 100
X-RateLimit-Remaining: 87
X-RateLimit-Window: 60
X-RateLimit-Reset: 1631384400
```text

**Rate Limit Exceeded** (HTTP 429):

```json
HTTP/1.1 429 Too Many Requests
Retry-After: 45

{
  "errors": [{
    "message": "Rate limit exceeded",
    "extensions": {
      "code": "RATE_LIMIT_EXCEEDED",
      "limit": 100,
      "reset_in": 45
    }
  }]
}
```text

### Store Implementations

**In-Memory Store** (development):

```python
RateLimitConfig(
    store="memory",  # No external dependencies
    cleanup_interval=300,  # Cleanup every 5 minutes
)
```text

**Redis Store** (production):

```python
RateLimitConfig(
    store="redis",
    redis_url="redis://localhost:6379/0",
    key_prefix="rate_limit:",
    ttl=3600,  # Automatic cleanup after 1 hour
)
```text

---

## Field-Level Authorization

FraiseQL supports fine-grained per-field authorization using decorators.

### Field Authorization Decorator

```python
from FraiseQL import FraiseQL, field_type
from FraiseQL.security.field_auth import authorize_field

@FraiseQL.type
class User:
    id: ID
    name: str

    @authorize_field(
        lambda info: info.context.get("user_id") == self.id,
        error_message="You can only view your own email"
    )
    email: str

    @authorize_field(
        lambda info: "admin" in info.context.get("roles", []),
        error_message="Only admins can view salary information"
    )
    salary: float | None = None
```text

### Permission Check Patterns

**Current User Check**:

```python
@authorize_field(
    lambda info: info.context.get("user_id") == self.id
)
def email(self) -> str:
    return self._email
```text

**Role Check**:

```python
@authorize_field(
    lambda info: "manager" in info.context.get("roles", [])
)
def sensitive_data(self) -> str:
    return self._sensitive_data
```text

**Combined Permissions**:

```python
from FraiseQL.security.field_auth import combine_permissions, any_permission

def is_owner(info) -> bool:
    return info.context.get("user_id") == self.id

def is_admin(info) -> bool:
    return "admin" in info.context.get("roles", [])

# All must pass (AND)
@authorize_field(combine_permissions(is_owner, is_admin))
def audit_log(self) -> str: ...

# Any can pass (OR)
@authorize_field(any_permission(is_owner, is_admin))
def admin_notes(self) -> str: ...
```text

### Async Permission Checks

```python
async def check_user_permission(info) -> bool:
    # Can make async calls (database queries, API calls)
    user_id = info.context.get("user_id")
    has_permission = await check_permission_in_db(user_id, "view_email")
    return has_permission

@authorize_field(check_user_permission)
def email(self) -> str:
    return self._email
```text

### Error Handling

When field authorization fails:

```json
{
  "data": {
    "user": {
      "id": "user-123",
      "name": "Alice",
      "email": null  // Removed due to authorization failure
    }
  },
  "errors": [{
    "message": "You can only view your own email",
    "path": ["user", "email"],
    "extensions": {
      "code": "FIELD_AUTHORIZATION_ERROR"
    }
  }]
}
```text

---

## Introspection Control

FraiseQL provides three introspection policies to balance schema transparency with security.

### Introspection Policies

**ENABLED** (development):

```python
FraiseQLConfig(
    introspection_policy="enabled"
)
```text

- Anyone can introspect the schema
- ✅ Developer-friendly
- ❌ Schema fully exposed to attackers

**AUTHENTICATED** (default for STANDARD):

```python
FraiseQLConfig(
    introspection_policy="authenticated"
)
```text

- Only authenticated users can introspect
- ✅ Balances usability and security
- ✅ Allows internal tool development
- ✅ Prevents external reconnaissance

**DISABLED** (production):

```python
FraiseQLConfig(
    introspection_policy="disabled"
)
```text

- Introspection queries completely blocked
- ✅ Maximum security
- ✅ Prevents schema disclosure
- ❌ Requires external schema documentation

### Enforcement

Introspection queries like `{ __schema { ... } }` are blocked according to policy:

**DISABLED Policy Response**:

```json
{
  "errors": [{
    "message": "Introspection is disabled",
    "extensions": {
      "code": "INTROSPECTION_DISABLED"
    }
  }]
}
```text

### Introspection Query Examples

**Full Schema Introspection**:

```graphql
query {
  __schema {
    types {
      name
      fields { name type }
    }
  }
}
```text

**Type Information**:

```graphql
query {
  __type(name: "User") {
    name
    fields { name type }
    interfaces { name }
  }
}
```text

---

## Key Management Service (KMS)

FraiseQL supports multiple Key Management Service (KMS) providers for encryption key management and data protection.

### Supported Providers

**AWS KMS**:

```python
config = FraiseQLConfig(
    kms_provider="aws",
    kms_config={
        "region": "us-east-1",
        "key_id": "arn:aws:kms:us-east-1:123456789:key/uuid",
        "profile": "default",  # Optional AWS profile
    }
)
```text

**HashiCorp Vault**:

```python
config = FraiseQLConfig(
    kms_provider="vault",
    kms_config={
        "url": "https://vault.example.com:8200",
        "token": os.getenv("VAULT_TOKEN"),
        "mount_path": "secret",
    }
)
```text

**Google Cloud KMS**:

```python
config = FraiseQLConfig(
    kms_provider="gcp",
    kms_config={
        "project_id": "my-project",
        "location": "global",
        "key_ring": "my-keyring",
        "key": "my-key",
    }
)
```text

**Local KMS** (development):

```python
config = FraiseQLConfig(
    kms_provider="local",
    kms_config={
        "key_path": "./local_key.bin"  # Optional
    }
)
```text

### Encryption/Decryption

**Encrypting Sensitive Data**:

```python
from FraiseQL.security.kms import KeyManager

key_manager = KeyManager(config=config)

# Encrypt data
plaintext = "sensitive_user_data"
encrypted = await key_manager.encrypt(
    plaintext.encode(),
    context={"user_id": "user-123"}  # Additional Authenticated Data
)

# Store encrypted_data.ciphertext in database
# Discard plaintext
```text

**Decrypting Data**:

```python
# Retrieve from database
encrypted_data = db.fetch_encrypted_field(user_id)

# Decrypt
plaintext = await key_manager.decrypt(
    encrypted_data.ciphertext,
    context={"user_id": "user-123"}
)

# Use plaintext
```text

### Key Rotation

**Manual Rotation**:

```python
await key_manager.rotate_key(
    key_reference="my-key",
    policy={
        "rotation_enabled": True,
        "rotation_period_days": 90,
    }
)
```text

**Envelope Encryption** (recommended):

```python
# Generate data key at startup
data_key = await key_manager.generate_data_key()

# Use for all encryption during application lifetime
plaintext = b"sensitive data"
ciphertext = await key_manager.encrypt_with_key(plaintext, data_key)

# On application shutdown or rotation:
# 1. Generate new data key
# 2. Re-encrypt all data with new key
# 3. Destroy old key
```text

---

## Security Event Logging

FraiseQL provides comprehensive security event logging for audit trails, compliance, and incident response.

### Security Event Types

**Authentication Events**:

- `AUTH_SUCCESS`: Successful login
- `AUTH_FAILURE`: Failed login attempt
- `AUTH_TOKEN_EXPIRED`: Token expiry
- `AUTH_TOKEN_INVALID`: Invalid token
- `AUTH_LOGOUT`: User logout

**Authorization Events**:

- `AUTHZ_DENIED`: Operation authorization failed
- `AUTHZ_FIELD_DENIED`: Field access denied
- `AUTHZ_PERMISSION_DENIED`: Permission check failed
- `AUTHZ_ROLE_DENIED`: Role requirement not met

**Rate Limiting Events**:

- `RATE_LIMIT_EXCEEDED`: Request exceeds rate limit
- `RATE_LIMIT_WARNING`: Approaching rate limit

**CSRF Events**:

- `CSRF_TOKEN_INVALID`: CSRF token validation failed
- `CSRF_TOKEN_MISSING`: CSRF token not provided

**Query Security Events**:

- `QUERY_COMPLEXITY_EXCEEDED`: Query too complex
- `QUERY_DEPTH_EXCEEDED`: Query too deep
- `QUERY_TIMEOUT`: Query execution timeout
- `QUERY_MALICIOUS_PATTERN`: Suspected attack pattern

### Security Event Structure

```python
from FraiseQL.audit.security_logger import SecurityEvent

event = SecurityEvent(
    event_type="AUTH_FAILURE",
    severity="WARNING",
    timestamp="2025-01-11T10:30:45Z",
    user_id="user-123",
    user_email="alice@example.com",
    ip_address="192.0.2.1",
    user_agent="Mozilla/5.0...",
    request_id="req-abc123def456",
    resource="User",
    action="authenticate",
    result="denied",
    reason="Invalid credentials",
    metadata={
        "attempted_username": "alice@example.com",
        "auth_method": "password",
        "failure_count": 3,
    }
)
```text

### Logging Configuration

```python
from FraiseQL.audit.security_logger import SecurityLogger

logger = SecurityLogger(
    log_file="/var/log/FraiseQL-security.log",
    log_stdout=True,
    severity_threshold="WARNING",  # Only log WARNING and above
)

# Enable global security logging
import FraiseQL.audit.security_logger
FraiseQL.audit.security_logger.set_global_logger(logger)
```text

### Log Output

**File Format**:

```json
{
  "timestamp": "2025-01-11T10:30:45.123456Z",
  "event_type": "AUTH_FAILURE",
  "severity": "WARNING",
  "user_id": "user-123",
  "user_email": "alice@example.com",
  "ip_address": "192.0.2.1",
  "request_id": "req-abc123def456",
  "resource": "User",
  "action": "authenticate",
  "result": "denied",
  "reason": "Invalid credentials",
  "metadata": {
    "attempted_username": "alice@example.com",
    "auth_method": "password",
    "failure_count": 3
  }
}
```text

### Compliance Integration

Security event logs support compliance requirements:

- **NIS2**: Event logging for security incidents
- **GDPR**: User activity tracking (anonymization supported)
- **PCI-DSS**: Authentication and authorization logging
- **HIPAA**: Access logging for protected health information
- **SOC 2**: Security event audit trail

---

## Regulatory Compliance Summary

FraiseQL's security features provide comprehensive support for major regulatory frameworks:

### Certification Status

| Regulation | Requirement | FraiseQL Support | Status |
|-----------|-------------|-----------------|--------|
| **NIS2** | SBOM provision | CycloneDX 1.5 signed SBOMs | ✅ Compliant |
| **NIS2** | Vulnerability tracking | Integrated with package URLs | ✅ Compliant |
| **NIS2** | Supply chain transparency | Automated SBOM generation | ✅ Compliant |
| **CRA** | SBOM requirement | CycloneDX 1.5 format | ✅ Compliant |
| **PCI-DSS 4.0** | Open-source testing | SBOM for dependency tracking | ✅ Compliant (6.3.2) |
| **ISO 27001** | Supply chain management | SBOM as supply chain documentation | ✅ Compliant (5.21) |
| **NIST 800-161** | Supply chain risk | Component inventory + vulnerability tracking | ✅ Compliant |
| **NIST 800-218** | Secure development | Security profiles + audit logging | ✅ Compliant |
| **GDPR** | Data protection | Field-level auth + encryption | ✅ Compliant |
| **HIPAA** | Access logging | Security event logging | ✅ Compliant |
| **SOC 2** | Security audit trail | Comprehensive event logging | ✅ Compliant |

---

## Deployment Checklist

### Development Environment

- [ ] Security Profile: `standard`
- [ ] TLS: Optional
- [ ] Introspection: AUTHENTICATED
- [ ] APQ Mode: optional
- [ ] Rate Limiting: 100 req/min
- [ ] CSRF: Enabled but relaxed
- [ ] Field Audit: Disabled

### Staging Environment

- [ ] Security Profile: `regulated`
- [ ] TLS: Required (1.2+)
- [ ] Introspection: DISABLED
- [ ] APQ Mode: optional
- [ ] Rate Limiting: 50 req/min
- [ ] CSRF: Strict validation
- [ ] Field Audit: Enabled
- [ ] SBOM: Generated
- [ ] Security Headers: Strict CSP

### Production Environment

- [ ] Security Profile: `restricted`
- [ ] TLS: Required (1.3 minimum)
- [ ] mTLS: Enabled
- [ ] Introspection: DISABLED
- [ ] APQ Mode: required
- [ ] Rate Limiting: 10 req/min
- [ ] CSRF: Strict + referrer check
- [ ] Field Audit: Enabled
- [ ] Token Revocation: PostgreSQL backend
- [ ] KMS: Multi-provider (AWS/Vault)
- [ ] SBOM: Generated + signed
- [ ] Security Logging: Enabled
- [ ] Health Checks: Configured
- [ ] Monitoring: Enabled

---

## Conclusion

FraiseQL's comprehensive security and compliance features provide enterprise-grade protection for GraphQL APIs. By leveraging security profiles, SBOM generation, cryptographic controls, and detailed audit logging, you can deploy with confidence to regulated industries and meet the most stringent security standards.

**Key Takeaways**:

- ✅ Use security profiles to bundle related settings by compliance tier
- ✅ Generate and sign SBOMs for supply chain security compliance
- ✅ Implement field-level authorization for data protection
- ✅ Enable security logging for audit trails and compliance reporting
- ✅ Use KMS for encryption of sensitive data
- ✅ Monitor security events and respond to anomalies

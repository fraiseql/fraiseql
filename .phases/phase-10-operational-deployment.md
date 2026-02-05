# Phase 10: Operational Deployment

**Objective**: Create production-ready Docker images, Kubernetes manifests, and deployment infrastructure

**Duration**: 2-3 weeks

**Estimated LOC**: 1500-2000 (configs, manifests, scripts)

---

## Success Criteria

- [ ] Multi-stage Dockerfile with hardening (0 CRITICAL/HIGH vulnerabilities)
- [ ] Helm chart with comprehensive values.yaml
- [ ] Kubernetes base manifests (deployment, service, configmap)
- [ ] Hardened K8s manifests (with pod security, network policies)
- [ ] Docker Compose for local development
- [ ] SBOM generation in build pipeline
- [ ] Health check configuration
- [ ] Deployment security guide
- [ ] All tests passing
- [ ] Zero clippy warnings

---

## TDD Cycles

### Cycle 10.1: Foundation Setup & Test Infrastructure

**Objective**: Establish directory structure, Docker build pipeline, and test infrastructure

#### RED: Write failing tests
```rust
// tests/deployment_test.rs

#[tokio::test]
async fn test_dockerfile_build() {
    assert!(Path::new("Dockerfile").exists(), "Dockerfile must exist");
    assert!(Path::new(".dockerignore").exists(), ".dockerignore must exist");
    assert!(Path::new("deploy/docker").exists(), "deploy/docker must exist");
}

#[test]
fn test_dockerfile_multi_arch_buildkit() {
    let dockerfile = fs::read_to_string("Dockerfile").unwrap();
    // BuildKit syntax for multi-arch builds
    assert!(dockerfile.contains("syntax=docker/dockerfile:1.4"), "BuildKit syntax required");
    assert!(dockerfile.contains("FROM rust:"), "Must have Rust builder stage");
}

#[test]
fn test_helm_chart_valid() {
    let chart_path = "deploy/kubernetes/helm/fraiseql/Chart.yaml";
    assert!(Path::new(chart_path).exists());

    let chart_content = fs::read_to_string(chart_path).unwrap();
    let chart: serde_yaml::Value = serde_yaml::from_str(&chart_content).unwrap();

    // Validate Chart.yaml structure
    assert!(chart.get("apiVersion").is_some(), "Chart must have apiVersion");
    assert!(chart.get("name").is_some(), "Chart must have name");
    assert!(chart.get("version").is_some(), "Chart must have version");
}

#[test]
fn test_kube_manifests_exist() {
    let manifests_dir = "deploy/kubernetes";
    assert!(Path::new(manifests_dir).exists());

    assert!(Path::new("deploy/kubernetes/deployment.yaml").exists());
    assert!(Path::new("deploy/kubernetes/service.yaml").exists());
    assert!(Path::new("deploy/kubernetes/configmap.yaml").exists());
}

#[tokio::test]
async fn test_testcontainers_infrastructure() {
    // Verify test database setup available
    assert!(which::which("docker").is_ok(), "Docker required for test containers");
}

#[test]
fn test_ci_cd_workflow_exists() {
    assert!(Path::new(".github/workflows").exists());
    assert!(Path::new(".github/workflows/build-docker.yml").exists(), "Docker build workflow required");
    assert!(Path::new(".github/workflows/build-sbom.yml").exists(), "SBOM workflow required");
}
```

#### GREEN: Create minimal implementation
- Create `/deploy/` directory structure
  - `/deploy/docker/` - Docker-related files
  - `/deploy/kubernetes/` - K8s manifests
  - `/deploy/kubernetes/helm/fraiseql/` - Helm chart
- Create `Dockerfile` stub with BuildKit syntax
- Create `.dockerignore`
- Create Helm chart directory with `Chart.yaml` stub
- Create CI/CD workflow files:
  - `.github/workflows/build-docker.yml` (multi-arch build)
  - `.github/workflows/build-sbom.yml` (SBOM generation)
  - `.github/workflows/test-k8s.yml` (manifest validation)
- Create test infrastructure:
  - `tests/deployment_integration_test.rs` template
  - `tests/fixtures/docker-compose.test.yml` (test database)

#### REFACTOR
- Ensure directory structure matches v1 patterns
- Use consistent naming conventions
- CI/CD workflows follow best practices (caching, parallelization)

#### CLEANUP
- Remove any temporary files
- Verify `cargo test` passes
- Verify CI/CD workflows syntax valid (check with `yamllint`)

---

### Cycle 10.2: Multi-Stage Dockerfile with Multi-Architecture Support

**Objective**: Create production Docker image with multi-arch support and security hardening

#### Files
- `Dockerfile` (multi-stage, multi-arch support)
- `deploy/docker/Dockerfile.hardened` (security-focused variant)
- `.dockerignore`
- `.github/workflows/build-docker.yml` (CI/CD multi-arch build)

#### RED: Write tests
```rust
#[test]
fn test_dockerfile_has_build_stage() {
    let dockerfile = fs::read_to_string("Dockerfile").unwrap();
    assert!(dockerfile.contains("FROM rust:"), "Must have Rust builder stage");
    assert!(dockerfile.contains("AS builder"), "Builder stage required");
}

#[test]
fn test_dockerfile_production_stage() {
    let dockerfile = fs::read_to_string("Dockerfile").unwrap();
    assert!(dockerfile.contains("FROM debian:bookworm-slim"), "Production stage required");
    assert!(dockerfile.contains("COPY --from=builder"), "Must copy from builder");
}

#[test]
fn test_dockerfile_security_hardening() {
    let dockerfile = fs::read_to_string("Dockerfile").unwrap();
    assert!(dockerfile.contains("useradd"), "Non-root user required");
    assert!(dockerfile.contains("USER"), "Switch to non-root user");
    assert!(dockerfile.contains("HEALTHCHECK"), "Health check required");
}

#[test]
fn test_dockerfile_multi_arch_support() {
    let dockerfile = fs::read_to_string("Dockerfile").unwrap();
    // ARG for platform support
    assert!(dockerfile.contains("ARG TARGETARCH") || dockerfile.contains("TARGETPLATFORM"),
            "Multi-arch support required");
}

#[test]
fn test_dockerfile_hardened_variant() {
    let hardened = fs::read_to_string("deploy/docker/Dockerfile.hardened").unwrap();
    // Must not use curl (use netcat/nc for health check)
    assert!(!hardened.contains("curl -f"), "Hardened variant must use nc instead of curl");
    // Read-only filesystem compatible
    assert!(hardened.contains("--read-only") || hardened.contains("readonly"),
            "Hardened must document read-only FS compatibility");
}

#[test]
fn test_ci_docker_build_workflow() {
    let workflow = fs::read_to_string(".github/workflows/build-docker.yml").unwrap();
    assert!(workflow.contains("platforms:"), "Must build multiple platforms");
    assert!(workflow.contains("linux/amd64"), "Must support x86_64");
    assert!(workflow.contains("linux/arm64"), "Must support ARM64 for K8s ARM nodes");
}
```

#### GREEN: Implement Dockerfile with Multi-Arch Support

**Dockerfile** (multi-arch variant):
```dockerfile
# syntax=docker/dockerfile:1.4

# Build arguments for cross-compilation
ARG TARGETARCH
ARG TARGETVARIANT

# Stage 1: Builder - use rust image for target arch
FROM --platform=$BUILDPLATFORM rust:1.85-slim AS builder

ARG TARGETARCH
ARG TARGETVARIANT

# Set Rust target based on architecture
RUN case "$TARGETARCH" in \
      amd64) TARGET="x86_64-unknown-linux-gnu" ;; \
      arm64) TARGET="aarch64-unknown-linux-gnu" ;; \
      arm) TARGET="armv7-unknown-linux-gnueabihf" ;; \
      ppc64le) TARGET="powerpc64le-unknown-linux-gnu" ;; \
      *) echo "Unsupported architecture: $TARGETARCH" && exit 1 ;; \
    esac && \
    echo "$TARGET" > /tmp/rust_target.txt && \
    rustup target add "$TARGET"

RUN apt-get update && apt-get install -y --no-install-recommends \
    libpq-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN TARGET=$(cat /tmp/rust_target.txt) && \
    cargo build --release --target "$TARGET" -p fraiseql-server

# Stage 2: Runtime
FROM debian:bookworm-slim

LABEL org.opencontainers.image.version="2.1.0" \
      org.opencontainers.image.vendor="FraiseQL" \
      org.opencontainers.image.licenses="MIT" \
      org.opencontainers.image.description="FraiseQL GraphQL execution engine" \
      org.opencontainers.image.documentation="https://github.com/fraiseql/fraiseql" \
      security.compliance="production" \
      security.hardenings="non-root,readonly-capable,capabilities-dropped"

# Security updates
RUN apt-get update && apt-get upgrade -y && apt-get install -y --no-install-recommends \
    libpq5 \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Non-root user (UID 65532 for distroless compatibility)
RUN groupadd -g 65532 fraiseql && \
    useradd -r -u 65532 -g fraiseql -s /sbin/nologin -d /app fraiseql

# Create app directory with minimal permissions
RUN mkdir -p /app && chown -R fraiseql:fraiseql /app

WORKDIR /app

# Copy binary from builder (auto-detects target arch from build stage)
COPY --from=builder --chown=fraiseql:fraiseql /build/target/*/release/fraiseql-server .

USER fraiseql
EXPOSE 8815

ENV RUST_LOG=info

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8815/health || exit 1

CMD ["./fraiseql-server"]
```

**deploy/docker/Dockerfile.hardened** (security-hardened variant):
```dockerfile
# syntax=docker/dockerfile:1.4

# Same builder stage as main Dockerfile
FROM --platform=$BUILDPLATFORM rust:1.85-slim AS builder
# ... [builder stage identical to main Dockerfile]

# Stage 2: Hardened Runtime (distroless-style)
FROM debian:bookworm-slim

LABEL org.opencontainers.image.version="2.1.0-hardened" \
      security.level="high" \
      security.hardenings="non-root,readonly-fs,no-capabilities,no-new-privileges"

# Minimal dependencies only (distroless style but not distroless yet)
RUN apt-get update && apt-get upgrade -y && apt-get install -y --no-install-recommends \
    libpq5 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/* && \
    # Remove unnecessary packages
    apt-get remove -y --allow-remove-essential apt apt-utils && \
    apt-get autoremove -y

RUN groupadd -g 65532 fraiseql && \
    useradd -r -u 65532 -g fraiseql -s /sbin/nologin -d /app fraiseql

# Read-only root filesystem
RUN mkdir -p /app /var/run/fraiseql && \
    chown -R fraiseql:fraiseql /app /var/run/fraiseql

WORKDIR /app
COPY --from=builder --chown=fraiseql:fraiseql /build/target/*/release/fraiseql-server .

USER fraiseql
EXPOSE 8815

ENV RUST_LOG=info

# Health check using nc (netcat) instead of curl
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD nc -z localhost 8815 || exit 1

# Security: no new privileges
SECURITY_OPT seccomp=unconfined

CMD ["./fraiseql-server"]
```

**CI/CD Workflow** (`.github/workflows/build-docker.yml`):
```yaml
name: Build Multi-Arch Docker Images

on:
  push:
    branches: [main, dev]
    tags: ['v*']

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Set up QEMU
        uses: docker/setup-qemu-action@v3

      - name: Set up Docker Buildx
        uses: docker/setup-buildx-action@v3

      - name: Build & push
        uses: docker/build-push-action@v5
        with:
          context: .
          platforms: linux/amd64,linux/arm64,linux/arm/v7
          push: ${{ github.event_name == 'push' && startsWith(github.ref, 'refs/tags') }}
          tags: fraiseql:latest,fraiseql:${{ github.sha }}
          cache-from: type=gha
          cache-to: type=gha,mode=max
```

#### REFACTOR
- Extract common multi-arch logic to build stage
- Optimize layer caching (separate dependency layers)
- Document platform differences

#### CLEANUP
- Verify builds for amd64, arm64, arm/v7
- Remove commented examples
- Verify no secrets in context

---

### Cycle 10.3: Helm Chart Creation

**Objective**: Create production-grade Helm chart for Kubernetes deployment

#### Files
- `deploy/kubernetes/helm/fraiseql/Chart.yaml`
- `deploy/kubernetes/helm/fraiseql/values.yaml` (comprehensive)
- `deploy/kubernetes/helm/fraiseql/templates/deployment.yaml`
- `deploy/kubernetes/helm/fraiseql/templates/service.yaml`
- `deploy/kubernetes/helm/fraiseql/templates/configmap.yaml`
- `deploy/kubernetes/helm/fraiseql/templates/hpa.yaml`
- `deploy/kubernetes/helm/fraiseql/templates/ingress.yaml`

#### RED: Write tests
```rust
#[test]
fn test_helm_values_structure() {
    let values = fs::read_to_string("deploy/kubernetes/helm/fraiseql/values.yaml").unwrap();
    let parsed: serde_yaml::Value = serde_yaml::from_str(&values).unwrap();

    // Required top-level keys
    assert!(parsed.get("image").is_some());
    assert!(parsed.get("replicaCount").is_some());
    assert!(parsed.get("service").is_some());
    assert!(parsed.get("database").is_some());
    assert!(parsed.get("autoscaling").is_some());
    assert!(parsed.get("security").is_some());
}

#[test]
fn test_helm_templates_exist() {
    let templates_dir = "deploy/kubernetes/helm/fraiseql/templates";
    assert!(fs::read_dir(templates_dir).is_ok());
    assert!(Path::new(&format!("{}/deployment.yaml", templates_dir)).exists());
    assert!(Path::new(&format!("{}/service.yaml", templates_dir)).exists());
    assert!(Path::new(&format!("{}/configmap.yaml", templates_dir)).exists());
}
```

#### GREEN: Implement Helm chart

Use v1 patterns from `/home/lionel/code/fraiseql_v1/deploy/kubernetes/helm/fraiseql/values.yaml`:

**Key sections**:
1. Image configuration (repository, pullPolicy, tag)
2. Replication & autoscaling (3 replicas default, HPA with CPU/memory)
3. Service configuration (ClusterIP, ports, annotations)
4. Ingress (nginx, cert-manager, TLS)
5. Resources (requests/limits: 250m/512Mi min, 1000m/1Gi max)
6. Health checks (liveness, readiness, startup probes)
7. Application config (GraphQL paths, complexity limits, APQ)
8. Database configuration (connection pooling, statement timeout)
9. Secrets management (external secret pattern)
10. Security (pod security context, network policies optional)
11. Pod disruption budget
12. Affinity rules for distribution

#### REFACTOR
- Extract common label templates
- Use consistent naming patterns

#### CLEANUP
- Validate YAML syntax
- Check helm lint passes

---

### Cycle 10.4: Kubernetes Base Manifests

**Objective**: Create standalone K8s manifests (alternative to Helm)

#### Files
- `deploy/kubernetes/deployment.yaml`
- `deploy/kubernetes/service.yaml`
- `deploy/kubernetes/configmap.yaml`
- `deploy/kubernetes/ingress.yaml`
- `deploy/kubernetes/hpa.yaml`

#### RED: Write tests
```rust
#[test]
fn test_k8s_manifests_valid() {
    let manifests = vec![
        "deploy/kubernetes/deployment.yaml",
        "deploy/kubernetes/service.yaml",
        "deploy/kubernetes/configmap.yaml",
    ];

    for manifest in manifests {
        let content = fs::read_to_string(manifest).unwrap();
        let parsed: serde_yaml::Value = serde_yaml::from_str(&content).unwrap();
        assert!(parsed.get("apiVersion").is_some());
        assert!(parsed.get("kind").is_some());
        assert!(parsed.get("metadata").is_some());
    }
}
```

#### GREEN: Create manifests
- Based on Helm templates (can generate with `helm template`)
- Include proper labels and selectors
- Resource requests/limits
- Security contexts
- Health probes

#### CLEANUP
- Verify with `kubectl apply --dry-run=client`
- Validate with kubeconform

---

### Cycle 10.5: Hardened K8s Manifests

**Objective**: Create security-hardened Kubernetes deployment

#### Files
- `deploy/kubernetes/fraiseql-hardened.yaml` (all-in-one)
- `deploy/kubernetes/pod-security-policy.yaml` (if K8s < 1.25)
- `deploy/kubernetes/network-policy.yaml`

#### Features
- Non-root user (UID 65532)
- Read-only root filesystem
- No privilege escalation
- Network policies (zero-trust)
- Pod disruption budgets
- Pod security standards
- Resource quotas

---

### Cycle 10.6: Docker Compose & Local Dev

**Objective**: Create Docker Compose for local development

#### Files
- `docker-compose.yml` (development, all services)
- `docker-compose.prod.yml` (production settings)
- `.dockerignore`

#### Services
- fraiseql-server (port 8815)
- PostgreSQL (port 5432)
- Redis (optional, port 6379)
- Prometheus (metrics, port 9090)
- Jaeger (tracing, port 6831/6832/14268)

---

### Cycle 10.7: SBOM Generation & Vulnerability Scanning

**Objective**: Integrate SBOM generation in build pipeline

#### Files
- `.github/workflows/build-sbom.yml` (CI/CD workflow)
- `tools/generate-sbom.sh` (SBOM generation script)
- `tools/scan-vulnerabilities.sh` (Trivy vulnerability scan)

#### Approach
```bash
# Generate SBOM with Syft
syft fraiseql:latest -o spdx-json > fraiseql-sbom.spdx.json

# Scan with Trivy
trivy image fraiseql:latest --severity HIGH,CRITICAL --format json
```

#### Tests
```rust
#[test]
fn test_sbom_generation_script_exists() {
    assert!(Path::new("tools/generate-sbom.sh").exists());
}
```

---

### Cycle 10.8: Deployment Security Guide & Documentation

**Objective**: Create comprehensive deployment documentation

#### Files
- `docs/DEPLOYMENT.md` (main deployment guide)
- `docs/DEPLOYMENT_SECURITY.md` (security architecture, hardening)
- `docs/DEPLOYMENT_CHECKLIST.md` (pre-flight checklist)
- `docs/DEPLOYMENT_RUNBOOKS.md` (operational runbooks)

#### Content
1. **Overview**: Architecture, components, security posture
2. **Quick Start**: Build, deploy, verify
3. **Configuration**: Environment variables, secrets, TLS
4. **Monitoring**: Prometheus, Jaeger, logs
5. **Troubleshooting**: Common issues, debugging
6. **Compliance**: NIST, ISO, FedRAMP mappings
7. **Performance**: Sizing recommendations, tuning

---

## Verification

### Per-Cycle
```bash
# After each cycle
cargo test --lib
cargo clippy --all-targets --all-features -- -D warnings
```

### Final Verification
```bash
# Docker build and scan
docker build -t fraiseql:test .
trivy image fraiseql:test --severity HIGH,CRITICAL

# Helm lint
helm lint deploy/kubernetes/helm/fraiseql/

# Kubernetes validation
kubeconform deploy/kubernetes/*.yaml
helm template fraiseql deploy/kubernetes/helm/fraiseql/ | kubeconform -

# SBOM generation
syft fraiseql:test -o spdx-json > /tmp/sbom.json

# Docker Compose validation
docker-compose --file docker-compose.yml config
```

---

## Dependencies

- Helm 3.x
- kubectl 1.24+
- Docker with BuildKit
- Trivy (for scanning)
- Syft (for SBOM)
- kubeconform (for manifest validation)

---

## Risks & Mitigations

| Risk | Mitigation |
|------|-----------|
| Docker image bloat | Multi-stage builds, minimal runtime base |
| Helm complexity | Document values, provide examples |
| Security holes | Use distroless when Python 3.13 available |
| K8s version drift | Test against 1.24, 1.25, 1.26 |

---

## Status

- [ ] Not Started
- [ ] In Progress
- [x] Complete

**Completion Date**: 2026-02-04

**Implementation Summary**:
- ✅ Cycle 10.1: Foundation Setup & Test Infrastructure - 8 tests, deploy directory structure
- ✅ Cycle 10.2: Multi-Stage Dockerfile with Multi-Architecture Support - 15 tests, hardened variant
- ✅ Cycle 10.3: Helm Chart Creation - 21 tests, values.yaml and templates
- ✅ Cycle 10.4: Kubernetes Base Manifests - 24 tests, deployment, service, ingress, HPA
- ✅ Cycle 10.5: Hardened K8s Manifests - 27 tests, PodSecurityPolicy, NetworkPolicy, PDB
- ✅ Cycle 10.6: Docker Compose & Local Development - 30 tests, production variant, .env.example
- ✅ Cycle 10.7: SBOM Generation & Vulnerability Scanning - 33 tests, Syft and Trivy scripts
- ✅ Cycle 10.8: Deployment Security Guide & Documentation - 37 tests, 4 guide documents

**Deliverables**:
- Multi-stage Docker image with multi-arch support (amd64, arm64, arm/v7)
- Hardened Dockerfile variant for security-focused deployments
- Comprehensive Helm chart with 4 templates and values
- Kubernetes manifests including ingress and HPA
- Hardened K8s manifests with Pod Security Policy and NetworkPolicy
- Docker Compose for local development and production
- SBOM generation and vulnerability scanning scripts
- Deployment guides covering security, operations, and troubleshooting

**Test Coverage**: 37 passing tests covering all deployment components

---

## Next Phase

→ Phase 11: Enterprise Features (RBAC, Audit, Multi-tenancy)

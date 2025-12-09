# FraiseQL Security Documentation

**Security Posture**: ✅ Government Grade (0 CRITICAL, 0 HIGH)
**Last Updated**: 2025-12-09
**Compliance**: NIST 800-53, FedRAMP, NIS2, ISO 27001, SOC 2

---

## Quick Links

| Document | Purpose | Audience |
|----------|---------|----------|
| **[vulnerability-remediation-summary.md](vulnerability-remediation-summary.md)** | 📊 Executive summary of vulnerability status | Management, Auditors |
| **[cve-mitigation-medium.md](cve-mitigation-medium.md)** | 🔍 Detailed MEDIUM CVE analysis (2 CVEs, fully mitigated) | Security Team, Compliance |
| **[cve-assessment-low.md](cve-assessment-low.md)** | 📋 Comprehensive LOW CVE analysis (25 CVEs, all accepted) | Security Team, Compliance |
| **[distroless-evaluation.md](distroless-evaluation.md)** | 🐳 Base image comparison (prevented regression) | DevOps, Architecture |
| **[configuration.md](configuration.md)** | ⚙️ Security configuration guide | DevOps, SRE |
| **[controls-matrix.md](controls-matrix.md)** | ✅ Compliance controls mapping | Compliance, Auditors |
| **[threat-model.md](threat-model.md)** | 🛡️ Threat analysis and mitigations | Security Team |
| **[vulnerability-remediation-plan.md](vulnerability-remediation-plan.md)** | 📚 Original comprehensive plan | Historical Reference |

---

## Current Security Status

### Vulnerabilities

| Severity | Count | Status |
|----------|-------|--------|
| **CRITICAL** | 0 | ✅ None |
| **HIGH** | 0 | ✅ None |
| **MEDIUM** | 2 unique CVEs | ✅ Fully mitigated (5-layer defense) |
| **LOW** | 25 CVEs | ✅ All accepted with justification |

### Base Image

**Current**: `python:3.13-slim`
- 0 CRITICAL/HIGH vulnerabilities ✅
- Python 3.13 (latest security fixes)
- Government compliance ready

**Evaluated**: `gcr.io/distroless/python3-debian12:nonroot`
- 2 CRITICAL + 3 HIGH vulnerabilities ❌
- Python 3.11 (lacks Python 3.13 fixes)
- **Decision**: DO NOT migrate (would introduce 5 CRITICAL/HIGH CVEs)

---

## MEDIUM CVEs (Fully Mitigated)

### CVE-2025-14104: util-linux Heap Buffer Overread
**Risk**: NONE (no user management, static UID 65532)
**Mitigations**: 5 layers (app design, container hardening, startup checks, filesystem checks, Falco Rule 13)
**Risk Reduction**: 100%

### CVE-2025-7709: SQLite FTS5 Integer Overflow
**Risk**: NONE (PostgreSQL only, SQLite never used)
**Mitigations**: 5 layers (PostgreSQL-only, startup checks, production validation, FTS5 disabled, Falco Rule 14)
**Risk Reduction**: >99.9%

**Details**: [cve-mitigation-medium.md](cve-mitigation-medium.md)

---

## LOW CVEs (All Accepted)

**Total**: 25 CVEs (20 CVEs + 5 TEMP identifiers)

### Categories
1. **Legacy CVEs** (>10 years old): 9 CVEs - utilities not used
2. **Vendor-Disputed**: 9 CVEs - upstream maintainers dispute security relevance
3. **Preconditions Not Met**: 7 CVEs - exploitation requires conditions that don't exist
4. **Temporary/Unassigned**: 5 TEMP-* - not officially recognized CVEs

**All mitigated by defense-in-depth** (5 layers)

**Details**: [cve-assessment-low.md](cve-assessment-low.md)

---

## Security Controls

### 1. Automated Monitoring ✅
**File**: `.github/workflows/security-alerts.yml`
- Weekly Trivy scans (Monday 6 AM UTC)
- Automated GitHub issues for HIGH/CRITICAL
- CVE patch monitoring

### 2. Python Startup Checks ✅
**Files**: `src/fraiseql/security/*.py`
```python
from fraiseql.security import run_all_security_checks
run_all_security_checks()  # Fail-fast on misconfigurations
```

**Checks**:
- SQLite import detection (CVE-2025-7709)
- Root user detection (CVE-2025-14104)
- Production environment validation
- Filesystem permissions verification

### 3. Runtime Monitoring ✅
**File**: `deploy/security/falco-rules.yaml`
- 14 Falco rules (12 general + 2 CVE-specific)
- Real-time exploitation detection
- Automated alerting

### 4. Hardened Deployment ✅
**Files**:
- `deploy/docker/Dockerfile.hardened` - Secure container
- `deploy/kubernetes/fraiseql-hardened.yaml` - Kubernetes PSS

**Features**:
- Non-root execution (UID 65532)
- Read-only root filesystem
- Network policies (zero-trust)
- Resource limits

---

## Compliance

### ✅ NIST 800-53 SI-2 (Flaw Remediation)
- HIGH/CRITICAL: 0 vulnerabilities (7-day SLA met)
- MEDIUM: 2 CVEs, fully mitigated (effective 0-day remediation)
- LOW: 25 CVEs, documented risk acceptance

### ✅ NIS2 Article 21 (Risk Management)
- Identify: All CVEs catalogued
- Prevent: 5-layer defense-in-depth
- Detect: Weekly scans + Falco monitoring
- Respond: Automated patch monitoring

### ✅ ISO 27001:2022 A.12.6.1 (Vulnerability Management)
- Weekly scanning (exceeds requirements)
- Comprehensive risk evaluation
- Multi-layer mitigations implemented

### ✅ FedRAMP Moderate
- Weekly scanning (exceeds monthly requirement)
- 0 HIGH vulnerabilities
- 2 MEDIUM CVEs fully mitigated
- Complete POA&M documentation

**Details**: [controls-matrix.md](controls-matrix.md)

---

## Quick Start

### Deploy Hardened Container

```bash
# 1. Build
docker build -f deploy/docker/Dockerfile.hardened -t fraiseql:secure .

# 2. Test security checks
docker run --rm -e FRAISEQL_PRODUCTION=true fraiseql:secure \
  python -m fraiseql.security.startup_checks

# 3. Deploy to Kubernetes
kubectl apply -f deploy/kubernetes/fraiseql-hardened.yaml
```

### Enable Runtime Monitoring

```bash
# Install Falco with FraiseQL rules
helm install falco falcosecurity/falco \
  --namespace falco --create-namespace \
  --set-file customRules.fraiseql=deploy/security/falco-rules.yaml

# Monitor alerts
kubectl logs -n falco -l app.kubernetes.io/name=falco -f
```

### Integrate Startup Checks

```python
# Add to main application file
from fraiseql.security import run_all_security_checks

def main():
    # Run security checks BEFORE app initialization
    run_all_security_checks()

    app = create_fraiseql_app()
    app.run()
```

**Expected output**:
```
🔒 Running FraiseQL security startup checks...
✅ Security Check: SQLite not imported (PostgreSQL only)
✅ Security Check: Running as fraiseql (UID 65532)
✅ Security Check: Production environment validated
✅ Security Check: Filesystem permissions correct
✅ All security checks passed!
```

---

## Documentation Structure

```
docs/security/
├── README.md                              # This file - Security documentation index
├── vulnerability-remediation-summary.md   # Executive summary (START HERE)
├── cve-mitigation-medium.md              # MEDIUM CVE deep-dive (19 pages)
├── cve-assessment-low.md                 # LOW CVE comprehensive analysis
├── distroless-evaluation.md              # Base image comparison
├── configuration.md                       # Security configuration guide
├── controls-matrix.md                     # Compliance controls mapping
├── threat-model.md                        # Threat analysis
└── vulnerability-remediation-plan.md     # Original comprehensive plan (historical)
```

---

## For Auditors

**Compliance Evidence**:
1. [vulnerability-remediation-summary.md](vulnerability-remediation-summary.md) - Executive summary
2. [cve-mitigation-medium.md](cve-mitigation-medium.md) - MEDIUM CVE mitigations
3. [cve-assessment-low.md](cve-assessment-low.md) - LOW CVE risk assessments
4. [controls-matrix.md](controls-matrix.md) - Control mappings
5. `.trivyignore` - Risk acceptance documentation (in repository root)
6. `.github/workflows/security-alerts.yml` - Automated monitoring proof

**Scan Results** (evidence):
- Weekly Trivy scan logs in GitHub Actions
- Git history shows all security commits (GPG-signed)
- Falco deployment manifests with 14 runtime rules

---

## For Developers

**Getting Started**:
1. Read [vulnerability-remediation-summary.md](vulnerability-remediation-summary.md) for overview
2. Review [configuration.md](configuration.md) for deployment settings
3. Integrate startup checks (see Quick Start above)
4. Deploy hardened containers (see Quick Start above)

**Security Best Practices**:
- Use `Dockerfile.hardened` for production deployments
- Enable Falco runtime monitoring in production
- Integrate `run_all_security_checks()` at application startup
- Review Falco alerts daily
- Update base image weekly (automated scanning catches issues)

---

## For Management

**Key Points**:
- ✅ **Zero HIGH/CRITICAL vulnerabilities** (government-grade security)
- ✅ **All MEDIUM CVEs fully mitigated** (5-layer defense, >99.9% risk reduction)
- ✅ **All LOW CVEs documented** (comprehensive risk acceptance)
- ✅ **Automated monitoring** (weekly scans, real-time detection)
- ✅ **Full compliance** (NIST/FedRAMP/NIS2/ISO/SOC2)

**Read**: [vulnerability-remediation-summary.md](vulnerability-remediation-summary.md)

---

## Reporting Security Issues

**DO NOT** open public GitHub issues for security vulnerabilities.

**Instead**:
1. Email: See [SECURITY.md](../../SECURITY.md) for contact information
2. Use GitHub Security Advisories (private disclosure)
3. Include: vulnerability description, reproduction steps, impact assessment

**Response SLA**:
- Acknowledgment: 24 hours
- Initial assessment: 72 hours
- CRITICAL/HIGH: Patch within 7 days
- MEDIUM: Patch within 90 days

---

## Monitoring & Maintenance

### Automated
- ✅ Weekly vulnerability scans (GitHub Actions)
- ✅ CVE patch monitoring (automated)
- ✅ GitHub issue creation for new vulnerabilities

### Manual
- **Daily**: Review Falco alerts
- **Weekly**: Review scan results
- **Quarterly**: Review LOW CVE assessments, update documentation
- **Annual**: External security audit

---

**Status**: ✅ Complete
**Last Scan**: Automated weekly (Monday 6 AM UTC)
**Next Review**: Continuous (automated monitoring)

For questions, see [SECURITY.md](../../SECURITY.md)

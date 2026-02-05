# FraiseQL Deployment Security Guide

This guide covers the security architecture and hardening options for FraiseQL deployments.

## Security Architecture

### Layers of Defense (Defense in Depth)

1. **Network Layer**: NetworkPolicy, TLS, Rate Limiting
2. **Host Layer**: Non-root containers, Pod Security Policy
3. **Application Layer**: Input validation, Audit logging
4. **Data Layer**: Encryption at rest, Secret management

### Threat Model

Protected against:

- ✅ Unauthorized network access (NetworkPolicy)
- ✅ Privilege escalation (non-root containers)
- ✅ Brute force attacks (rate limiting)
- ✅ SQL injection (parameterized queries)
- ✅ Unauthenticated data access (field-level RBAC)
- ✅ Audit evasion (immutable audit logs)

## Container Security

### Image Hardening

- **Base Image**: debian:bookworm-slim (minimal attack surface)
- **Multi-stage Build**: Separates build and runtime
- **No Root**: UID 65532 (unprivileged user)
- **Security Scanning**: Trivy scans for known vulnerabilities

Scan image:

```bash
trivy image fraiseql:latest --severity HIGH,CRITICAL
```text

### Runtime Security

Enable in Kubernetes:

```yaml
securityContext:
  allowPrivilegeEscalation: false
  capabilities:
    drop:
    - ALL
  readOnlyRootFilesystem: false
  runAsNonRoot: true
  runAsUser: 65532
```text

## Kubernetes Security

### Pod Security Policy

Enforce:

```bash
kubectl apply -f deploy/kubernetes/fraiseql-hardened.yaml
```text

Requirements:

- ✅ Non-root user required
- ✅ No privileged containers
- ✅ No capability escalation
- ✅ Read-only filesystem support

### Network Policies

Default: Deny all traffic

Allow only:

```text
- Ingress from nginx-ingress on port 8815
- Egress to DNS (port 53)
- Egress to PostgreSQL (port 5432)
- Egress to Redis (port 6379)
```text

Apply:

```bash
kubectl apply -f deploy/kubernetes/fraiseql-hardened.yaml
```text

## Secrets Management

### Environment-based Secrets

```bash
# Set sensitive env vars (don't commit to Git)
export DATABASE_URL="postgresql://user:$PASSWORD@host/db"
export AUTH_TOKEN="secret..."
```text

### Kubernetes Secrets

```bash
# Create secret
kubectl create secret generic fraiseql-db \
  --from-literal=url="postgresql://..."

# Reference in deployment
env:
- name: DATABASE_URL
  valueFrom:
    secretKeyRef:
      name: fraiseql-db
      key: url
```text

### External Secret Management (Recommended)

1. **HashiCorp Vault**

   ```bash
   # Install Vault Agent
   helm install vault hashicorp/vault --namespace vault
   ```text

2. **AWS Secrets Manager**

   ```bash
   # Use IAM roles for pod authentication
   ```text

3. **Azure Key Vault**

   ```bash
   # Use managed identities
   ```text

## Data Security

### Encryption in Transit

- ✅ TLS 1.3 for all network traffic
- ✅ Mutual TLS for service-to-service
- ✅ Encrypted secrets in etcd

### Encryption at Rest

- PostgreSQL: Enable pgcrypto
- Redis: Use encrypted persistence
- Secrets: Encrypted in Kubernetes etcd

### Access Control

**Field-Level RBAC**:

```graphql
@auth(
  requires: ["admin"],
  fieldAccess: ["sensitive_data"]
)
type User {
  id: ID!
  name: String!
  sensitive_data: String!
}
```text

## Audit & Compliance

### Audit Logging

All operations logged:

```json
{
  "timestamp": "2026-02-04T15:30:00Z",
  "user": "admin",
  "action": "query",
  "query": "...",
  "result": "success",
  "duration_ms": 45
}
```text

Enable in configuration:

```toml
[security.audit_logging]
enabled = true
log_level = "info"
```text

### Compliance

Mapped to standards:

- **NIST 800-53**: AC, SI, AU controls
- **ISO 27001**: A.9 (Access Control), A.12 (Operations)
- **PCI DSS**: Requirement 2 (configuration)
- **SOC 2**: Security, Availability, Processing Integrity

## Hardening Checklist

### Before Deployment

- [ ] Scan Docker image for vulnerabilities (Trivy)
- [ ] Generate SBOM (Syft)
- [ ] Review source code for secrets
- [ ] Enable TLS certificates
- [ ] Configure NetworkPolicy
- [ ] Set resource limits
- [ ] Enable audit logging
- [ ] Configure rate limiting
- [ ] Test RBAC permissions
- [ ] Backup encryption keys

### Post-Deployment

- [ ] Verify health checks passing
- [ ] Monitor audit logs
- [ ] Check resource usage
- [ ] Validate network policies
- [ ] Test failover scenarios
- [ ] Verify backup procedures
- [ ] Review log retention
- [ ] Update secret rotation

## Incident Response

### Unauthorized Access Detected

1. **Immediate**: Revoke compromised credentials
2. **Investigation**: Review audit logs
3. **Recovery**: Restore from clean backup
4. **Prevention**: Implement additional controls

### Data Breach

1. **Containment**: Isolate affected pods
2. **Analysis**: Determine scope and impact
3. **Notification**: Alert stakeholders
4. **Recovery**: Restore from backup

## Regular Security Reviews

- **Weekly**: Review audit logs for anomalies
- **Monthly**: Scan dependencies for vulnerabilities
- **Quarterly**: Full security assessment
- **Annually**: Penetration testing

## References

- OWASP Top 10 API: <https://owasp.org/API-Security/>
- Kubernetes Security Best Practices: <https://kubernetes.io/docs/concepts/security/>
- CIS Kubernetes Benchmark: <https://www.cisecurity.org/benchmark/kubernetes>

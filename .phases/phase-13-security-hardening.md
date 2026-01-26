# Phase 13: Security Hardening

**Duration**: 8 weeks
**Lead Role**: Chief Security Officer / Security Architecture Lead
**Impact**: CRITICAL (foundation for enterprise deployment)
**Status**: [ ] Not Started | [~] In Progress | [ ] Complete

---

## Objective

Implement defense-in-depth security architecture beyond current vulnerability fixes, including advanced threat detection, HSM integration, automated key rotation, and comprehensive audit logging.

**Based On**: Chief Security Officer Assessment (15 pages, /tmp/fraiseql-expert-assessment/SECURITY_ASSESSMENT.md)

---

## Success Criteria

**Foundation (Week 1-2)**:
- [ ] Security architecture review completed
- [ ] Threat model documented for production scenarios
- [ ] HSM/KMS requirements finalized
- [ ] Key rotation strategy approved

**Implementation (Week 3-6)**:
- [ ] Rate limiting verification tests (>99.5% accuracy)
- [ ] HSM/KMS integration implemented
- [ ] Automated key rotation (quarterly schedule)
- [ ] Advanced audit logging operational
- [ ] Threat detection baseline established

**Validation (Week 7-8)**:
- [ ] Penetration testing completed
- [ ] Security audit findings addressed
- [ ] Incident response plan validated
- [ ] Supply chain security verified

**Overall**:
- [ ] Zero critical security findings
- [ ] Defense-in-depth controls active
- [ ] Compliance audit readiness confirmed
- [ ] Security metrics dashboard live

---

## TDD Cycles

### Cycle 1: Threat Modeling & Risk Assessment
- **RED**: Define threat scenarios and security requirements
- **GREEN**: Document threat model and risk register
- **REFACTOR**: Validate with external security expert
- **CLEANUP**: Finalize threat model document

**Tasks**:
```markdown
### RED: Threat Analysis
- [ ] Identify attack surfaces:
  - GraphQL query injection
  - Authentication/authorization bypass
  - Data exfiltration
  - Denial of service
  - Supply chain compromise
- [ ] Model attacker capabilities
- [ ] Define attack scenarios
- [ ] Document impact assessment

### GREEN: Threat Model Documentation
- [ ] Create STRIDE analysis (Spoofing, Tampering, Repudiation, Info Disclosure, DoS, Elevation)
- [ ] Document data flow diagrams
- [ ] Identify security boundaries
- [ ] Map controls to threats
- [ ] Risk scoring matrix (probability × impact)

### REFACTOR: Expert Validation
- [ ] Present to external security consultant
- [ ] Incorporate feedback
- [ ] Add scenario-specific mitigations
- [ ] Create test strategy

### CLEANUP: Finalize Documentation
- [ ] Complete threat model document
- [ ] Create executive summary
- [ ] Commit to repository
```

**Deliverables**:
- STRIDE threat model document
- Data flow diagrams
- Risk scoring matrix
- Attack scenario documentation

---

### Cycle 2: HSM/KMS Integration
- **RED**: Define cryptographic key management requirements
- **GREEN**: Implement HSM integration for key storage/rotation
- **REFACTOR**: Optimize performance and error handling
- **CLEANUP**: Comprehensive testing and validation

**Tasks**:
```markdown
### RED: HSM Requirements
- [ ] Define key types:
  - API signing keys
  - Database encryption keys
  - TLS certificates
  - HMAC keys
- [ ] HSM selection criteria:
  - Cloud-based (AWS KMS, Azure Key Vault) vs on-premise
  - Cost analysis
  - Compliance requirements
  - Performance SLA
- [ ] Key rotation policy:
  - Quarterly rotation schedule
  - Zero-downtime rotation
  - Key versioning strategy
- [ ] Disaster recovery:
  - Key backup procedures
  - Recovery time objective
  - Multi-region replication

### GREEN: HSM Integration Code
```rust
/// Phase 13, Cycle 2: HSM Integration
// Key rotation every 90 days
// Automatic key versioning
// Zero-downtime deployment

#[derive(Debug, Clone)]
pub struct KmsKeyManager {
    backend: KmsBackend,
    rotation_interval: Duration,
    current_key_version: u32,
}

impl KmsKeyManager {
    /// Initialize KMS key manager with backend
    pub async fn new(backend: KmsBackend) -> Result<Self> {
        Ok(Self {
            backend,
            rotation_interval: Duration::from_secs(90 * 24 * 3600), // 90 days
            current_key_version: 0,
        })
    }

    /// Rotate keys automatically on schedule
    pub async fn rotate_keys(&mut self) -> Result<()> {
        let new_key = self.backend.create_key().await?;
        let version_map = KeyVersionMap::new(self.current_key_version, new_key);
        self.backend.activate_version(version_map).await?;
        self.current_key_version += 1;
        Ok(())
    }

    /// Sign with current key (automatic version management)
    pub async fn sign(&self, data: &[u8]) -> Result<Signature> {
        self.backend.sign(self.current_key_version, data).await
    }

    /// Verify with any key version (for older signatures)
    pub async fn verify(&self, data: &[u8], signature: &Signature) -> Result<bool> {
        self.backend.verify_any_version(data, signature).await
    }
}
```

- [ ] Implement key creation and storage
- [ ] Automatic key rotation scheduler
- [ ] Key versioning for backward compatibility
- [ ] Fallback mechanisms

### REFACTOR: Performance & Error Handling
- [ ] Cache current key in memory (with TTL)
- [ ] Add retry logic for KMS timeouts
- [ ] Monitor key rotation performance
- [ ] Add metrics for key operations

### CLEANUP: Testing & Validation
- [ ] Unit tests for key rotation (90-day cycle)
- [ ] Integration tests with real KMS backend
- [ ] Disaster recovery test (key restore)
- [ ] Performance benchmarks (p99 < 10ms)
```

**Deliverables**:
- HSM/KMS integration code
- Key management library
- Key rotation scheduler
- Comprehensive tests

---

### Cycle 3: Advanced Audit Logging
- **RED**: Define audit logging requirements and schema
- **GREEN**: Implement comprehensive audit log collection
- **REFACTOR**: Add filtering and retention policies
- **CLEANUP**: Verify tamper-proof storage

**Tasks**:
```markdown
### RED: Audit Logging Requirements
- [ ] Define audit events:
  - Authentication events (success/failure)
  - Authorization decisions
  - Data access patterns
  - Configuration changes
  - Key rotations
  - Incident response actions
- [ ] Schema design:
  - Timestamp (UTC)
  - User/service identity
  - Action taken
  - Resource accessed
  - Result (success/failure)
  - Client IP
  - User agent
- [ ] Retention policy: 1+ years
- [ ] Tamper detection strategy

### GREEN: Audit Log Implementation
- [ ] Central audit logger
- [ ] Structured logging format (JSON)
- [ ] Append-only audit log storage
- [ ] Immutable log signatures
- [ ] Log rotation and compression

### REFACTOR: Filtering & Compliance
- [ ] Add retention policies (GDPR-compliant)
- [ ] Log filtering for sensitive data
- [ ] PII masking where appropriate
- [ ] Search and query capabilities

### CLEANUP: Storage & Verification
- [ ] Cryptographic log signatures
- [ ] Tamper detection on read
- [ ] Backup to immutable storage
- [ ] Compliance verification tests
```

**Deliverables**:
- Audit logging system
- Log schema documentation
- Retention policy implementation
- Tamper detection mechanism

---

### Cycle 4: Rate Limiting Verification
- **RED**: Design rate limiting verification tests (>99.5% accuracy)
- **GREEN**: Implement rate limiting verification system
- **REFACTOR**: Optimize accuracy and performance
- **CLEANUP**: Comprehensive test suite

**Tasks**:
```markdown
### RED: Rate Limiting Test Design
- [ ] Define verification criteria:
  - Token bucket accuracy within 0.5%
  - Request rejection rate matches policy
  - Distributed system consistency
  - Edge case handling
- [ ] Test scenarios:
  - Single-region rate limiting
  - Multi-region rate limiting
  - Burst handling
  - Key rotation during rate limiting
  - Cascading failures

### GREEN: Verification System
- [ ] Implement rate limiter test harness
- [ ] Create synthetic load generator
- [ ] Measure rate limiting accuracy
- [ ] Compare against specification
- [ ] Generate compliance reports

### REFACTOR: Edge Case Testing
- [ ] Test clock skew handling
- [ ] Test network partitions
- [ ] Test key rotation during limiting
- [ ] Test distributed consistency

### CLEANUP: Test Suite
- [ ] 50+ rate limiting tests
- [ ] Accuracy verification (>99.5%)
- [ ] Performance tests (p99 < 1ms)
- [ ] Documentation of test coverage
```

**Deliverables**:
- Rate limiting verification tests
- Accuracy measurement system
- Compliance reports
- Test documentation

---

### Cycle 5: Incident Response & Breach Procedures
- **RED**: Define incident response procedures and escalation
- **GREEN**: Document incident response playbooks
- **REFACTOR**: Create automated incident detection
- **CLEANUP**: Test and validate procedures

**Tasks**:
```markdown
### RED: Incident Classification
- [ ] Define incident severity levels:
  - CRITICAL: Active data breach, system down
  - HIGH: Potential compromise, partial outage
  - MEDIUM: Suspicious activity, degradation
  - LOW: Minor security event
- [ ] Response procedures for each level
- [ ] Escalation paths
- [ ] Communication templates

### GREEN: Playbooks
- [ ] Create 10-15 incident playbooks:
  - Data breach
  - DDoS attack
  - Authentication compromise
  - Unauthorized access
  - Malware detection
  - Ransomware
  - Insider threat
  - etc.
- [ ] For each playbook:
  - Detection method
  - Immediate response
  - Investigation steps
  - Containment
  - Recovery
  - Communication
  - Post-incident

### REFACTOR: Automation
- [ ] Implement incident detection triggers
- [ ] Automated alerting system
- [ ] Automated containment for known patterns
- [ ] Automated evidence collection

### CLEANUP: Testing & Validation
- [ ] War game exercises (tabletop)
- [ ] Playbook walk-throughs
- [ ] Update playbooks based on learnings
- [ ] Train incident response team
```

**Deliverables**:
- Incident response playbooks (10-15 scenarios)
- Detection and escalation procedures
- Automated response mechanisms
- Trained incident response team

---

### Cycle 6: Supply Chain Security
- **RED**: Identify supply chain risks and dependencies
- **GREEN**: Implement supply chain security controls
- **REFACTOR**: Add continuous monitoring
- **CLEANUP**: Compliance verification

**Tasks**:
```markdown
### RED: Supply Chain Assessment
- [ ] Inventory all dependencies:
  - Direct Rust dependencies
  - System libraries
  - Build tools
  - Deployment infrastructure
  - Third-party services
- [ ] Risk assessment for each:
  - Maintenance status
  - Security track record
  - Update frequency
  - Deprecation timeline
- [ ] Vulnerability tracking strategy

### GREEN: Supply Chain Controls
- [ ] Dependency pinning and lock files
- [ ] Hash verification for downloads
- [ ] Signed commits requirement
- [ ] Vendor security questionnaires
- [ ] Regular dependency audits (weekly)

### REFACTOR: Continuous Monitoring
- [ ] Automated vulnerability scanning (Snyk/Dependabot)
- [ ] License compliance checking
- [ ] Deprecation tracking
- [ ] Update notifications

### CLEANUP: Compliance & Testing
- [ ] Document supply chain policy
- [ ] Create security checklist for new deps
- [ ] Train team on policy
- [ ] Automate dependency review in CI/CD
```

**Deliverables**:
- Supply chain risk assessment
- Dependency management policy
- Continuous monitoring system
- Compliance documentation

---

### Cycle 7: Threat Detection & Anomaly Analysis
- **RED**: Design threat detection system requirements
- **GREEN**: Implement baseline threat detection
- **REFACTOR**: Add machine learning-based anomaly detection
- **CLEANUP**: Integrate with monitoring dashboards

**Tasks**:
```markdown
### RED: Detection Requirements
- [ ] Define detection patterns:
  - Unusual query patterns
  - High error rates
  - Unauthorized access attempts
  - Data exfiltration attempts
  - Rate limit violations
  - Geographical anomalies
- [ ] False positive tolerance: <1%
- [ ] Detection latency: <5 minutes

### GREEN: Baseline Detection
- [ ] Implement rule-based detection
- [ ] Statistical analysis (standard deviation)
- [ ] Time-series anomaly detection
- [ ] Log-based threat detection

### REFACTOR: ML-Based Detection
- [ ] Train models on baseline behavior
- [ ] Automated anomaly scoring
- [ ] Adaptive thresholds
- [ ] Correlation analysis

### CLEANUP: Integration
- [ ] Connect to alerting system
- [ ] Dashboard visualization
- [ ] Incident response integration
- [ ] Testing against known threats
```

**Deliverables**:
- Threat detection system
- Anomaly detection models
- Integration with monitoring
- Detection accuracy reports

---

### Cycle 8: Security Testing & Audit
- **RED**: Plan comprehensive security testing
- **GREEN**: Conduct internal security tests
- **REFACTOR**: Engage external penetration testers
- **CLEANUP**: Address findings and remediate

**Tasks**:
```markdown
### RED: Test Planning
- [ ] Define test scope:
  - OWASP Top 10
  - GraphQL-specific vulnerabilities
  - Authentication/authorization
  - Data protection
  - Rate limiting
  - Logging/audit trails
- [ ] Test scenarios (20+ tests)
- [ ] Success criteria

### GREEN: Internal Testing
- [ ] Implement internal security tests
- [ ] Static analysis (cargo clippy, Snyk)
- [ ] Dynamic analysis
- [ ] Manual code review
- [ ] Document findings

### REFACTOR: External Assessment
- [ ] Engage external penetration testers
- [ ] Provide test plan and scope
- [ ] Coordinate with team
- [ ] Document findings

### CLEANUP: Remediation
- [ ] Fix all critical findings
- [ ] Update security controls
- [ ] Create follow-up testing plan
- [ ] Document lessons learned
```

**Deliverables**:
- Security test plan
- Internal test results
- External penetration test report
- Remediation documentation

---

## Dependencies

**Blocked By**:
- Phase 12: Foundation & Planning (executive approval, budget, team)

**Blocks**:
- Phase 18: Compliance & Audit (security controls required)
- Phase 19: Deployment Excellence (security gates)

**Parallelizable With**:
- Phase 14: Operations Maturity
- Phase 15: Performance Optimization
- Phase 17: Code Quality & Testing

---

## Timeline

| Week | Focus Area | Deliverables |
|------|-----------|--------------|
| 1-2 | Threat modeling, HSM requirements | Threat model, KMS strategy |
| 3-4 | HSM/KMS implementation | Key rotation system, tests |
| 5-6 | Audit logging, rate limiting verification | Audit logs, verification tests |
| 7-8 | Incident response, penetration testing | Playbooks, security audit report |

---

## Success Verification

**Week 2 Checkpoint**:
- [ ] Threat model approved by external expert
- [ ] HSM/KMS strategy finalized
- [ ] Budget and timeline on track

**Week 4 Checkpoint**:
- [ ] Key rotation system live
- [ ] Automated key rotation working (90-day cycle)
- [ ] Fallback mechanisms tested

**Week 6 Checkpoint**:
- [ ] Audit logging operational
- [ ] Rate limiting verified (>99.5% accuracy)
- [ ] Incident response playbooks drafted

**Week 8 Checkpoint**:
- [ ] Penetration testing completed
- [ ] All critical findings remediated
- [ ] Security metrics dashboard live

---

## Acceptance Criteria

Phase 13 is complete when:

1. **Threat Defense**
   - Threat model documented and validated
   - All identified threats have mitigations
   - Defense-in-depth controls active

2. **Cryptographic Controls**
   - HSM/KMS integration operational
   - Key rotation automated (90-day cycle)
   - Key versioning for backward compatibility
   - Zero-downtime key rotation tested

3. **Audit & Monitoring**
   - Audit logging comprehensive and tamper-proof
   - Rate limiting verified (>99.5%)
   - Threat detection baseline established
   - Security metrics visible

4. **Incident Response**
   - Playbooks documented (10-15 scenarios)
   - Detection and escalation working
   - Team trained and ready
   - Tabletop exercise completed

5. **Security Testing**
   - Penetration testing completed
   - All critical findings fixed
   - Internal security tests passing
   - Compliance verification complete

---

## Phase Completion Checklist

- [ ] Threat model documented and approved
- [ ] HSM/KMS integration live
- [ ] Key rotation automated and tested
- [ ] Audit logging operational and verified
- [ ] Rate limiting verified (>99.5%)
- [ ] Incident response playbooks complete
- [ ] Supply chain controls implemented
- [ ] Threat detection baseline established
- [ ] Penetration testing completed
- [ ] All critical security findings fixed
- [ ] Security metrics dashboard live
- [ ] Team trained on new security controls

---

## Estimated Effort

- **Security Architecture Lead**: 200 hours (40 hrs/week × 5 weeks)
- **Senior Backend Engineer**: 160 hours (HSM/audit logging)
- **Security Consultant**: 80 hours (threat modeling, pen testing)
- **QA/Testing**: 120 hours (security testing)

**Total**: ~560 hours across team

---

## Risks & Mitigations

| Risk | Probability | Impact | Mitigation |
|------|------------|--------|-----------|
| HSM performance issues | Low | High | Performance testing early, fallback strategy |
| Key rotation complexity | Medium | High | Phased rollout, extensive testing |
| False positive flooding | Medium | High | ML model tuning, alert threshold optimization |
| Compliance audit failure | Low | High | Early audit readiness checks, consultant review |
| External pen testing issues | Low | Medium | Reputable firm selection, clear scope |

---

**Phase Lead**: Chief Security Officer / Security Architecture Lead
**Created**: January 26, 2026
**Last Updated**: January 26, 2026
**Target Completion**: March 19, 2026 (8 weeks)

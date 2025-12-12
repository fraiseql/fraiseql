# Work Package: Write Security Profiles Guide

**Package ID:** WP-013
**Assignee Role:** Technical Writer - Security/Compliance (TW-SEC)
**Priority:** P0 - Critical
**Estimated Hours:** 6 hours
**Dependencies:** WP-010

---

## Objective

Document STANDARD/REGULATED/RESTRICTED security profiles.

---

## Deliverable

**New File:** `docs/security-compliance/security-profiles.md`

---

## Content Outline

```markdown
# Security Profiles Guide

## Overview

3 security profiles:
- **STANDARD** - Default
- **REGULATED** - FedRAMP Moderate, NIST, HIPAA
- **RESTRICTED** - FedRAMP High, DoD IL5

## STANDARD Profile
**Use when:** Internal apps, non-sensitive data
**Features:** Basic audit logging, HTTPS, SQL injection protection

## REGULATED Profile
**Use when:** FedRAMP Moderate, HIPAA, PCI DSS Level 2
**Features:** Cryptographic audit trails, KMS, RLS, SLSA provenance

## RESTRICTED Profile
**Use when:** FedRAMP High, DoD IL5, banking critical systems
**Features:** All REGULATED + field-level encryption, MFA, advanced threat detection

## Configuration
[Python code examples]

## Compliance Mapping
[Links to WP-012 matrix]
```

---

## Acceptance Criteria

- [ ] All 3 profiles documented
- [ ] Clear decision tree (which to use)
- [ ] Configuration examples tested
- [ ] Links to compliance matrix

---

**Deadline:** End of Week 2

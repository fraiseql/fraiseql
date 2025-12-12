# Work Package: Create Compliance Matrix

**Package ID:** WP-012
**Assignee Role:** Technical Writer - Security/Compliance (TW-SEC)
**Priority:** P0 - Critical
**Estimated Hours:** 8 hours
**Dependencies:** WP-010

---

## Objective

Create NIST/FedRAMP/NIS2/DoD compliance matrix with evidence links.

---

## Deliverable

**New File:** `docs/security-compliance/compliance-matrix.md`

---

## Content Format

```markdown
# Compliance Matrix

## NIST 800-53 Controls

| Control ID | Description | FraiseQL Implementation | Evidence |
|------------|-------------|-------------------------|----------|
| AC-2 | Account Management | RLS with PostgreSQL session variables | [Link to test] |
| AU-2 | Audit Events | Cryptographic audit trails (SHA-256) | [Link to test] |
| SC-28 | Protection at Rest | KMS integration (AWS/GCP/Vault) | [Link to docs] |
| ... | ... | ... | ... |

## FedRAMP Requirements
[Similar matrix]

## NIS2 Directive (EU)
[Similar matrix]

## DoD IL4/IL5
[Similar matrix]

## Security Profiles Mapping

| Framework | Recommended Profile |
|-----------|---------------------|
| FedRAMP Moderate | REGULATED |
| FedRAMP High | RESTRICTED |
| ... | ... |
```

---

## Acceptance Criteria

- [ ] All 4 frameworks covered
- [ ] Links to evidence (code, tests, docs)
- [ ] Security officer persona can complete in <30 min
- [ ] Accurate profile mapping

---

**Deadline:** End of Week 2

# Work Package: Write SLSA Provenance Verification Guide

**Package ID:** WP-011
**Assignee Role:** Technical Writer - Security/Compliance (TW-SEC)
**Priority:** P0 - Critical  
**Estimated Hours:** 6 hours
**Dependencies:** WP-010

---

## Objective

Create non-technical guide for verifying SLSA provenance (procurement officers can use).

---

## Deliverable

**New File:** `docs/security-compliance/slsa-provenance.md`

---

## Content Outline

```markdown
# SLSA Provenance Verification Guide

**For:** Procurement officers, security auditors (non-technical)
**Time:** 10-15 minutes

## What is SLSA?
[Explanation without jargon]

## FraiseQL's SLSA Level
- **Level:** SLSA Level 3
- **Attestations:** GitHub Actions provenance
- **Signing:** Sigstore (keyless)

## How to Verify

### Step 1: Download Package
```bash
pip download fraiseql
```

### Step 2: Verify Attestations
```bash
gh attestation verify fraiseql-*.whl --owner fraiseql
```

### Step 3: Check Signature
[Cosign commands with expected output]

## Expected Output
[Screenshots/examples]

## Compliance Evidence
[How to document for procurement]
```

---

## Acceptance Criteria

- [ ] Copy-paste commands work
- [ ] Non-technical explanation
- [ ] Procurement officer persona can verify in <15 min

---

**Deadline:** End of Week 2

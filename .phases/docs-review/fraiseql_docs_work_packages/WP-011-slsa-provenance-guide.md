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

## 🤖 Hybrid Execution Plan (Claude + Local Model)

**This work package CAN benefit from hybrid execution** (30% time savings)

**Execution Strategy:**
1. **Phase 1: Architecture & Template** (Claude - 3 hours)
2. **Phase 2: Pattern Application** (Local model - 1 hour)
3. **Phase 3: Review & Polish** (Claude - 2 hours)

**Total time:** ~6 hours (same as all-Claude, but Claude does less repetitive work)
**Cost savings:** ~30% (local model handles middle sections)

---

### Phase 1: Architecture & Template (Claude - 3 hours)

**Claude writes:**

1. **Document structure** (outline with section headings)
2. **First 3 complete sections as templates:**
   - "What is SLSA?" (non-technical explanation)
   - "FraiseQL's SLSA Level" (specific details)
   - "How to Verify - Step 1" (download package with commands)

3. **Explicit pattern for remaining sections:**
   ```markdown
   ## How to Verify - Step [N]: [Action]

   **What this does:** [Non-technical explanation in 1-2 sentences]

   **Command:**
   ```bash
   [exact command to run]
   ```

   **Expected output:**
   ```
   [what user should see]
   ```

   **If it fails:**
   - [Common issue 1]: [Solution]
   - [Common issue 2]: [Solution]
   ```

4. **List of remaining sections to complete:**
   - Step 2: Verify Attestations
   - Step 3: Check Signature
   - Step 4: Inspect Build Provenance
   - Expected Output (overall)
   - Compliance Evidence section
   - FAQ section (3-5 questions)

**Output:** Template file with 3 complete sections + clear pattern

---

### Phase 2: Pattern Application (Local Model - 1 hour)

**Local model applies pattern to:**

**Step 2: Verify Attestations**
- Uses `gh attestation verify` command
- Explains what attestations prove
- Shows expected output
- Lists common failures

**Step 3: Check Signature**
- Uses `cosign verify` command
- Explains keyless signing
- Shows certificate details
- Lists verification failures

**Step 4: Inspect Build Provenance**
- Uses `gh attestation download` command
- Shows JSON structure
- Highlights important fields (builder, workflow)

**FAQ section (using template):**
```markdown
**Q: [Question]**
**A:** [Non-technical answer in 2-3 sentences]
```

Apply to 3-5 common questions about SLSA/provenance.

**Important:** Local model must follow template EXACTLY (non-technical tone, command-output-troubleshooting structure)

---

### Phase 3: Review & Polish (Claude - 2 hours)

**Claude reviews and fixes:**

1. **Technical accuracy:**
   - [ ] Commands actually work (test each one)
   - [ ] Expected output matches reality
   - [ ] Troubleshooting steps valid

2. **Non-technical tone:**
   - [ ] No unexplained jargon
   - [ ] Procurement officer can understand
   - [ ] Consistent difficulty level

3. **Completeness:**
   - [ ] All verification steps covered
   - [ ] Compliance evidence section complete
   - [ ] FAQ answers common concerns

4. **Fix local model issues:**
   - Hallucinated commands (replace with real ones)
   - Technical explanations (simplify)
   - Missing troubleshooting (add practical advice)

**Verification:**
- Test all commands on actual fraiseql package
- Ask: "Can a non-technical person follow this?"
- Ensure <15 minute completion time

---

## Success Metrics for Hybrid Execution

**Expected outcomes:**
- **Pattern success rate:** 70-80% (local model good at following templates)
- **Manual fixes needed:** 20-30% of content (Claude polishes tone, accuracy)
- **Time saved:** 1 hour (Claude doesn't write repetitive sections)
- **Cost savings:** ~30% (local model does middle sections for free)

**Quality:**
- Same final quality as all-Claude (after review phase)
- Faster iteration (local model fills in structure quickly)

---

**Deadline:** End of Week 2

**End of Work Package WP-011**

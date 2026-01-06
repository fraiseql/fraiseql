# Orchestrator Guide - Phase-Based Implementation

**Implementation Plan:** 8-hour documentation-focused quick wins
**Junior Engineer Role:** Documentation writer (excellent writing skills)
**Your Role:** Orchestrator (gather context, review, verify, commit)

---

## Quick Start

1. **Review phase instructions:** Read `.phases/01-operations-runbook/phase.md`
2. **Gather context files:** Copy relevant docs to `.phases/01-operations-runbook/context/`
3. **Brief junior engineer:** Point them to `phase.md` and `context/` directory
4. **Junior writes to:** `.phases/01-operations-runbook/output/`
5. **Review their work:** Check output files meet requirements
6. **Run verification:** Execute verification commands from `phase.md`
7. **Move to final location:** Copy from `output/` to final destination
8. **Commit:** Use commit message template from `phase.md`
9. **Update status:** Mark phase as ✅ Complete in `.phases/README.md`

---

## Phase Overview

| Phase | Task | Time | Output |
|-------|------|------|--------|
| 01 | Operations Runbook | 2.5h | `OPERATIONS_RUNBOOK.md` |
| 02 | Loki Configuration | 1.5h | `examples/observability/loki/` + docs |
| 03 | Dependabot Config | 0.75h | `.github/dependabot.yml` + docs |
| 04 | Incident Response | 1.5h | `COMPLIANCE/SECURITY/INCIDENT_RESPONSE.md` |
| 05 | Classified Deployment | 1.5h | `docs/deployment/CLASSIFIED_ENVIRONMENTS.md` |
| 06 | Security Validation | 1h | `scripts/validate_security_config.py` |

---

## Recommended Execution Order

### Day 1 (4 hours)
1. **Phase 03** (0.75h) - Quick win, builds confidence
2. **Phase 02** (1.5h) - Straightforward configuration
3. **Phase 01** (2h of 2.5h) - Start longest task

### Day 2 (4.75 hours)
1. **Phase 01** (0.5h) - Finish operations runbook
2. **Phase 04** (1.5h) - Incident response procedures
3. **Phase 05** (1.5h) - Classified deployment guide
4. **Phase 06** (1h) - Security validation script (bonus if time permits)

---

## Your Responsibilities (Orchestrator)

### Before Each Phase

1. **Read phase instructions:**
   ```bash
   cat .phases/0X-phase-name/phase.md
   ```

2. **Gather context files:**
   ```bash
   # Example for Phase 01
   cp docs/production/MONITORING.md .phases/01-operations-runbook/context/
   cp docs/production/ALERTING.md .phases/01-operations-runbook/context/
   cp docs/security/PROFILES.md .phases/01-operations-runbook/context/
   ```

3. **Brief junior engineer:**
   - "Read `.phases/0X-phase-name/phase.md` for requirements"
   - "Review files in `context/` directory for reference"
   - "Write your output to `output/` directory"
   - "Let me know when ready for review"

### During Phase

4. **Answer questions:** Junior may ask for clarification on requirements
5. **Check progress:** Peek at `output/` directory to see drafts
6. **Provide feedback:** If they share drafts, give constructive feedback

### After Phase Completion

7. **Review output:**
   ```bash
   # Check all required files exist
   ls -lh .phases/0X-phase-name/output/

   # Read the content
   cat .phases/0X-phase-name/output/FILE.md
   ```

8. **Run verification commands:**
   ```bash
   # Example verification from phase.md
   wc -l .phases/01-operations-runbook/output/OPERATIONS_RUNBOOK.md
   grep "^## " .phases/01-operations-runbook/output/OPERATIONS_RUNBOOK.md
   ```

9. **Move to final location:**
   ```bash
   # Example for Phase 01
   cp .phases/01-operations-runbook/output/OPERATIONS_RUNBOOK.md ./OPERATIONS_RUNBOOK.md
   ```

10. **Commit changes:**
    ```bash
    # Use commit message template from phase.md
    git add OPERATIONS_RUNBOOK.md
    git commit -m "docs(ops): add centralized operations runbook

    Consolidate operational documentation into single runbook:
    - Quick reference tables for emergency response
    - Incident response procedures for top 3 incidents
    - Deployment and troubleshooting procedures
    - Cross-references to detailed documentation

    Impact: +1 point to Observability score

    Refs: Phase 01"
    ```

11. **Update phase status:**
    ```bash
    # Edit .phases/README.md
    # Change phase status: ⬜ Not Started → ✅ Complete
    ```

---

## Context Files Guide

### Phase 01: Operations Runbook
```bash
cp docs/production/MONITORING.md .phases/01-operations-runbook/context/
cp docs/production/ALERTING.md .phases/01-operations-runbook/context/
cp COMPLIANCE/AUDIT/AUDIT_LOGGING.md .phases/01-operations-runbook/context/
cp docs/security/PROFILES.md .phases/01-operations-runbook/context/
# Any other docs/production/*.md files
```

### Phase 02: Loki Configuration
```bash
cp docs/production/MONITORING.md .phases/02-loki-configuration/context/
# Copy any existing observability examples
find examples/observability -type f -name "*.yml" -o -name "*.yaml" | \
  xargs -I {} cp {} .phases/02-loki-configuration/context/
```

### Phase 03: Dependabot Config
```bash
# Check for existing CI/CD workflows
ls .github/workflows/*.yml
# Copy existing DEPENDENCY_MANAGEMENT.md if it exists
test -f COMPLIANCE/SUPPLY_CHAIN/DEPENDENCY_MANAGEMENT.md && \
  cp COMPLIANCE/SUPPLY_CHAIN/DEPENDENCY_MANAGEMENT.md .phases/03-dependabot-config/context/
```

### Phase 04: Incident Response
```bash
cp docs/production/MONITORING.md .phases/04-incident-response/context/
cp docs/security/PROFILES.md .phases/04-incident-response/context/
cp COMPLIANCE/AUDIT/AUDIT_LOGGING.md .phases/04-incident-response/context/
# Copy Phase 01 output if complete
test -f OPERATIONS_RUNBOOK.md && \
  cp OPERATIONS_RUNBOOK.md .phases/04-incident-response/context/
```

### Phase 05: Classified Deployment
```bash
cp docs/security/PROFILES.md .phases/05-classified-deployment/context/
cp COMPLIANCE/AUDIT/AUDIT_LOGGING.md .phases/05-classified-deployment/context/
cp docs/production/MONITORING.md .phases/05-classified-deployment/context/
# Copy any existing deployment docs
find docs/deployment -type f -name "*.md" | \
  xargs -I {} cp {} .phases/05-classified-deployment/context/
```

### Phase 06: Security Validation
```bash
cp docs/security/PROFILES.md .phases/06-security-validation/context/
# Copy Phase 05 output if complete
test -f docs/deployment/CLASSIFIED_ENVIRONMENTS.md && \
  cp docs/deployment/CLASSIFIED_ENVIRONMENTS.md .phases/06-security-validation/context/
# Copy any existing scripts for pattern reference
find scripts -name "*.py" | head -3 | \
  xargs -I {} cp {} .phases/06-security-validation/context/
```

---

## Quality Checklist

Before committing, verify:

### Documentation Quality
- [ ] Content is clear and well-organized
- [ ] Commands are copy-paste ready (no placeholders like `<example>`)
- [ ] Cross-references use correct relative paths
- [ ] Tables are formatted correctly
- [ ] Code blocks have proper language tags (```bash, ```python)
- [ ] No typos or grammar errors
- [ ] Meets line count requirements (from phase.md)

### Technical Accuracy
- [ ] Configuration files have valid syntax (YAML, Python)
- [ ] File paths reference actual FraiseQL components
- [ ] Commands reference actual directories/files
- [ ] Security settings match FraiseQL capabilities
- [ ] Examples are realistic and usable

### Completeness
- [ ] All required sections present (from phase.md)
- [ ] All acceptance criteria met
- [ ] Verification commands pass
- [ ] Cross-references to other docs are correct

---

## Common Issues & Solutions

### Issue: Junior asks "What should X be?"
**Solution:** Check context files for existing patterns, or use best practices from the phase.md instructions

### Issue: Output file is too short/long
**Solution:** Review phase.md requirements - most docs should be 300-600 lines

### Issue: Commands reference non-existent files
**Solution:** Have junior check actual codebase structure in context files

### Issue: Configuration file has syntax errors
**Solution:** Run verification commands from phase.md to catch errors

### Issue: Junior is stuck on technical detail
**Solution:** Provide guidance, or mark as "good enough" and note for future improvement

---

## Time Management

- **Don't over-optimize:** First draft should be 80% good, iterate if needed
- **Time-box reviews:** Spend max 15 minutes reviewing each output
- **Focus on content:** Grammar/style can be polished later
- **Skip Phase 06 if needed:** It's marked as bonus/optional

---

## Success Metrics

After all phases complete:

✅ **All output files exist in final locations**
✅ **All verification commands pass**
✅ **All commits made with descriptive messages**
✅ **Documentation is usable by operators**
✅ **Total time ≤ 10 hours (8h junior + 2h orchestration)**

---

## Final Verification

After all phases:

```bash
# Run final verification from .phases/README.md
cd /home/lionel/code/fraiseql

# Check all files exist
test -f OPERATIONS_RUNBOOK.md && echo "✓ Runbook"
test -f examples/observability/loki/loki-config.yaml && echo "✓ Loki"
test -f .github/dependabot.yml && echo "✓ Dependabot"
test -f COMPLIANCE/SECURITY/INCIDENT_RESPONSE.md && echo "✓ Incident Response"
test -f docs/deployment/CLASSIFIED_ENVIRONMENTS.md && echo "✓ IL4/IL5 Docs"
test -f scripts/validate_security_config.py && echo "✓ Validation Script"

# Validate syntax
uv run python -c "import yaml; yaml.safe_load(open('examples/observability/loki/loki-config.yaml'))"
uv run python -c "import yaml; yaml.safe_load(open('.github/dependabot.yml'))"

# Check documentation quality
wc -l OPERATIONS_RUNBOOK.md  # Should be 300-500
wc -l COMPLIANCE/SECURITY/INCIDENT_RESPONSE.md  # Should be 400-600
wc -l docs/deployment/CLASSIFIED_ENVIRONMENTS.md  # Should be 400-600

# Test validation script
python scripts/validate_security_config.py --help
```

---

## Notes

- **Git commits:** Keep them atomic (one phase = one commit)
- **Commit messages:** Remove any references to assessment names
- **Context gathering:** Can be scripted or manual
- **Junior feedback:** Provide constructive, specific feedback
- **Time tracking:** Monitor to ensure 8-hour target is realistic

---

## Next Steps After Completion

1. Review all commits for quality
2. Update project README if needed
3. Share new documentation with team
4. Consider adding docs to onboarding materials
5. Run security validation script in CI/CD

# Phase 03: Enable GitHub Dependabot

**Priority:** HIGH
**Time Estimate:** 0.75 hours (45 minutes)
**Impact:** +1.0 point to Supply Chain Security score (22/25 → 23/25)
**Status:** ⬜ Not Started

---

## Problem Statement

Pentagon-Readiness Assessment recommends "Enable Dependabot for automated dependency updates" to improve supply chain security. This provides continuous vulnerability monitoring for dependencies and automated security patches.

---

## Objective

Configure GitHub Dependabot for:

1. Automated dependency updates (weekly schedule)
2. Security vulnerability alerts
3. Automated PR creation for updates
4. Proper grouping of patch/minor updates
5. Documentation of review workflow

**Deliverables:**

- `.github/dependabot.yml` configuration
- Documentation update to `COMPLIANCE/SUPPLY_CHAIN/DEPENDENCY_MANAGEMENT.md`

---

## Context Files

**Review these files (orchestrator will copy to `context/` if they exist):**

- `.github/workflows/*.yml` - Existing CI/CD workflows
- `pyproject.toml` or `requirements.txt` - Python dependencies
- `COMPLIANCE/SUPPLY_CHAIN/DEPENDENCY_MANAGEMENT.md` - Existing supply chain docs
- `COMPLIANCE/SUPPLY_CHAIN/SBOM.md` - SBOM documentation

**External References:**

- Dependabot configuration: https://docs.github.com/en/code-security/dependabot/dependabot-version-updates/configuration-options-for-the-dependabot.yml-file
- Dependabot grouping: https://docs.github.com/en/code-security/dependabot/dependabot-version-updates/configuration-options-for-the-dependabot.yml-file#groups

---

## Deliverables

### 1. Dependabot Configuration File

**File:** `.phases/03-dependabot-config/output/dependabot.yml`

**Target Location:** `.github/dependabot.yml`

**Requirements:**

- [ ] Python package ecosystem configured
- [ ] GitHub Actions ecosystem configured
- [ ] Weekly update schedule (Mondays, 09:00 UTC)
- [ ] Reviewers and assignees set
- [ ] Labels configured (`dependencies`, `security`)
- [ ] Commit message prefix: `chore(deps)`
- [ ] Grouped updates for patch and minor versions
- [ ] PR limit: 10 open PRs maximum

**Configuration Structure:**

```yaml
version: 2
updates:
  # Python dependencies
  - package-ecosystem: "pip"
    directory: "/"
    schedule:
      interval: "weekly"
      day: "monday"
      time: "09:00"
      timezone: "UTC"
    open-pull-requests-limit: 10
    reviewers:
      - "fraiseql/maintainers"  # Adjust based on actual team
    assignees:
      - "fraiseql/security-team"  # Adjust based on actual team
    commit-message:
      prefix: "chore(deps)"
      include: "scope"
    labels:
      - "dependencies"
      - "security"
    groups:
      patch-updates:
        patterns:
          - "*"
        update-types:
          - "patch"
      minor-updates:
        patterns:
          - "*"
        update-types:
          - "minor"

  # GitHub Actions
  - package-ecosystem: "github-actions"
    directory: "/"
    schedule:
      interval: "weekly"
      day: "monday"
    commit-message:
      prefix: "chore(ci)"
    labels:
      - "ci"
      - "dependencies"
```

**Notes:**

- Replace `fraiseql/maintainers` and `fraiseql/security-team` with actual GitHub teams or usernames
- If no teams exist, use individual maintainer usernames like `@username1`, `@username2`
- Remove Docker ecosystem if not using Dockerfiles
- Grouping prevents PR spam by combining patch/minor updates into single PRs

---

### 2. Documentation Update

**File:** `.phases/03-dependabot-config/output/DEPENDENCY_MANAGEMENT_ADDITION.md`

This will be **appended** to `COMPLIANCE/SUPPLY_CHAIN/DEPENDENCY_MANAGEMENT.md`

**Requirements:**

- [ ] Section title: "## Automated Dependency Updates"
- [ ] Dependabot configuration overview
- [ ] Update schedule documented
- [ ] PR review process explained
- [ ] Handling breaking changes guidance
- [ ] Security alert notification settings
- [ ] Query commands for status (using `gh` CLI)

**Content Structure:**

```markdown
## Automated Dependency Updates

### Dependabot Configuration

FraiseQL uses GitHub Dependabot for automated dependency updates and security vulnerability monitoring.

**Update Schedule:**
- Python dependencies: Weekly (Mondays, 09:00 UTC)
- GitHub Actions: Weekly (Mondays)
- Security updates: Immediate (as vulnerabilities are discovered)

**Configuration Location:** `.github/dependabot.yml`

### PR Review Process

When Dependabot creates a pull request:

1. **Automated Checks:**
   - CI pipeline runs full test suite
   - Security scans execute (if configured)
   - SBOM is regenerated (if applicable)

2. **Review Assignment:**
   - Maintainers are auto-assigned for review
   - Security team is notified for security updates

3. **Approval Criteria:**
   - **Patch updates:** Can be auto-merged if all tests pass (optional)
   - **Minor updates:** Manual review required, check for API changes
   - **Major updates:** Thorough review required, expect breaking changes

4. **Merge:**
   - Security updates: Merge within 24 hours
   - Patch updates: Merge within 1 week
   - Minor/major updates: Merge when tested and approved

### Handling Breaking Changes

For updates that introduce breaking changes:

1. **Review CHANGELOG:**
   - Check dependency's CHANGELOG or release notes
   - Identify breaking changes and migration steps

2. **Update Code:**
   - Fix deprecation warnings
   - Update API calls to match new interface
   - Update tests if needed

3. **Test Thoroughly:**
   - Run full test suite: `uv run pytest`
   - Run manual smoke tests
   - Check for performance regressions

4. **Update Documentation:**
   - Update internal docs if API changes
   - Add migration notes to `CHANGELOG.md`

### Security Alerts

**Notification Settings:**
- **Critical/High severity:** Immediate email/Slack notification
- **Medium severity:** Weekly digest email
- **Low severity:** Monthly digest email

**Response Time:**
- Critical: Patch within 24 hours
- High: Patch within 7 days
- Medium: Patch within 30 days
- Low: Patch in next maintenance cycle

### Query Dependabot Status

**View pending security alerts:**
```bash
gh api repos/:owner/:repo/dependabot/alerts
```

**View open Dependabot PRs:**

```bash
gh pr list --label dependencies
```

**View Dependabot configuration:**

```bash
cat .github/dependabot.yml
```

### Disabling Dependabot (Emergency)

If Dependabot creates too many PRs or causes issues:

1. **Temporarily pause updates:**
   - Edit `.github/dependabot.yml`
   - Change `open-pull-requests-limit` to `0`
   - Commit and push

2. **Close all pending PRs:**

   ```bash
   gh pr list --label dependencies --json number --jq '.[].number' | \
     xargs -I {} gh pr close {}
   ```

3. **Re-enable when ready:**
   - Restore `open-pull-requests-limit` to `10`
   - Commit and push

### Metrics

Track Dependabot effectiveness:

- Number of security vulnerabilities patched per month
- Average time to merge security updates
- Number of automated vs manual dependency updates
- Percentage of successful auto-merges (if enabled)

### References

- GitHub Dependabot Documentation: https://docs.github.com/en/code-security/dependabot
- Dependabot Configuration Options: https://docs.github.com/en/code-security/dependabot/dependabot-version-updates/configuration-options-for-the-dependabot.yml-file

```

---

## Verification (Orchestrator)

After junior engineer delivers configuration:

```bash
# 1. Validate YAML syntax
uv run python -c "import yaml; yaml.safe_load(open('.phases/03-dependabot-config/output/dependabot.yml'))"

# 2. Check required fields
grep "package-ecosystem" .phases/03-dependabot-config/output/dependabot.yml | wc -l
# Should be 2 (pip and github-actions)

# 3. Verify grouping configuration
grep -A 5 "groups:" .phases/03-dependabot-config/output/dependabot.yml

# 4. Check documentation has required sections
grep -E "^### (Dependabot Configuration|PR Review Process|Security Alerts)" .phases/03-dependabot-config/output/DEPENDENCY_MANAGEMENT_ADDITION.md

# 5. Verify gh CLI commands are included
grep "gh pr list" .phases/03-dependabot-config/output/DEPENDENCY_MANAGEMENT_ADDITION.md
```

---

## Final Placement (Orchestrator)

After verification passes:

```bash
# 1. Place Dependabot config
mkdir -p .github
cp .phases/03-dependabot-config/output/dependabot.yml .github/dependabot.yml

# 2. Append to existing documentation
if [ -f COMPLIANCE/SUPPLY_CHAIN/DEPENDENCY_MANAGEMENT.md ]; then
  echo "" >> COMPLIANCE/SUPPLY_CHAIN/DEPENDENCY_MANAGEMENT.md
  echo "---" >> COMPLIANCE/SUPPLY_CHAIN/DEPENDENCY_MANAGEMENT.md
  echo "" >> COMPLIANCE/SUPPLY_CHAIN/DEPENDENCY_MANAGEMENT.md
  cat .phases/03-dependabot-config/output/DEPENDENCY_MANAGEMENT_ADDITION.md >> COMPLIANCE/SUPPLY_CHAIN/DEPENDENCY_MANAGEMENT.md
else
  # If file doesn't exist, create it
  mkdir -p COMPLIANCE/SUPPLY_CHAIN
  cp .phases/03-dependabot-config/output/DEPENDENCY_MANAGEMENT_ADDITION.md COMPLIANCE/SUPPLY_CHAIN/DEPENDENCY_MANAGEMENT.md
fi

# 3. Commit
git add .github/dependabot.yml COMPLIANCE/SUPPLY_CHAIN/DEPENDENCY_MANAGEMENT.md
git commit -m "feat(supply-chain): enable GitHub Dependabot for dependency updates

Configure automated dependency updates and security alerts:
- Weekly Python dependency scans (Mondays 09:00 UTC)
- Weekly GitHub Actions updates
- Grouped patch and minor updates to reduce PR volume
- Auto-assignment to maintainers and security team
- Security alert notifications configured

Documentation updates:
- Added Dependabot workflow to DEPENDENCY_MANAGEMENT.md
- Documented PR review process and approval criteria
- Added security alert response times
- Included query commands for monitoring

Impact: +1 point to Supply Chain Security score (22/25 → 23/25)

Refs: Pentagon-Readiness Assessment - Phase 03"

# 4. Enable Dependabot in GitHub repo settings (manual step)
echo "⚠️  MANUAL STEP REQUIRED:"
echo "1. Go to: https://github.com/fraiseql/fraiseql/settings/security_analysis"
echo "2. Enable 'Dependabot alerts'"
echo "3. Enable 'Dependabot security updates'"
echo "4. Optionally enable 'Dependabot version updates' (auto-enabled by config file)"
```

---

## GitHub Repository Settings (Manual - Orchestrator)

After committing, enable in GitHub UI:

1. **Navigate to repository settings:**
   - Go to `https://github.com/fraiseql/fraiseql/settings/security_analysis`

2. **Enable Dependabot features:**
   - ✅ Dependabot alerts
   - ✅ Dependabot security updates
   - ✅ Grouped security updates (optional, for cleaner PRs)

3. **Configure notification preferences:**
   - Settings → Notifications → Dependabot alerts
   - Choose email/Slack/webhook notifications

4. **Verify configuration:**

   ```bash
   # Check if Dependabot is enabled (requires GitHub CLI with auth)
   gh api repos/fraiseql/fraiseql/vulnerability-alerts

   # Wait a few minutes, then check for initial Dependabot PRs
   gh pr list --label dependencies
   ```

---

## Tips for Documentation Writer

1. **Team names:** Check GitHub organization for actual team names (e.g., `@fraiseql/core`, `@fraiseql/security`)
2. **If no teams exist:** Use individual usernames like `@username1`, `@username2`
3. **Review workflow:** Think about actual PR review process - who reviews? What's the SLA?
4. **Security response times:** Be realistic - can you patch critical issues in 24 hours?
5. **Keep it practical:** Don't document processes you won't follow
6. **Update schedule:** Monday morning is good for reviews during the week

---

## Success Criteria

- [ ] File created: `.phases/03-dependabot-config/output/dependabot.yml`
- [ ] File created: `.phases/03-dependabot-config/output/DEPENDENCY_MANAGEMENT_ADDITION.md`
- [ ] YAML syntax is valid
- [ ] Configuration includes both `pip` and `github-actions` ecosystems
- [ ] Weekly schedule configured (Mondays, 09:00 UTC)
- [ ] Reviewers/assignees configured (even if placeholder)
- [ ] Grouping configured for patch and minor updates
- [ ] Documentation includes PR review process
- [ ] Documentation includes security alert response times
- [ ] Documentation includes `gh` CLI query commands

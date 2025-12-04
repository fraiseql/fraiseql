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
   - Security scans execute (Trivy scans are already configured)
   - SBOM is regenerated (if applicable)

2. **Review Assignment:**
   - As solo maintainer, all PRs are reviewed by the repository owner

3. **Approval Criteria:**
   - **Patch updates:** Can be merged if all tests pass (grouped updates reduce review burden)
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
- **Critical/High severity:** Immediate email notification
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

# FraiseQL GitHub Configuration

This directory contains GitHub-specific configuration files that define workflows, templates, and automation for the FraiseQL repository.

## 📂 Directory Structure

### 🤖 Workflows (`workflows/`)
**Purpose**: Automated CI/CD pipelines and repository automation

```
workflows/
├── test.yml                  # Test suite execution (unit, integration, system)
├── lint.yml                  # Code quality checks (ruff, black, pyright)
├── security.yml              # Security scanning and vulnerability detection
├── quality-gate.yml          # Quality gates for pull requests
├── docs.yml                  # Documentation building and deployment
└── publish.yml               # Package publishing to PyPI
```

**Workflow Triggers**:
- **test.yml**: Push to main, PRs, manual dispatch
- **lint.yml**: Push, PRs
- **security.yml**: Push to main, schedule (weekly)
- **quality-gate.yml**: PRs only
- **docs.yml**: Push to main, docs changes
- **publish.yml**: Release tags only

### 📋 Issue Templates (`ISSUE_TEMPLATE/`)
**Purpose**: Structured issue creation for consistent reporting

```
ISSUE_TEMPLATE/
├── config.yml               # Issue template configuration
├── bug_report.md           # Bug report template with debugging info
└── feature_request.md      # Feature request template with use cases
```

**Template Features**:
- **Guided information gathering**: Ensures all necessary details provided
- **Automatic labeling**: Templates auto-assign relevant labels
- **Consistent formatting**: Standardized issue structure

### 🏷️ Automation Configuration

#### `dependabot.yml`
**Purpose**: Automated dependency updates
**Features**:
- Python package updates (weekly)
- GitHub Actions updates (monthly)
- Automatic PR creation with changelogs
- Security update prioritization

#### `labeler.yml`
**Purpose**: Automatic pull request labeling based on file changes
**Categories**:
- `documentation`: Changes to docs/, README, etc.
- `tests`: Changes to tests/ directory
- `ci`: Changes to .github/workflows/
- `core`: Changes to src/fraiseql/core/
- `examples`: Changes to examples/ directory

#### `pull_request_template.md`
**Purpose**: Structured pull request descriptions
**Sections**:
- Change summary and motivation
- Testing approach and validation
- Breaking changes checklist
- Review guidance for maintainers

#### `branch-protection.md`
**Purpose**: Documentation of branch protection rules
**Contents**: Branch protection configuration for main branch

## 🔄 Workflow Details

### Test Pipeline (`test.yml`)
**Strategy**: Multi-matrix testing across Python versions and environments

```yaml
# Test matrix
Python: [3.13]
OS: [ubuntu-latest, windows-latest, macos-latest]
Database: [PostgreSQL 15, 16]
```

**Test Stages**:
1. **Environment Setup**: Python, PostgreSQL, dependencies
2. **Unit Tests**: Fast component tests
3. **Integration Tests**: Database-dependent tests
4. **System Tests**: End-to-end application tests
5. **Coverage Reporting**: Code coverage analysis

### Quality Gate (`quality-gate.yml`)
**Purpose**: Enforce quality standards on pull requests

**Quality Checks**:
- **Code Coverage**: Minimum threshold enforcement
- **Test Passing**: All tests must pass
- **Linting**: Code style compliance
- **Type Checking**: Static type analysis
- **Security**: Vulnerability scanning
- **Performance**: Regression detection

### Security Pipeline (`security.yml`)
**Security Tools**:
- **Bandit**: Python security linting
- **Safety**: Dependency vulnerability scanning
- **CodeQL**: Advanced semantic code analysis
- **Trivy**: Container and dependency scanning

**Reporting**: Security findings reported as GitHub Security Advisories

### Documentation (`docs.yml`)
**Features**:
- **Auto-build**: Documentation built on changes
- **Multi-version**: Version-specific documentation
- **GitHub Pages**: Automatic deployment
- **Link Validation**: Broken link detection

### Publishing (`publish.yml`)
**Release Process**:
1. **Trigger**: Git tag matching `v*.*.*` pattern
2. **Validation**: Full test suite execution
3. **Build**: Package building with version validation
4. **Publish**: PyPI upload with release notes
5. **Notification**: Release announcement automation

## 🛡️ Security Configuration

### Branch Protection Rules
**Main Branch Protection**:
- Require pull request reviews (2 reviewers)
- Dismiss stale reviews when new commits pushed
- Require status checks to pass (all CI workflows)
- Restrict who can push (maintainers only)
- Require linear history (no merge commits)

### Dependency Security
- **Dependabot**: Automated security updates
- **Vulnerability Alerts**: Email notifications for security issues
- **Private Vulnerability Reporting**: Secure disclosure process

## 👥 Community Integration

### Issue Management
**Automatic Triaging**:
- Bug reports → `bug` label + priority assessment
- Feature requests → `enhancement` label + needs-triage
- Security issues → `security` label + private handling

### Pull Request Workflow
**Review Process**:
1. **Automated Checks**: CI pipeline validation
2. **Manual Review**: Code review by maintainers
3. **Testing**: Feature validation in review app
4. **Merge**: Squash and merge with linear history

### Release Management
**Release Process**:
- **Semantic Versioning**: MAJOR.MINOR.PATCH versioning
- **Automated Changelog**: Generated from commit messages
- **Release Notes**: Auto-generated with PR references
- **Asset Publishing**: Packages published to PyPI

## 🔧 Configuration Management

### Updating Workflows
**Best Practices**:
1. **Test in fork**: Validate workflow changes in personal fork
2. **Incremental updates**: Small, focused changes
3. **Version pinning**: Pin action versions for reproducibility
4. **Documentation**: Update this README when adding workflows

### Template Maintenance
**Regular Tasks**:
- **Review templates**: Ensure they gather needed information
- **Update examples**: Keep examples current with project state
- **User feedback**: Incorporate user suggestions for improvements

### Security Updates
**Security Practices**:
- **Action versions**: Keep GitHub Actions updated
- **Permissions**: Use minimal required permissions
- **Secrets**: Rotate secrets regularly
- **Scanning**: Regular security scanning of workflows

## 📊 Monitoring and Metrics

### Workflow Success Rates
**Key Metrics**:
- Test success rate across platforms
- Average CI/CD pipeline duration
- Security scan findings trends
- Deployment success rate

### Community Engagement
**Tracked Metrics**:
- Issue response time
- PR review turnaround
- Community contribution rate
- Documentation usage analytics

## 🚨 Troubleshooting

### Common Workflow Issues
| Issue | Cause | Solution |
|-------|-------|----------|
| Test failures | Flaky tests or environment issues | Review logs, fix tests |
| Lint errors | Code style violations | Run `make lint` locally |
| Security alerts | Vulnerable dependencies | Update dependencies |
| Build failures | Dependency conflicts | Review dependency changes |

### Getting Help
- **Workflow logs**: Check GitHub Actions tab for detailed logs
- **Issue templates**: Use appropriate templates for reporting problems
- **Maintainer contact**: Tag @lionel-hamayon for workflow issues

---

## 🎯 Quick Reference

**Creating issues?** → Use `.github/ISSUE_TEMPLATE/`
**Submitting PRs?** → Follow `.github/pull_request_template.md`
**Workflow failing?** → Check workflow logs in Actions tab
**Security concern?** → Use private vulnerability reporting

---

*This GitHub configuration evolves with FraiseQL development needs. Workflow changes should be tested and documented.*

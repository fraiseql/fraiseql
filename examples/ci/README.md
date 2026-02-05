# FraiseQL Design Quality CI/CD Integration Examples

Ready-to-use workflow files for integrating design quality checks into your CI/CD pipeline.

## Files

### GitHub Actions
- **`github-actions-lint.yml`** - Complete GitHub Actions workflow
  - Runs on PR, push to main/develop, and manual trigger
  - Waits for fraiseql-server to start
  - Generates HTML report as artifact
  - Posts PR comments with results
  - Sends Slack notifications on failure
  - Creates GitHub status checks

**Installation:**
```bash
cp github-actions-lint.yml .github/workflows/design-quality.yml
```

**Required Secrets:**
- `SLACK_WEBHOOK_URL` (optional, for notifications)

**Usage:**
- Automatically runs on schema changes
- Can trigger manually with custom threshold
- Use status checks to block merges

### GitLab CI
- **`gitlab-ci-lint.yml`** - GitLab CI configuration
  - Runs on MR and push to protected branches
  - Services mode for fraiseql-server
  - Artifacts with 30-day retention
  - Slack notifications (success and failure)
  - Configurable threshold via variables
  - Automatic retries on transient failures

**Installation:**
```bash
# Add to your existing .gitlab-ci.yml
cat gitlab-ci-lint.yml >> .gitlab-ci.yml

# Or copy and customize
cp gitlab-ci-lint.yml .gitlab-ci.yml
```

**Required Variables:**
- `SLACK_WEBHOOK_URL` (optional, for notifications)

**Usage:**
```yaml
# Override threshold in CI settings or .gitlab-ci.yml
variables:
  DESIGN_QUALITY_THRESHOLD: "80"
```

### Pre-Commit Hooks
- **`../pre-commit-hooks.sh`** - Local git pre-commit hook
  - Runs before every commit
  - Checks if schema changed
  - Fails commit if design score is below threshold
  - Can be skipped with `--no-verify`

**Installation:**
```bash
cp examples/pre-commit-hooks.sh .git/hooks/pre-commit
chmod +x .git/hooks/pre-commit
```

**Configuration:**
```bash
# Environment variables
export SCHEMA_FILE="schema.compiled.json"
export FRAISEQL_API_ENDPOINT="http://localhost:8080"
export DESIGN_QUALITY_THRESHOLD="70"
export PYTHON_CMD="python3"
```

## Quick Start

### 1. Start fraiseql-server

```bash
# Option A: Compile and run
cargo build --release -p fraiseql-server
./target/release/fraiseql-server

# Option B: Docker
docker run -p 8080:8080 fraiseql/fraiseql-server

# Option C: Development
cargo run -p fraiseql-server
```

Server will be available at `http://localhost:8080`

### 2. Set up CI/CD Integration

**GitHub Actions:**
```bash
mkdir -p .github/workflows
cp examples/ci/github-actions-lint.yml .github/workflows/design-quality.yml
git add .github/workflows/design-quality.yml
```

**GitLab CI:**
```bash
# Append to existing .gitlab-ci.yml
cat examples/ci/gitlab-ci-lint.yml >> .gitlab-ci.yml
git add .gitlab-ci.yml
```

**Pre-Commit (Local):**
```bash
cp examples/pre-commit-hooks.sh .git/hooks/pre-commit
chmod +x .git/hooks/pre-commit
```

### 3. (Optional) Set up Slack Notifications

**GitHub:**
1. Go to Slack App settings
2. Create incoming webhook
3. Add to GitHub repo secrets: `SLACK_WEBHOOK_URL`

**GitLab:**
1. Go to Slack App settings
2. Create incoming webhook
3. Add to GitLab CI/CD variables: `SLACK_WEBHOOK_URL`

### 4. Test

```bash
# Run locally first
python examples/agents/python/schema_auditor.py schema.compiled.json

# Commit to test CI/CD
git commit --allow-empty -m "test: trigger design quality check"
```

## Configuration

### Threshold Levels

- **80-100 (Excellent)**: No issues, optimal federation design
- **70-79 (Good)**: Minor issues, should be addressed
- **60-69 (Fair)**: Several issues, should be fixed
- **Below 60 (Poor)**: Critical issues, must be fixed

### Recommended Thresholds by Context

```
Main branch:        fail_if_below = 85  (strict)
Feature branches:   fail_if_below = 75  (standard)
Team PRs:           fail_if_below = 70  (reasonable)
External PRs:       fail_if_below = 60  (lenient)
```

### Gradual Rollout

Implement in stages:

```
Week 1: Report only (fail_if_below = 0)
Week 2: Warn at medium (fail_if_below = 50)
Week 3: Enforce at standard (fail_if_below = 70)
Week 4: Enforce at excellent (fail_if_below = 80)
```

## Customization

### Changing the Schema File Path

GitHub Actions:
```yaml
- name: Run design audit
  run: |
    python examples/agents/python/schema_auditor.py \
      path/to/your/schema.compiled.json \
      --api-endpoint http://localhost:8080
```

GitLab CI:
```yaml
variables:
  SCHEMA_FILE: "path/to/schema.compiled.json"
```

Pre-commit:
```bash
export SCHEMA_FILE="path/to/schema.compiled.json"
```

### Changing the API Endpoint

For remote fraiseql-server instance:

GitHub Actions:
```yaml
- name: Run design audit
  run: |
    python examples/agents/python/schema_auditor.py \
      schema.compiled.json \
      --api-endpoint https://api.example.com:8080
```

GitLab CI:
```yaml
before_script:
  - export FRAISEQL_API_ENDPOINT="https://api.example.com:8080"
```

Pre-commit:
```bash
export FRAISEQL_API_ENDPOINT="https://api.example.com:8080"
```

## Troubleshooting

### "Server not running"

```bash
# Check if server is running
curl http://localhost:8080/health

# Start server
fraiseql-server

# Or use Docker
docker run -p 8080:8080 fraiseql/fraiseql-server
```

### "Schema file not found"

```bash
# Compile schema first
fraiseql-cli compile schema.json -o schema.compiled.json

# Then run audit
python examples/agents/python/schema_auditor.py schema.compiled.json
```

### "Python not found" (pre-commit)

```bash
# Install Python
# macOS:
brew install python@3.11

# Linux (Ubuntu):
sudo apt-get install python3

# Then set in hook:
export PYTHON_CMD="python3.11"
```

### "API error" in CI/CD

1. Check server is running in CI environment
2. Verify API endpoint is correct
3. Check server logs: `docker logs <container-id>`
4. Verify firewall allows traffic on port 8080

## Examples

### Simple Setup (GitHub Actions)

```yaml
# .github/workflows/design-quality.yml
name: Design Quality

on:
  pull_request:
    paths: ['schema.compiled.json']

jobs:
  check:
    runs-on: ubuntu-latest
    services:
      server:
        image: fraiseql/fraiseql-server:latest
        options: --health-cmd "curl -f http://localhost:8080/health"
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-python@v4
        with:
          python-version: '3.11'
      - run: pip install -r examples/agents/python/requirements.txt
      - run: |
          python examples/agents/python/schema_auditor.py \
            schema.compiled.json \
            --fail-if-below 70
```

### Advanced Setup (GitLab CI)

```yaml
# .gitlab-ci.yml
design_quality:
  stage: test
  image: python:3.11
  services:
    - fraiseql/fraiseql-server:latest
  script:
    - pip install -r examples/agents/python/requirements.txt
    - python examples/agents/python/schema_auditor.py schema.compiled.json --fail-if-below 75
  artifacts:
    paths: [design-audit-report.html]
    expire_in: 30 days
  only:
    - merge_requests
    - main
```

### Local Pre-Commit

```bash
# Install
cp examples/pre-commit-hooks.sh .git/hooks/pre-commit
chmod +x .git/hooks/pre-commit

# Usage
git add schema.compiled.json
git commit -m "Update schema"  # Runs audit automatically

# Override if needed
git commit --no-verify -m "Update schema"  # Skip audit
```

## Best Practices

1. **Start Lenient**: Begin with low threshold, gradually increase
2. **Notify Team**: Use Slack/Discord to keep team informed
3. **Track Trends**: Store reports to visualize improvements
4. **Fix Quickly**: Address violations immediately
5. **Document**: Link to DESIGNING_FOR_FRAISEQL.md in PR comments

## References

- [DESIGNING_FOR_FRAISEQL.md](../../docs/DESIGNING_FOR_FRAISEQL.md) - Design patterns guide
- [LINTING_RULES.md](../../docs/LINTING_RULES.md) - Rule reference
- [CI_CD_INTEGRATION.md](../../docs/CI_CD_INTEGRATION.md) - Integration guide

## Support

Having issues? Check:

1. [CI_CD_INTEGRATION.md](../../docs/CI_CD_INTEGRATION.md) Troubleshooting section
2. Server logs: `docker logs <container-id>` or look at fraiseql-server output
3. Python script help: `python examples/agents/python/schema_auditor.py --help`

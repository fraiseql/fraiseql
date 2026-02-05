<!-- Skip to main content -->
---
title: CI/CD Integration Guide
description: This guide shows how to enforce design quality standards in continuous integration pipelines.
keywords: []
tags: ["documentation", "reference"]
---

# CI/CD Integration Guide

## Integrate FraiseQL design quality checks into your development workflow

This guide shows how to enforce design quality standards in continuous integration pipelines.

---

## Table of Contents

- [Quick Start](#quick-start)
- [GitHub Actions](#github-actions)
- [GitLab CI](#gitlab-ci)
- [CircleCI](#circleci)
- [Local Pre-commit Hooks](#local-pre-commit-hooks)
- [Slack Notifications](#slack-notifications)
- [Failing Builds](#failing-builds)
- [Design Quality Reports](#design-quality-reports)

---

## Quick Start

### 1. Install Agents

**Python Schema Auditor**:

```bash
<!-- Code example in BASH -->
pip install -r examples/agents/python/requirements.txt
```text
<!-- Code example in TEXT -->

**TypeScript Federation Analyzer**:

```bash
<!-- Code example in BASH -->
cd examples/agents/typescript
npm install
npm run build
```text
<!-- Code example in TEXT -->

### 2. Start FraiseQL-server

```bash
<!-- Code example in BASH -->
cargo build --release -p FraiseQL-server
./target/release/FraiseQL-server
```text
<!-- Code example in TEXT -->

Server runs on `http://localhost:8080` by default.

### 3. Run Analysis

**Python**:

```bash
<!-- Code example in BASH -->
python examples/agents/python/schema_auditor.py schema.compiled.json
```text
<!-- Code example in TEXT -->

**TypeScript**:

```bash
<!-- Code example in BASH -->
npx federation-analyzer --schema schema.compiled.json
```text
<!-- Code example in TEXT -->

---

## GitHub Actions

### Setup

1. Add workflow file:

```yaml
<!-- Code example in YAML -->
# .github/workflows/design-quality.yml
name: Design Quality Check

on:
  pull_request:
    paths:
      - 'schema/**'
      - 'schema.compiled.json'
      - '*.toml'
  workflow_dispatch:

jobs:
  design-quality:
    runs-on: ubuntu-latest
    services:
      FraiseQL-server:
        image: FraiseQL/FraiseQL-server:latest
        ports:
          - 8080:8080
        env:
          DATABASE_URL: sqlite::memory:
          FRAISEQL_SCHEMA_PATH: schema.compiled.json
    steps:
      - uses: actions/checkout@v4

      - name: Set up Python
        uses: actions/setup-python@v4
        with:
          python-version: '3.11'

      - name: Install Python dependencies
        run: |
          pip install -r examples/agents/python/requirements.txt

      - name: Wait for server
        run: |
          for i in {1..30}; do
            if curl -f http://localhost:8080/health; then
              echo "Server ready"
              exit 0
            fi
            echo "Waiting for server... ($i/30)"
            sleep 1
          done
          echo "Server failed to start"
          exit 1

      - name: Run design audit
        run: |
          python examples/agents/python/schema_auditor.py \
            schema.compiled.json \
            --api-endpoint http://localhost:8080 \
            --output design-audit-report.html

      - name: Check design score
        run: |
          python examples/agents/python/schema_auditor.py \
            schema.compiled.json \
            --api-endpoint http://localhost:8080 \
            --fail-if-below 70
        continue-on-error: true

      - name: Upload audit report
        if: always()
        uses: actions/upload-artifact@v3
        with:
          name: design-audit-report
          path: design-audit-report.html

      - name: Comment on PR
        if: github.event_name == 'pull_request'
        uses: actions/github-script@v7
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}
          script: |
            const fs = require('fs');
            const report = fs.readFileSync('design-audit-report.html', 'utf8');
            github.rest.issues.createComment({
              issue_number: context.issue.number,
              owner: context.repo.owner,
              repo: context.repo.repo,
              body: '## 📊 Design Quality Audit\nSee artifacts for detailed HTML report'
            });
```text
<!-- Code example in TEXT -->

### Features

- ✅ Runs on schema changes
- ✅ Waits for server startup
- ✅ Generates HTML report
- ✅ Comments on PR
- ✅ Fails build if score < 70
- ✅ Uploads report as artifact

---

## GitLab CI

### Setup

1. Add to `.gitlab-ci.yml`:

```yaml
<!-- Code example in YAML -->
design-quality:
  stage: test
  image: python:3.11
  services:
    - name: FraiseQL/FraiseQL-server:latest
      alias: FraiseQL-server
      variables:
        DATABASE_URL: sqlite::memory:
        FRAISEQL_SCHEMA_PATH: schema.compiled.json
  before_script:
    - pip install -r examples/agents/python/requirements.txt
    - |
      for i in {1..30}; do
        if python -c "import requests; requests.get('http://FraiseQL-server:8080/health')" 2>/dev/null; then
          echo "Server ready"
          break
        fi
        echo "Waiting for server... ($i/30)"
        sleep 1
      done
  script:
    - |
      python examples/agents/python/schema_auditor.py \
        schema.compiled.json \
        --api-endpoint http://FraiseQL-server:8080 \
        --output design-audit-report.html
    - |
      python examples/agents/python/schema_auditor.py \
        schema.compiled.json \
        --api-endpoint http://FraiseQL-server:8080 \
        --fail-if-below 70
  artifacts:
    paths:
      - design-audit-report.html
    reports:
      # Could generate JUnit XML for GitLab
      junit: design-audit-results.xml
    expire_in: 30 days
  allow_failure: false
  only:
    - merge_requests
    - main
```text
<!-- Code example in TEXT -->

### Features

- ✅ Services (containerized server)
- ✅ Automatic retry logic
- ✅ Artifacts with expiration
- ✅ Only runs on MR/main
- ✅ Blocks merge if score low

---

## CircleCI

### Setup

1. Add to `.circleci/config.yml`:

```yaml
<!-- Code example in YAML -->
version: 2.1

jobs:
  design-quality:
    docker:
      - image: cimg/python:3.11
      - image: FraiseQL/FraiseQL-server:latest
        environment:
          DATABASE_URL: sqlite::memory:
          FRAISEQL_SCHEMA_PATH: schema.compiled.json
    steps:
      - checkout

      - run:
          name: Install Python dependencies
          command: pip install -r examples/agents/python/requirements.txt

      - run:
          name: Wait for FraiseQL-server
          command: |
            for i in {1..30}; do
              if curl -f http://localhost:8080/health; then
                echo "Server ready"
                exit 0
              fi
              echo "Waiting... ($i/30)"
              sleep 1
            done
            echo "Server failed to start"
            exit 1

      - run:
          name: Run design audit
          command: |
            python examples/agents/python/schema_auditor.py \
              schema.compiled.json \
              --api-endpoint http://localhost:8080 \
              --output design-audit-report.html \
              --format html

      - run:
          name: Check design score
          command: |
            python examples/agents/python/schema_auditor.py \
              schema.compiled.json \
              --api-endpoint http://localhost:8080 \
              --fail-if-below 70

      - store_artifacts:
          path: design-audit-report.html
          destination: design-audit/report.html

workflows:
  test:
    jobs:
      - design-quality:
          filters:
            branches:
              only:
                - main
                - /^feature\/.*/
```text
<!-- Code example in TEXT -->

### Features

- ✅ Docker services
- ✅ Artifact storage
- ✅ Branch filtering
- ✅ Workflow orchestration

---

## Local Pre-commit Hooks

### Setup

1. Create `.git/hooks/pre-commit`:

```bash
<!-- Code example in BASH -->
#!/bin/bash
# Hook to run design audit before committing

set -e

SCHEMA_FILE="schema.compiled.json"
API_ENDPOINT="${FRAISEQL_API_ENDPOINT:-http://localhost:8080}"
THRESHOLD="${DESIGN_QUALITY_THRESHOLD:-70}"

# Check if schema file changed
if ! git diff --cached --name-only | grep -q "$SCHEMA_FILE"; then
  exit 0
fi

echo "Running design quality check..."

# Check if server is running
if ! curl -f "$API_ENDPOINT/health" > /dev/null 2>&1; then
  echo "Error: FraiseQL-server not running at $API_ENDPOINT"
  echo "Start server with: FraiseQL-server"
  exit 1
fi

# Run audit
python examples/agents/python/schema_auditor.py \
  "$SCHEMA_FILE" \
  --api-endpoint "$API_ENDPOINT" \
  --fail-if-below "$THRESHOLD" \
  --quiet

if [ $? -eq 0 ]; then
  echo "✅ Design quality check passed"
  exit 0
else
  echo "❌ Design quality check failed"
  echo "Fix issues or override with: git commit --no-verify"
  exit 1
fi
```text
<!-- Code example in TEXT -->

1. Make executable:

```bash
<!-- Code example in BASH -->
chmod +x .git/hooks/pre-commit
```text
<!-- Code example in TEXT -->

1. Install globally (optional):

```bash
<!-- Code example in BASH -->
mkdir -p ~/.githooks
cp .git/hooks/pre-commit ~/.githooks/pre-commit
chmod +x ~/.githooks/pre-commit
git config --global core.hooksPath ~/.githooks
```text
<!-- Code example in TEXT -->

### Features

- ✅ Runs before commit
- ✅ Checks schema changes only
- ✅ Can override with `--no-verify`
- ✅ Configurable threshold
- ✅ Uses environment variables

---

## Slack Notifications

### GitHub Actions

Add to workflow:

```yaml
<!-- Code example in YAML -->
- name: Notify Slack
  if: failure()
  uses: slackapi/slack-github-action@v1.24.0
  with:
    payload: |
      {
        "text": "Design quality check failed",
        "blocks": [
          {
            "type": "section",
            "text": {
              "type": "mrkdwn",
              "text": "*Design Quality Alert*\n*Repository:* ${{ github.repository }}\n*Branch:* ${{ github.ref_name }}\n*PR:* #${{ github.event.pull_request.number }}"
            }
          },
          {
            "type": "actions",
            "elements": [
              {
                "type": "button",
                "text": {
                  "type": "plain_text",
                  "text": "View Report"
                },
                "url": "${{ github.server_url }}/${{ github.repository }}/actions/runs/${{ github.run_id }}"
              }
            ]
          }
        ]
      }
  env:
    SLACK_WEBHOOK_URL: ${{ secrets.SLACK_WEBHOOK_URL }}
    SLACK_WEBHOOK_TYPE: INCOMING_WEBHOOK
```text
<!-- Code example in TEXT -->

### GitLab CI

Add to pipeline:

```yaml
<!-- Code example in YAML -->
slack-notify:
  stage: .post
  image: curlimages/curl:latest
  script:
    - |
      curl -X POST $SLACK_WEBHOOK_URL \
        -H 'Content-type: application/json' \
        -d '{
          "text": "Design quality check failed",
          "attachments": [{
            "color": "danger",
            "fields": [
              {
                "title": "Repository",
                "value": "'$CI_PROJECT_PATH'",
                "short": true
              },
              {
                "title": "Branch",
                "value": "'$CI_COMMIT_BRANCH'",
                "short": true
              },
              {
                "title": "Details",
                "value": "'$CI_PIPELINE_URL'",
                "short": false
              }
            ]
          }]
        }'
  when: on_failure
  only:
    - main
    - merge_requests
```text
<!-- Code example in TEXT -->

---

## Failing Builds

### Threshold Strategy

```python
<!-- Code example in Python -->
# Different thresholds for different contexts
if branch == "main":
    threshold = 90  # Strict for main
elif pr.is_from_team_member:
    threshold = 75  # Reasonable for team
else:
    threshold = 60  # Lenient for contributors
```text
<!-- Code example in TEXT -->

### Exit Codes

```text
<!-- Code example in TEXT -->
0: Score >= threshold (pass)
1: Score < threshold (fail)
2: API error (fail, but different)
3: Configuration error
```text
<!-- Code example in TEXT -->

### Gradual Enforcement

```bash
<!-- Code example in BASH -->
# Week 1: Report only (no blocking)
python schema_auditor.py --fail-if-below 0

# Week 2: Warn at 50
python schema_auditor.py --fail-if-below 50

# Week 3: Enforce at 70
python schema_auditor.py --fail-if-below 70

# Week 4: Enforce at 80 (production standard)
python schema_auditor.py --fail-if-below 80
```text
<!-- Code example in TEXT -->

---

## Design Quality Reports

### Historical Tracking

Store reports for trend analysis:

```bash
<!-- Code example in BASH -->
# Save report with timestamp
TIMESTAMP=$(date +%Y%m%d-%H%M%S)
python schema_auditor.py \
  schema.compiled.json \
  --output "reports/design-audit-$TIMESTAMP.json" \
  --format json
```text
<!-- Code example in TEXT -->

### Dashboard Example

```html
<!-- Code example in HTML -->
<!-- Simple HTML dashboard -->
<html>
<head>
  <title>Design Quality Trend</title>
  <script src="https://cdn.jsdelivr.net/npm/chart.js"></script>
</head>
<body>
  <h1>Design Quality Trend</h1>
  <canvas id="trendChart"></canvas>

  <script>
    const reports = [
      { date: '2024-01-01', score: 65 },
      { date: '2024-01-02', score: 68 },
      { date: '2024-01-03', score: 72 },
      { date: '2024-01-04', score: 75 },
    ];

    new Chart(document.getElementById('trendChart'), {
      type: 'line',
      data: {
        labels: reports.map(r => r.date),
        datasets: [{
          label: 'Design Score',
          data: reports.map(r => r.score),
          borderColor: '#667eea',
          fill: false,
        }],
      },
      options: {
        scales: {
          y: { min: 0, max: 100 }
        }
      }
    });
  </script>
</body>
</html>
```text
<!-- Code example in TEXT -->

---

## Best Practices

### 1. **Progressive Enforcement**

Start lenient, gradually increase standards:

```yaml
<!-- Code example in YAML -->
# Month 1: Informational only
fail_threshold: 0

# Month 2: Warn at medium
fail_threshold: 60

# Month 3: Enforce at good
fail_threshold: 75

# Month 4+: Enforce at excellent
fail_threshold: 85
```text
<!-- Code example in TEXT -->

### 2. **Team Communication**

Send results to Slack/Discord:

```bash
<!-- Code example in BASH -->
# Extract score from JSON report
SCORE=$(jq '.data.overall_score' report.json)
echo "Design Quality Score: $SCORE/100" > /dev/slack
```text
<!-- Code example in TEXT -->

### 3. **Exemptions**

Allow explicit overrides for well-justified exceptions:

```bash
<!-- Code example in BASH -->
# Skip if approved by architect
if git log -1 --format=%B | grep -q "DESIGN-OVERRIDE: APPROVED"; then
  exit 0
fi
```text
<!-- Code example in TEXT -->

### 4. **Benchmarking**

Track improvement over time:

```bash
<!-- Code example in BASH -->
# Compare to previous version
PREV_SCORE=$(git show HEAD:design-score.txt)
CURR_SCORE=$(python schema_auditor.py --json | jq '.data.overall_score')
echo "Score: $PREV_SCORE → $CURR_SCORE"
```text
<!-- Code example in TEXT -->

---

## Troubleshooting

### Server Not Found

```bash
<!-- Code example in BASH -->
# Check server is running
curl http://localhost:8080/health

# Start server
cargo run -p FraiseQL-server

# Or use Docker
docker run -p 8080:8080 FraiseQL/FraiseQL-server
```text
<!-- Code example in TEXT -->

### Schema Compilation Failure

```bash
<!-- Code example in BASH -->
# Compile schema first
FraiseQL-cli compile schema.json -o schema.compiled.json

# Then run audit
python schema_auditor.py schema.compiled.json
```text
<!-- Code example in TEXT -->

### API Errors

```bash
<!-- Code example in BASH -->
# Check API endpoint
curl http://api.example.com/api/v1/design/audit

# Enable verbose logging
RUST_LOG=debug FraiseQL-server
```text
<!-- Code example in TEXT -->

---

## Next Steps

- Run on all PRs with `fail-if-below 75`
- Set up Slack notifications for failures
- Create dashboard for historical tracking
- Train team on design patterns
- Review and fix violations

See [LINTING_RULES.md](./LINTING_RULES.md) for how to fix specific violations.

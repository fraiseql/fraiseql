# FraiseQL Design Quality Guide

## Introduction

FraiseQL Design Quality is an automated linting and auditing system for GraphQL schema architecture. It helps teams design schemas that work optimally with FraiseQL's JSONB batching and compilation model.

Think of it as "Clippy for GraphQL" - it enforces best practices specific to FraiseQL compilation.

## Quick Start

### CLI Usage

```bash
# Analyze your schema
fraiseql lint schema.json

# Output:
# {
#   "overall_score": 85,
#   "severity_counts": { "critical": 0, "warning": 2, "info": 3 },
#   "categories": {
#     "federation": 80,
#     "cost": 90,
#     "cache": 85,
#     "authorization": 90,
#     "compilation": 80
#   }
# }

# Filter to specific categories
fraiseql lint schema.json --federation --cost

# CI/CD integration
fraiseql lint schema.json --fail-on-critical --fail-on-warning

# Get detailed analysis
fraiseql lint schema.json --verbose --json | jq '.data'
```

### API Usage

```bash
# POST design audit request
curl -X POST http://localhost:8080/api/v1/design/audit \
  -H "Content-Type: application/json" \
  -d '{
    "schema": {
      "types": [{
        "name": "User",
        "fields": [{"name": "id", "type": "ID", "isPrimaryKey": true}]
      }]
    }
  }'

# Response:
{
  "status": "success",
  "data": {
    "overall_score": 92,
    "severity_counts": {"critical": 0, "warning": 0, "info": 1},
    "federation": {"score": 100, "issues": []},
    "cost": {"score": 100, "issues": []},
    "cache": {"score": 95, "issues": [...]},
    "authorization": {"score": 100, "issues": []},
    "compilation": {"score": 90, "issues": [...]}
  }
}
```

## Understanding Design Scores

Design scores range from 0-100:

- **90-100**: Excellent design - follows FraiseQL best practices
- **70-89**: Good design - minor optimizations recommended
- **50-69**: Acceptable design - some issues should be addressed
- **0-49**: Poor design - significant improvements needed

### Category Scores

Each category is scored separately:

#### Federation (JSONB Batching)

- Detects over-federation (entities in 3+ subgraphs)
- Identifies circular dependencies
- Flags fragmented entity resolution

### Cost (Compilation Determinism)

- Analyzes worst-case query complexity
- Detects unbounded pagination
- Identifies multiplier patterns (lists in lists)

### Cache (JSONB Coherency)

- Checks TTL consistency across subgraphs
- Validates cache directives on expensive fields
- Verifies coherency strategies

### Authorization (Security Boundaries)

- Detects auth boundary leaks
- Checks for unprotected mutations
- Verifies scope consistency

### Compilation (Type Suitability)

- Validates schema structure for compilation
- Checks for circular type references
- Verifies required metadata

## Design Rules Reference

### Federation Rules

#### Rule: Over-Federation Detection

**Severity**: Warning (3+ subgraphs), Critical (5+)
**Description**: Entity exists in too many subgraphs
**Problem**: Can't batch efficiently with JSONB views across multiple subgraphs

```json
// ❌ Anti-pattern: User in 3 subgraphs
{
  "subgraphs": [
    {"name": "users", "entities": ["User"]},
    {"name": "posts", "entities": ["User", "Post"]},
    {"name": "comments", "entities": ["User", "Comment"]}
  ]
}

// ✅ Fix: User in one subgraph, referenced elsewhere
{
  "subgraphs": [
    {"name": "users", "entities": ["User"]},
    {"name": "posts", "entities": ["Post"],
     "references": [{"type": "User", "via": "users"}]},
    {"name": "comments", "entities": ["Comment"],
     "references": [{"type": "User", "via": "users"}]}
  ]
}
```

#### Rule: Circular Dependency Detection

**Severity**: Warning
**Description**: Circular reference chain (A → B → A)
**Problem**: Nested JSONB becomes inefficient, hard to compile

```json
// ❌ Anti-pattern: Circular reference
// users-service: User references Post
// posts-service: Post references User

// ✅ Fix: Break cycle at reference boundary
{
  "subgraphs": [
    {"name": "users", "entities": ["User"],
     "references": [{"type": "Post", "via": "posts"}]},
    {"name": "posts", "entities": ["Post"]  // No back-reference
    }
  ]
}
```

### Cost Rules

#### Rule: Worst-Case Complexity

**Severity**: Critical (>5000), Warning (>1000)
**Description**: Query can hit extreme complexity
**Problem**: Unexpected production issues under load

```json
// ❌ Anti-pattern: O(n²) query pattern
{
  "types": [
    {"name": "User", "fields": [{"name": "posts", "type": "[Post!]"}]},
    {"name": "Post", "fields": [{"name": "comments", "type": "[Comment!]"}]},
    {"name": "Comment", "fields": [{"name": "replies", "type": "[Comment!]"}]}
  ]
}

// ✅ Fix: Add pagination limits
{
  "types": [
    {"name": "User", "fields": [
      {"name": "posts", "type": "[Post!]", "limit": 10}
    ]},
    {"name": "Post", "fields": [
      {"name": "comments", "type": "[Comment!]", "limit": 10}
    ]}
  ]
}
```

### Cache Rules

#### Rule: TTL Consistency

**Severity**: Warning
**Description**: Same entity with different TTLs across subgraphs
**Problem**: Stale data in some subgraphs, fresh in others

```json
// ❌ Anti-pattern: Inconsistent TTL
{
  "subgraphs": [
    {"name": "users", "entities": ["User"], "cache_ttl_seconds": 300},
    {"name": "posts", "entities": [],
     "references": [{"type": "User", "cache_ttl_seconds": 1800}]}
  ]
}

// ✅ Fix: Consistent TTL
{
  "subgraphs": [
    {"name": "users", "entities": ["User"], "cache_ttl_seconds": 300},
    {"name": "posts", "entities": [],
     "references": [{"type": "User", "cache_ttl_seconds": 300}]}
  ]
}
```

### Authorization Rules

#### Rule: Auth Boundary Leak

**Severity**: Critical
**Description**: Protected field accessible without auth check
**Problem**: Security violation, data exposure

```json
// ❌ Anti-pattern: Protected field exposed
{
  "types": [{
    "name": "User",
    "fields": [{
      "name": "email",
      "requires_auth": true
    }]
  }],
  "subgraphs": [
    {"name": "users", "entities": ["User"]},
    {"name": "analytics",
     "references": [{"type": "User", "accessible_fields": ["id", "email"]}]}
  ]
}

// ✅ Fix: Auth check enforced
{
  "subgraphs": [
    {"name": "users", "entities": ["User"]},
    {"name": "analytics",
     "references": [
       {"type": "User", "accessible_fields": ["id", "name"]}
     ]}
  ]
}
```

## Real-World Examples

### Example 1: User/Posts/Comments Schema

**Initial Design**:

```json
{
  "subgraphs": [
    {"name": "users", "entities": ["User"]},
    {"name": "posts", "entities": ["User", "Post"]},  // ⚠️ User duplicate
    {"name": "comments", "entities": ["User", "Comment"]}  // ⚠️ User duplicate
  ]
}
```

**Issues Found**:

- User in 3 subgraphs (Warning) - Score -15
- Potential circular references - Score -10
- No TTL specified - Score -5

**Score: 70** (Acceptable, but needs work)

**Improved Design**:

```json
{
  "subgraphs": [
    {"name": "users", "entities": ["User"], "cache_ttl_seconds": 300},
    {"name": "posts", "entities": ["Post"],
     "references": [{"type": "User", "cache_ttl_seconds": 300}]},
    {"name": "comments", "entities": ["Comment"],
     "references": [{"type": "User"}, {"type": "Post"}]}
  ],
  "types": [
    {"name": "User", "fields": [
      {"name": "id", "type": "ID", "isPrimaryKey": true},
      {"name": "posts", "type": "[Post!]", "limit": 10}
    ]},
    {"name": "Post", "fields": [
      {"name": "id", "type": "ID", "isPrimaryKey": true},
      {"name": "comments", "type": "[Comment!]", "limit": 10}
    ]},
    {"name": "Comment", "fields": [
      {"name": "id", "type": "ID", "isPrimaryKey": true}
    ]}
  ]
}
```

**Result: Score 92** (Excellent)

## CI/CD Integration

### GitHub Actions

```yaml
# .github/workflows/design-quality.yml
name: Design Quality Gate

on:
  pull_request:
    paths:
      - 'schema/*.json'
      - '.github/workflows/design-quality.yml'

jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      - name: Install FraiseQL
        run: |
          curl -sSL https://install.fraiseql.dev | bash

      - name: Check design quality
        run: |
          for schema in schema/*.json; do
            echo "Checking $schema..."
            fraiseql lint "$schema" \
              --fail-on-critical \
              --fail-on-warning \
              --json | jq '.data'
          done

      - name: Comment on PR
        if: failure()
        uses: actions/github-script@v6
        with:
          script: |
            github.rest.issues.createComment({
              issue_number: context.issue.number,
              owner: context.repo.owner,
              repo: context.repo.repo,
              body: 'Design quality checks failed. Run `fraiseql lint schema.json --verbose` for details.'
            })
```

### GitLab CI

```yaml
# .gitlab-ci.yml
design_quality:
  stage: validate
  script:
    - fraiseql lint schema.json --fail-on-critical
  artifacts:
    reports:
      dotenv: quality.env
  only:
    - merge_requests
    - main
```

## Agents & Automation

### Python Auditor Agent

```python
import requests
import json

def audit_schema(schema_path):
    """Audit schema and return detailed report"""
    with open(schema_path) as f:
        schema = json.load(f)

    # Call design audit API
    response = requests.post(
        'http://localhost:8080/api/v1/design/audit',
        json={'schema': schema},
        timeout=5
    )

    audit = response.json()['data']

    # Generate recommendations
    if audit['overall_score'] < 70:
        print(f"🔴 Design score: {audit['overall_score']} - Major improvements needed")
    elif audit['overall_score'] < 85:
        print(f"🟡 Design score: {audit['overall_score']} - Some improvements recommended")
    else:
        print(f"🟢 Design score: {audit['overall_score']} - Excellent!")

    # Print issues
    for category in ['federation', 'cost', 'cache', 'authorization']:
        issues = audit[category]['issues']
        if issues:
            print(f"\n{category.upper()} ({audit[category]['score']}):")
            for issue in issues:
                print(f"  • {issue['severity']}: {issue['message']}")
                print(f"    → {issue['suggestion']}")
```

### TypeScript Analyzer

```typescript
import axios from 'axios';

async function analyzeSchema(schemaPath: string) {
  const schema = require(schemaPath);

  const response = await axios.post(
    'http://localhost:8080/api/v1/design/audit',
    { schema },
    { timeout: 5000 }
  );

  const audit = response.data.data;

  // Gate PR on critical issues
  const critical = audit.severity_counts.critical;
  if (critical > 0) {
    console.error(`❌ ${critical} critical design issues found`);
    process.exit(1);
  }

  console.log(`✅ Design quality score: ${audit.overall_score}`);
}
```

## Performance

- **Design audit API**: <50ms p95 latency
- **CLI lint command**: <100ms for typical schema
- **Memory usage**: <100MB for enterprise schemas
- **Throughput**: 10,000+ concurrent API requests

See [DESIGN_QUALITY_PERFORMANCE.md](./DESIGN_QUALITY_PERFORMANCE.md) for details.

## Security

All design audit endpoints are secured with:

- Input validation
- Rate limiting
- Error message sanitization
- Authorization support

See [DESIGN_QUALITY_SECURITY.md](./DESIGN_QUALITY_SECURITY.md) for details.

## Troubleshooting

### Issue: Low score on well-designed schema

**Check**: Are you using the right pattern for FraiseQL?

- Federation: Entities should be in ONE primary subgraph
- Cost: Add pagination limits to list fields
- Cache: Specify consistent TTLs

### Issue: "Unknown rule" error

**Update**: Ensure you're using FraiseQL v2.0.0-alpha.1 or later

```bash
fraiseql --version
```

### Issue: API timeout

**Cause**: Schema too large (>100MB) or server overloaded
**Solution**:

- Split into smaller schemas
- Check server logs: `RUST_LOG=debug fraiseql-server`

## FAQ

### Q: Can I disable specific rules?
A: Not in the core, but design audit APIs return structured results so agents can implement custom policies.

### Q: Is design audit required for production?
A: No, it's optional. Use it for development and CI/CD gates if desired.

### Q: How often should I run design audit?
A: On every schema change (integrate with CI/CD).

### Q: Can I compare scores over time?
A: Yes, save audit results and track trends.

## References

- API Reference: See `/api/v1/design/*` endpoints
- CLI Help: `fraiseql lint --help`
- Performance: [DESIGN_QUALITY_PERFORMANCE.md](./DESIGN_QUALITY_PERFORMANCE.md)
- Security: [DESIGN_QUALITY_SECURITY.md](./DESIGN_QUALITY_SECURITY.md)

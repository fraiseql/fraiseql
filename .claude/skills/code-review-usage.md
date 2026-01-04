# How to Use the Code Review Prompt

## Quick Start

The comprehensive review prompt is available at:
`.claude/skills/code-review-prompt.md`

## Three Ways to Run the Review

### Option 1: Web Chat (Recommended for Quality)
1. Open Claude at https://claude.ai
2. Create a new conversation
3. Paste the entire contents of `code-review-prompt.md`
4. Let it run (expect 10-15 minute response)
5. Export the full review report

**Best for**: Comprehensive, production-ready review
**Quality**: ⭐⭐⭐⭐⭐
**Cost**: Professional quality

### Option 2: Local Model (Budget-Friendly)
```bash
# Switch to architect model (reasoning)
vllm-switch architect

# Make API call (example with curl)
curl http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d @- << 'EOF'
{
  "model": "/data/models/fp16/Ministral-3-8B-Reasoning-2512",
  "messages": [{"role": "user", "content": "[PASTE PROMPT CONTENT]"}],
  "max_tokens": 4000,
  "temperature": 0.3
}
EOF
```

**Best for**: Quick assessment, budget constraints
**Quality**: ⭐⭐⭐⭐
**Cost**: Free (local GPU)

### Option 3: Streamlined Quick Review
Use this simplified version for a focused review:

```
You are a senior Rust architect reviewing FraiseQL, a GraphQL framework.

CRITICAL FOCUS (in order):
1. Security vulnerabilities (auth, RBAC, SQL injection, data isolation)
2. Data loss risks (multi-tenancy isolation, transaction safety)
3. Performance bottlenecks (N+1 queries, memory leaks, connection handling)
4. Architecture issues (scalability limits, maintainability)

MUST VERIFY:
- Can a tenant access another's data? (multi-tenancy isolation)
- WebSocket subscription DoS protection?
- SQL injection vectors in query building?
- Rate limiting can't be bypassed?
- GraphQL query depth/complexity limits enforced?
- RBAC can't be bypassed at field level?

DELIVER AS:
✅ Executive summary (1-2 pages)
🔴 Critical issues (must fix before production)
🟡 Major issues (should fix, with effort estimates)
🟢 Minor issues (nice to have)
⭐ Positive findings
📊 Risk assessment matrix

REPOSITORY: https://github.com/fraiseql/fraiseql (or local path)
FILES TO FOCUS: auth/, rbac/, security/, db/, http/, subscriptions/
```

**Best for**: Time-constrained review, specific focus areas
**Time**: 5-10 minutes
**Cost**: Professional quality

## Expected Output Structure

```
📋 FRAISEQL CODE REVIEW REPORT

1️⃣ EXECUTIVE SUMMARY
   - Ready for production? [YES/NO]
   - Risk level: [LOW/MEDIUM/HIGH]
   - Top 3 recommendations
   - Estimated effort to fix critical issues

2️⃣ CRITICAL ISSUES (Must Fix)
   [Security vulnerabilities, data loss risks]
   - Issue #1: [Description]
     Severity: CRITICAL
     Component: [Location]
     Impact: [What breaks]
     Fix: [Recommended approach]
     Effort: [X hours]

3️⃣ MAJOR ISSUES (Should Fix)
   [Architecture, performance, maintainability]
   - Issue #1-N with same detail

4️⃣ MINOR ISSUES (Nice to Have)
   - Improvements list
   - Testing suggestions
   - Documentation gaps

5️⃣ POSITIVE FINDINGS
   - Well-designed components
   - Security wins
   - Performance optimizations

6️⃣ DETAILED ANALYSIS BY COMPONENT
   - HTTP layer (Web framework)
   - Subscriptions (Real-time)
   - Database layer
   - Security/Auth/RBAC
   - Mutations/Queries
   - Caching
   - Each with: architecture score, security analysis,
              performance notes, maintainability (1-10)

7️⃣ RISK ASSESSMENT MATRIX
   ┌─────────────────────────────────────┐
   │ Security Risk:        8/10           │
   │ Scalability Risk:      5/10           │
   │ Maintainability:       8/10           │
   │ Production Readiness:  7/10           │
   │ Overall Risk Level:    MEDIUM         │
   └─────────────────────────────────────┘

8️⃣ FINAL VERDICT
   - Deploy to production today? [YES/NO & why]
   - Riskiest component? [What & recommendations]
   - Architectural concerns? [What needs rethinking]
   - Strongest aspects? [What's excellent]
```

## Key Files to Review

**SECURITY-CRITICAL** (Review first):
- `fraiseql_rs/src/auth/**` - Authentication mechanisms
- `fraiseql_rs/src/rbac/**` - Authorization/permissions
- `fraiseql_rs/src/security/**` - Protections (rate limiting, CSRF, etc)
- `fraiseql_rs/src/db/**` - Database query safety

**CORE FUNCTIONALITY** (Review second):
- `fraiseql_rs/src/http/**` - HTTP/web layer
- `fraiseql_rs/src/subscriptions/**` - Real-time/WebSocket
- `fraiseql_rs/src/mutation/**` - Write operations
- `fraiseql_rs/src/query/**` - Read operations

**SUPPORTING** (Review last):
- `fraiseql_rs/src/cache/**` - Caching strategy
- `fraiseql_rs/src/federation/**` - Entity composition
- `fraiseql_rs/src/pipeline/**` - Middleware

## What We Already Know

✅ **Passed Quality Gates**:
- Clippy pedantic (0 warnings) across 161 files
- Strict mode (-D warnings) compliant
- Release build optimized
- No unsafe code patterns

❓ **Under Review**:
- Security of multi-tenancy implementation
- RBAC field-level enforcement
- Performance under high concurrency
- Operational readiness for production
- Architectural flexibility

## Tips for Best Results

1. **Provide Context**: Mention this is approaching production release
2. **Be Specific**: Ask about specific concerns (WebSocket scalability, SQL injection, etc)
3. **Allow Time**: Comprehensive reviews need 10-20 minutes
4. **Follow-up**: Ask clarifying questions on findings
5. **Iterate**: Run reviews of specific components separately if needed

## File Locations

```
.claude/skills/
├── code-review-prompt.md          ← Full comprehensive prompt
├── code-review-usage.md           ← This file
└── README.md                      ← Additional documentation

Repository: /home/lionel/code/fraiseql
```

## Next Steps After Review

1. **Collect Report** - Save the full review response
2. **Prioritize Issues** - Focus on critical items first
3. **Create Issues** - File GitHub issues for findings
4. **Plan Sprints** - Schedule work by effort estimates
5. **Verify Fixes** - Re-run focused reviews on addressed issues

## Expected Review Turnaround

| Approach | Time | Quality | Cost |
|----------|------|---------|------|
| Full web | 15 min | ⭐⭐⭐⭐⭐ | $$ |
| Local quick | 5 min | ⭐⭐⭐⭐ | $0 |
| Focused area | 10 min | ⭐⭐⭐⭐⭐ | $ |
| Follow-up | 5 min | ⭐⭐⭐⭐ | $ |

---

**Need to run the review?**
1. Open `.claude/skills/code-review-prompt.md`
2. Choose your approach (web/local/quick)
3. Submit the prompt
4. Analyze findings and create action items

**Last Updated**: 2026-01-04
**FraiseQL Version**: v1.9.1
**Status**: Ready for independent review

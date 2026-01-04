# FraiseQL Code Review Resources

Comprehensive, independent code review prompts and questions for FraiseQL v1.9.1.

## 📚 What's Included

### 1. **code-review-prompt.md** (230 lines)
The master review prompt - a production-grade code review specification.

**Use this when**: You want a comprehensive, professional-grade review of the entire codebase.

**Covers**:
- ✅ Architecture & Design (20%)
- ✅ Security (25%)
- ✅ Performance & Optimization (20%)
- ✅ Reliability & Error Handling (15%)
- ✅ Code Quality & Maintainability (10%)
- ✅ Operational Readiness (10%)

**Time**: 15 minutes | **Quality**: ⭐⭐⭐⭐⭐ | **Output**: 15-25 page report

---

### 2. **code-review-usage.md** (219 lines)
How to run the review and what to expect.

**Includes**:
- Three different review approaches (web/local/quick)
- Expected output structure
- Tips for best results
- Key file priorities
- Next steps after review

**Start here to understand**: How to submit the review and what to expect.

---

### 3. **targeted-review-questions.md** (243 lines)
50+ specific technical questions organized by topic.

**Use this when**: You want to focus on specific areas or verify particular concerns.

**Includes**:
- **Security Questions** (30+): Auth, RBAC, GraphQL, DoS, data protection
- **Performance Questions** (20+): N+1 queries, caching, subscriptions, memory
- **Architecture Questions** (10+): Design, scalability, operational
- **Testing & Reliability** (15+): Coverage, error handling, failure modes
- **Vulnerability Checks**: Code examples to test specific risks
- **Checklist**: 15-point production readiness checklist

**Time**: 10 minutes | **Quality**: ⭐⭐⭐⭐⭐ | **Output**: 5-10 page focused analysis

---

## 🚀 Quick Start

### For Comprehensive Review
1. Open `code-review-prompt.md`
2. Read `code-review-usage.md` → "Option 1: Web Chat"
3. Copy entire prompt and paste into Claude
4. Wait 10-15 minutes for full report

### For Quick Assessment
1. Read `code-review-usage.md` → "Option 3: Streamlined Quick Review"
2. Use the simplified prompt from that section
3. Takes 5-10 minutes

### For Specific Concerns
1. Open `targeted-review-questions.md`
2. Find the relevant section (security/performance/architecture)
3. Ask Claude those specific questions
4. Get focused analysis in 10 minutes

---

## 📊 Coverage Matrix

| Area | Prompt | Questions | Usage |
|------|--------|-----------|-------|
| Security | ✅ (25%) | ✅ (30+) | Focus here first |
| Performance | ✅ (20%) | ✅ (20+) | Critical for scale |
| Architecture | ✅ (20%) | ✅ (10+) | Long-term concerns |
| Reliability | ✅ (15%) | ✅ (15+) | Production ops |
| Code Quality | ✅ (10%) | ✅ (Tools) | Already verified |
| Operations | ✅ (10%) | ✅ (5+) | Deployment needs |

---

## ✅ What We Already Know

The FraiseQL codebase has been verified to:
- ✅ Pass clippy pedantic (0 warnings across 161 files)
- ✅ Compile successfully in strict mode (-D warnings)
- ✅ Build for production (release profile)
- ✅ Follow Rust idioms and best practices
- ✅ Have complete error documentation

**This review will assess**:
- Security & authorization
- Performance under load
- Scalability & architecture
- Operational readiness
- Production suitability

---

## 📖 How to Use Each File

### code-review-prompt.md

**Best for**: Comprehensive, executive-level assessment

**How to use**:
```
1. Open the file
2. Copy ALL content
3. Go to https://claude.ai
4. Create new conversation
5. Paste entire prompt
6. Wait for report
```

**Expected output**:
- Executive summary
- Critical issues (must fix)
- Major issues (should fix)
- Minor improvements
- Risk assessment matrix
- Component-by-component analysis

**Length**: 15-25 pages

---

### code-review-usage.md

**Best for**: Understanding how to run a review

**How to use**:
```
1. Read the "Quick Start" section
2. Choose your approach (web/local/quick)
3. Follow the specific instructions
4. Submit prompt to Claude
5. Review findings
```

**Includes**:
- 3 different review approaches with tradeoffs
- Expected output format
- Tips for best results
- File organization and priorities
- Timeline expectations
- Next steps

---

### targeted-review-questions.md

**Best for**: Deep-dive on specific areas

**How to use**:
```
1. Identify area of concern (security/performance/architecture)
2. Open the corresponding section
3. Copy relevant questions
4. Ask Claude those specific questions
5. Get focused analysis
```

**Sections**:
- 🔒 Security (Auth, RBAC, GraphQL, DoS, data protection)
- ⚡ Performance (N+1, caching, subscriptions, memory)
- 🏗️ Architecture (Design, scalability, operations)
- 🧪 Testing (Coverage, error handling, reliability)
- 📋 Checklist (15-point production readiness)

---

## 🎯 Review Priorities

**Highest Priority** (Review first):
1. Security - Can RBAC be bypassed? Is multi-tenancy isolated?
2. Data Loss - Are transactions atomic? Can data be lost?
3. Performance - Will subscriptions scale? N+1 queries?

**High Priority** (Review second):
1. Architecture - Can design scale? What breaks first under load?
2. Operational - Can it run in production? Monitoring? Recovery?
3. Reliability - How does it fail? Can it recover?

**Medium Priority** (Review third):
1. Code quality - Maintainability, documentation, testing
2. Integration - How does Python/Rust boundary work?
3. Extensibility - Can it be extended? Changed?

---

## 📊 Expected Findings

**Common findings in production code reviews**:
- Security: 3-5 findings (mix of critical and minor)
- Performance: 2-4 findings (optimization opportunities)
- Architecture: 1-3 findings (scalability concerns)
- Operational: 1-2 findings (monitoring/recovery)
- Code quality: 0-5 findings (minor improvements)

**Total expected**: 7-19 findings, mostly actionable improvements

---

## 🔄 Review Workflow

```
1. Submit Prompt
   ├─ Choose approach (web/local/quick)
   └─ Paste prompt to Claude

2. Receive Report
   ├─ Save markdown output
   └─ Review findings (30 min)

3. Analyze
   ├─ Separate critical/major/minor
   ├─ Estimate effort
   └─ Prioritize work

4. Plan Work
   ├─ Create GitHub issues
   ├─ Schedule in sprints
   └─ Assign owners

5. Verify Fixes
   ├─ Re-run focused reviews
   ├─ Confirm issues resolved
   └─ Document decisions
```

---

## ❓ FAQ

**Q: How long does a review take?**
A: 10-15 minutes to get the report, then 30 min to 1 hour to analyze and plan.

**Q: Which approach should I use?**
A: Start with "Option 1: Web Chat" for comprehensive review. Use targeted questions for specific concerns.

**Q: Will it find security issues?**
A: Yes. The prompts are designed to uncover security vulnerabilities, data loss risks, and architectural problems.

**Q: Can I use the same prompt twice?**
A: Yes. After fixing issues, submit the same prompt to verify improvements.

**Q: What if we disagree with a finding?**
A: Document why you're not fixing it. Create an ADR (Architecture Decision Record) with the rationale.

**Q: How often should we review?**
A: After major architectural changes or before production release. Also after security patches.

---

## 📝 File Locations

```
.claude/skills/
├── README.md                      ← This file
├── code-review-prompt.md          ← Main review prompt (230 lines)
├── code-review-usage.md           ← How to run reviews (219 lines)
└── targeted-review-questions.md   ← Specific technical questions (243 lines)

Repository: /home/lionel/code/fraiseql
```

---

## 🎓 Learning Resource

These prompts are also useful for:
- Learning what production-grade code reviews look like
- Understanding what reviewers look for in code
- Building a checklist for your own code reviews
- Training junior developers on review standards

---

## 📞 Support

**Need help using these resources?**
1. Read `code-review-usage.md` for instructions
2. Check `targeted-review-questions.md` for similar questions
3. Ask Claude for clarification on specific findings

**Want to improve the prompts?**
1. Note what was missing from the review
2. Add those questions to a follow-up
3. Update the prompt files with new insights

---

## 🏆 Best Practices

1. **Review Timing**: Run before production release
2. **Save Reports**: Keep all review reports for historical reference
3. **Track Changes**: Link GitHub issues to review findings
4. **Follow-Up**: Re-run review after major fixes
5. **Documentation**: Document why you accept/reject findings

---

## 📊 Metrics

**Files**: 3 markdown files, 692 lines total
**Coverage**: Architecture, Security, Performance, Reliability, Quality, Operations
**Questions**: 50+ specific technical questions
**Time Investment**: 15-20 minutes for comprehensive review

---

**Version**: 1.0
**Created**: 2026-01-04
**For**: FraiseQL v1.9.1
**Status**: Ready for independent review

---

## Ready to Review?

Choose your approach and get started:

1. **Professional Review** (15 min) → Use `code-review-prompt.md`
2. **Quick Assessment** (5 min) → Use simplified prompt from `code-review-usage.md`
3. **Focused Deep-Dive** (10 min) → Use `targeted-review-questions.md`

👉 **Next Step**: Open the file that matches your chosen approach!

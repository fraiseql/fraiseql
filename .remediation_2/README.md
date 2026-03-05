# FraiseQL — Remediation Campaign 2

Follows the post-v2.0.0 quality campaign. Issues here were identified by an
independent "rapport d'étonnement" against the completed first campaign.

## Scope

Unlike Campaign 1 (security emergency, 121 issues), this campaign is
**quality consolidation**: eliminate fragility, close architectural gaps,
and build infrastructure that prevents regression categories from re-entering
the codebase.

Benchmarks remain out of scope (handled by `../velocitybench`).

## Structure

```
.remediation_2/
├── README.md                    ← this file
├── master-plan.md               ← tracking table for all issues
├── batches/
│   ├── batch-1-thread-safety.md         ← std::thread::sleep in tokio contexts
│   ├── batch-2-clock-injection.md       ← inject Clock trait, no SystemTime::now()
│   ├── batch-3-security-regressions.md  ← tests that would have caught E1/AA1/O1–O4
│   ├── batch-4-sdk-audit.md             ← SDK CI depth and parity
│   ├── batch-5-crate-split.md           ← fraiseql-core → fraiseql-db + fraiseql-executor
│   ├── batch-6-deprecation.md           ← observers-full alias enforcement
│   └── batch-7-blocked.md               ← V3 CVE monitoring (awaiting upstream)
└── infrastructure/
    ├── pre-release-security-checklist.md ← gate before every release
    ├── security-review-gate.md           ← PR review process for auth-touching code
    ├── test-quality-standards.md         ← no new sleeps, clock injection policy
    ├── sdk-parity-matrix.md              ← which SDKs test which features
    ├── crate-size-policy.md              ← prevent re-growth of large crates
    └── threat-modeling-process.md        ← per-feature threat model requirement
```

## Severity Legend

🔴 Critical — correctness or security
🟠 High — reliability, fragility in CI, major maintenance burden
🟡 Medium — code quality, architectural hygiene
🔵 Low — polish, future-proofing

## Status Legend

✅ Done · ❌ Blocked · (blank) Pending · 🔄 In progress

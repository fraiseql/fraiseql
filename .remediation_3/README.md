# FraiseQL — Remediation Campaign 3

Follows the post-v2.1.0-dev "rapport d'étonnement". Issues here were identified
by an independent discovery review against the completed second campaign.

## Scope

Campaign 2 closed thread safety, clock injection, security regression tests,
SDK parity, crate size enforcement, and deprecation hygiene. This campaign
addresses the five findings from the new rapport d'étonnement:

1. **Test utility consolidation** — `fraiseql-test-utils` has < 5% adoption
   despite covering common needs; three inconsistent `DATABASE_URL` helpers exist.
2. **Error path coverage** — compiler, database adapters, and validators have
   sparse tests for their error branches.
3. **Property testing extension** — `proptest` is absent from `fraiseql-server`
   and `fraiseql-observers`, leaving middleware and state machine logic without
   generative coverage.
4. **Arrow Flight completeness** — `execute_placeholder_query` is on a
   production code path; three `Status::unimplemented()` stubs are undocumented
   in any issue tracker.
5. **Core split prerequisite work** — CA-1/CA-2 from Campaign 2 remain blocked
   by concrete coupling; this campaign resolves the coupling so the splits can
   land.

Benchmarks remain out of scope (handled by `../velocitybench`).

## Structure

```
.remediation_3/
├── README.md                              ← this file
├── master-plan.md                         ← tracking table for all issues
├── batches/
│   ├── batch-1-test-utility.md            ← TU-1..9: consolidate test helpers
│   ├── batch-2-error-coverage.md          ← EP-1..8: compiler/adapter error paths
│   ├── batch-3-property-testing.md        ← PT-1..5: proptest in server/observers
│   ├── batch-4-arrow-flight.md            ← AF-1..5: remove stubs, implement stubs
│   └── batch-5-core-split.md             ← CS-1..4: unblock CA-1/CA-2 and execute
└── infrastructure/
    └── test-utility-adoption-policy.md    ← mandate fraiseql-test-utils for new tests
```

## Severity Legend

🔴 Critical — correctness or security
🟠 High — reliability, fragility in CI, major maintenance burden
🟡 Medium — code quality, architectural hygiene
🔵 Low — polish, future-proofing

## Status Legend

✅ Done · ❌ Blocked · (blank) Pending · 🔄 In progress

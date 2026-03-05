# Threat Modeling Process

## Why This Exists

Campaign 1 identified that four critical security bugs were introduced via
ordinary feature PRs:

- **E1** (GET RLS bypass): The GET handler was added without the same security
  context wiring as the POST handler. A threat model asking "who can call this?"
  would have identified the gap.

- **AA1** (tenant SQL injection): A `format!()` call was used for performance
  in a hot path. A threat model asking "what is the worst case?" would have
  identified the injection risk before it shipped.

- **O1–O4** (webhook algorithm errors): Each webhook verifier was implemented
  without validating against provider test vectors. A threat model asking
  "how do I know this is correct?" would have required test vectors.

A lightweight, mandatory threat model on every security-touching PR is
the structural fix.

---

## When a Threat Model Is Required

Required for any PR that:
- Adds a new HTTP route
- Changes authentication or authorization logic
- Generates SQL (new WHERE clause, JOIN, subquery, or window function)
- Implements a new webhook verifier
- Changes cryptographic operations (key generation, signing, verification)
- Adds a new multi-tenant data path

Not required for:
- Refactoring without behavioral change
- Adding tests
- Updating documentation
- Dependency bumps (except crypto dependencies)
- UI/config changes that don't affect data access

---

## The One-Page Template

Copy this into every PR description that triggers a threat model requirement.
Fill in all fields. The reviewer will reject the PR if any field is blank or
implausible.

```markdown
## Threat Model

### What does this change do?
<!-- 1-3 sentences. What feature or fix is this? -->

### Attack surface

**Who can call this code?**
- [ ] Any unauthenticated user on the public internet
- [ ] Authenticated user (no specific scope required)
- [ ] Authenticated user with scope: _______________
- [ ] Server-to-server only (describe authentication: _______)
- [ ] Internal / background task (no external caller)

**What inputs does it accept from callers?**
<!-- List each input field and its type. Highlight any that come from
user-controlled data. -->

### Worst-case failure

**If this code has a bug, what is the worst that can happen?**
<!-- Be specific. "Data leak" is too vague. "User A can read User B's
private messages" is specific. -->

**How likely is the worst case?**
- [ ] Trivially exploitable (e.g., unauthenticated caller, direct injection)
- [ ] Requires valid authentication first
- [ ] Requires unusual input combinations
- [ ] Theoretical only

### Defenses

**What prevents the worst case?**
<!-- List each defense: middleware, parameterized queries, scope checks,
input validation, etc. For each defense, note if it has a test. -->

| Defense | Tested? |
|---------|---------|
| ___ | Yes/No |

### Regression test

**Is there a test that would fail if the worst-case bug were introduced?**
- [ ] Yes: [link to test]
- [ ] No, because: _______________
  <!-- Acceptable reasons: the risk is purely theoretical; a test would
  require live external infrastructure not available in CI; the defense
  is enforced by the type system. "It would take too long" is not acceptable. -->

### Provider documentation

**(For webhook verifiers only)**
- [ ] Algorithm verified against official provider documentation: [link]
- [ ] Test vectors from provider documentation included in the test file
```

---

## Reviewer Responsibilities

The reviewer signs off on the threat model by posting a comment:

```
Threat model reviewed.
- Attack surface: [accurate / needs clarification]
- Worst case assessment: [plausible / under/over-stated]
- Defenses adequate: [yes / no — reason]
- Regression test: [present / accepted as N/A / required before merge]

Approved / Needs revision
```

If the PR author left any field blank, the reviewer must request it be
filled before approving.

---

## Examples

### Example: Adding a New GraphQL Route

```markdown
## Threat Model

### What does this change do?
Adds a new `/graphql/batch` endpoint that accepts an array of GraphQL operations
and executes them in a single HTTP request.

### Attack surface
- [ x ] Authenticated user (no specific scope required)

Inputs: JSON body with array of `{query: string, variables: object}`.
User-controlled: the entire body.

### Worst-case failure
A malicious authenticated user sends a batch of 10,000 operations, exhausting
the database connection pool and causing a denial of service for all other users.

Likelihood: Trivially exploitable by any authenticated user.

### Defenses
| Defense | Tested? |
|---------|---------|
| Batch size limit (max 10 operations) | Yes — test_batch_size_limit |
| Each operation goes through existing auth + RLS | Yes — inherited from single-op tests |
| Connection pool timeout prevents indefinite hold | Partially — no explicit test |

### Regression test
Yes: `test_batch_size_limit_rejects_large_batch` in `graphql_batch_test.rs`.
```

---

## Escalation

If a threat model reveals a HIGH or CRITICAL risk that cannot be mitigated
within the current PR, the PR must be closed and a security issue opened
(private if the risk is exploitable). Do not merge a PR with an unmitigated
high risk by adding it to a "future work" backlog.

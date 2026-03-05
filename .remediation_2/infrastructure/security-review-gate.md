# Security Review Gate

## Purpose

Campaign 1 closed 4 critical authentication bypasses and 6 SQL injection vectors.
All were introduced in normal feature PRs. This gate defines the process that
would have caught them before merge.

---

## Trigger Conditions

A PR requires a **Security Review** when it touches any of the following paths:

```
crates/fraiseql-server/src/routes/
crates/fraiseql-server/src/middleware/
crates/fraiseql-server/src/server/routing.rs
crates/fraiseql-server/src/mcp/
crates/fraiseql-auth/src/
crates/fraiseql-core/src/tenancy/
crates/fraiseql-core/src/security/
crates/fraiseql-core/src/db/*/adapter.rs
crates/fraiseql-webhooks/src/signature/
```

**Also triggers on**:
- New HTTP route added anywhere in `fraiseql-server`
- New `format!()` call in any `src/db/` file
- Changes to JWT validation configuration
- Changes to PKCE or OAuth flows

---

## CODEOWNERS Entry

Add to `.github/CODEOWNERS`:

```
# Security-sensitive paths — require security review
crates/fraiseql-server/src/routes/        @fraiseql/security-reviewers
crates/fraiseql-server/src/middleware/    @fraiseql/security-reviewers
crates/fraiseql-server/src/server/        @fraiseql/security-reviewers
crates/fraiseql-auth/src/                 @fraiseql/security-reviewers
crates/fraiseql-core/src/tenancy/         @fraiseql/security-reviewers
crates/fraiseql-core/src/security/        @fraiseql/security-reviewers
crates/fraiseql-webhooks/src/signature/   @fraiseql/security-reviewers
```

---

## PR Review Checklist (for reviewer)

When reviewing a PR that touches security-sensitive code, the reviewer must
explicitly answer each applicable question in the PR comment:

### For new HTTP routes

```
Security review for new route: [ROUTE NAME]

- [ ] Route is registered with correct middleware (auth, rate-limit, etc.)
- [ ] Unauthenticated access is intentional and documented, or
      authentication is enforced and tested
- [ ] If the route calls DB: SecurityContext is extracted and passed (not None)
- [ ] Route is added to docs/auth/route-auth-matrix.md
- [ ] At least one test covers the unauthenticated rejection case
```

### For SQL-generating code

```
Security review for SQL generation change:

- [ ] User-supplied values are passed as bind parameters, not format! strings
- [ ] Identifiers (column names, table names) are passed through quote_*_identifier()
      or validated against a whitelist before interpolation
- [ ] Tenant ID is never interpolated (always parameterized)
- [ ] A test exists for at least one SQL-injection attempt in the new code path
```

### For webhook signature code

```
Security review for webhook verifier:

- [ ] Algorithm matches the official provider documentation (link provided)
- [ ] Test vectors from the official provider documentation are included
- [ ] Timestamp freshness is checked (replay window ≤ 5 minutes)
- [ ] Forged signature test exists (must fail verification)
```

### For auth/crypto code

```
Security review for auth/crypto change:

- [ ] No thread_rng() — OsRng only for security-sensitive randomness
- [ ] No comparison with == for secrets — subtle::ConstantTimeEq or
      constant_time_compare used
- [ ] Secret material is not derived from Debug or Display
- [ ] Token expiry is checked at every use site (not just at creation)
```

---

## Automated Checks (CI lint rules)

Add these checks to `ci.yml` under a `security-lint` job:

```bash
# No new format!() in db/ paths that could be SQL (heuristic)
git diff origin/dev...HEAD -- 'crates/*/src/db/**/*.rs' | \
  grep '^+' | grep 'format!' | \
  grep -v 'error\|Error\|warn\|debug\|info\|trace\|log\|Err(' && \
  echo "WARN: Possible SQL via format! in db/ — verify with security reviewer" || true

# No new unwrap() in auth paths
git diff origin/dev...HEAD -- 'crates/fraiseql-auth/src/**/*.rs' | \
  grep '^+' | grep '\.unwrap()' && \
  echo "ERROR: unwrap() in auth code — use expect() with message" && exit 1 || true

# No new std::thread::sleep in async contexts (approximate)
git diff origin/dev...HEAD -- 'crates/**/*.rs' | \
  grep '^+' | grep 'std::thread::sleep' | grep -v '#\[test\]' && \
  echo "WARN: std::thread::sleep added — use tokio::time::sleep in async code" || true
```

These are advisory (warn, not block) except the `unwrap()` check.
The full block is enforced by Clippy `unwrap_used = "deny"`.

---

## Threat Model Template

Every PR that triggers a Security Review must include a brief threat model
in the PR description. Use this template:

```markdown
## Threat Model

**What does this change do?**
[1-2 sentences]

**Who can call this code?**
[ ] Any unauthenticated user
[ ] Authenticated user (any scope)
[ ] Authenticated user (specific scope: _______)
[ ] Server-to-server only
[ ] Internal only (no external surface)

**What is the worst case if this code has a bug?**
[1 sentence: data leak, auth bypass, DoS, data corruption, etc.]

**What prevents that worst case?**
[Enumerate: middleware, input validation, parameterized queries, etc.]

**Is there a regression test for the dangerous case?**
[ ] Yes — [link to test]
[ ] No — explain why: _______________
```

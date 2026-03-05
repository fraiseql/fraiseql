# Pre-Release Security Checklist

**Applies to**: Every `v*.*.0` release tag. Not required for patch releases
(`v*.*.1` etc.) unless they touch auth, crypto, or tenancy.

**Process**: One engineer completes the checklist; a second engineer reviews
and counter-signs. Both names are recorded in the release commit.

---

## Section 1 — Authentication and Authorization

- [ ] All HTTP routes in `fraiseql-server` are accounted for in the auth matrix
  (see `docs/auth/route-auth-matrix.md`). No new route added without an entry.
- [ ] GET and POST `/graphql` both enforce the same `SecurityContext`
  extraction path. No shortcut that passes `None`.
- [ ] RBAC management endpoints require both authentication and `rbac:admin` scope.
- [ ] Design API endpoints are fail-closed when `design_api_require_auth = true`
  and no OIDC provider is configured (not fail-open).
- [ ] MCP endpoints enforce `require_auth` when the flag is set.
- [ ] Session refresh (`auth_refresh`) checks `is_expired()` before issuing a
  new token.
- [ ] Rate limiter clock failure defaults to denying requests (fail-closed),
  not resetting the window.
- [ ] JWT validation has `validate_aud = true` by default; `validate_aud = false`
  requires explicit config and is documented as a security risk.
- [ ] `authorization_url()` returns the state token for CSRF verification.
  `callback()` rejects mismatched state.

**Sign-off**: \_\_\_\_\_\_\_\_\_\_\_ reviewed \_\_\_\_\_\_\_\_\_\_\_ on \_\_\_\_\_\_\_\_\_

---

## Section 2 — SQL Injection

- [ ] `cargo grep -rn "format!" crates/*/src/ --include="*.rs"` reviewed.
  All `format!` calls near SQL-like strings produce error messages or
  identifier-quoted strings — never raw user input.
- [ ] Tenant ID in `TenantContext::apply_to_where_clause` is bound as a
  parameter, not interpolated.
- [ ] Window function `orderBy.field` and `partitionBy` are validated against
  a whitelist of allowed column names before interpolation.
- [ ] Arrow Flight `filter`, `order_by`, and `table` fields are quoted via
  `quote_*_identifier()` before use.
- [ ] `escape_identifier()` returns `Err` (not passes-through) on unsafe input.

**Sign-off**: \_\_\_\_\_\_\_\_\_\_\_ reviewed \_\_\_\_\_\_\_\_\_\_\_ on \_\_\_\_\_\_\_\_\_

---

## Section 3 — Webhook Signature Verification

- [ ] Twilio: `TwilioVerifier` uses HMAC-SHA1 of `URL + sorted params` (not
  HMAC-SHA1 of body). Verified against official Twilio test vectors.
- [ ] SendGrid: `SendGridVerifier` uses ECDSA P-256. Verified against official
  SendGrid test vectors.
- [ ] Paddle: Uses v2 `ts:body` HMAC-SHA256 (not deprecated v1 SHA1).
- [ ] Slack: Checks timestamp freshness (≤ 5 minutes). Replay beyond 5 min fails.
- [ ] Discord: Checks timestamp freshness. Replay fails.
- [ ] All verifiers: SR-3 through SR-6 regression tests pass.

**Sign-off**: \_\_\_\_\_\_\_\_\_\_\_ reviewed \_\_\_\_\_\_\_\_\_\_\_ on \_\_\_\_\_\_\_\_\_

---

## Section 4 — Cryptography

- [ ] No `thread_rng()` in SCRAM nonce, PKCE verifier, or AES-GCM nonce
  generation. `OsRng` only.
- [ ] `compare_padded` handles tokens of any length (not capped at 1024 bytes).
- [ ] `clock_skew_secs` is capped at a reasonable value (e.g., 300 s) in config
  validation — misconfiguration cannot accept arbitrarily old tokens.
- [ ] `RuntimeError::IntoResponse` does not call `self.to_string()` in HTTP
  response bodies — uses `ErrorSanitizer` instead.
- [ ] `FieldEncryption::fmt` (Debug impl) redacts the cipher key.

**Sign-off**: \_\_\_\_\_\_\_\_\_\_\_ reviewed \_\_\_\_\_\_\_\_\_\_\_ on \_\_\_\_\_\_\_\_\_

---

## Section 5 — Supply Chain

- [ ] `cargo audit` returns zero vulnerabilities (or all remaining are documented
  as blocked with upstream references and risk accepted).
- [ ] CI security workflows (`security.yml`, `security-compliance.yml`) use
  pinned action versions (commit SHA), not floating tags like `@main`.
- [ ] `cargo deny check` passes.
- [ ] SBOM generated and attached to the release.

**Sign-off**: \_\_\_\_\_\_\_\_\_\_\_ reviewed \_\_\_\_\_\_\_\_\_\_\_ on \_\_\_\_\_\_\_\_\_

---

## Section 6 — Regression Tests

- [ ] All security regression tests (SR-1 through SR-8) pass on this release commit:
  ```bash
  cargo nextest run --test auth_regression_test
  cargo nextest run --test rbac_auth_regression_test
  cargo test -p fraiseql-core --test tenancy_sql_injection_test
  cargo test -p fraiseql-webhooks --test twilio_replay_test
  cargo test -p fraiseql-webhooks --test sendgrid_replay_test
  cargo test -p fraiseql-webhooks --test slack_replay_test
  cargo test -p fraiseql-webhooks --test discord_replay_test
  cargo test -p fraiseql-auth --test pkce_csrf_regression_test
  ```

**Sign-off**: \_\_\_\_\_\_\_\_\_\_\_ reviewed \_\_\_\_\_\_\_\_\_\_\_ on \_\_\_\_\_\_\_\_\_

---

## Final Release Gate

Both sign-offs are required. If any item is unchecked:
- If the item is "Blocked" on an upstream dependency: document it in the
  release notes under "Known Security Limitations" with a CVE reference and
  mitigation guidance.
- If the item reflects a genuine deficiency: **block the release** until fixed.

```
Release: v_____________
Checklist completed by: ____________________  Date: ____________
Reviewed by:            ____________________  Date: ____________
Exceptions (if any):
  _______________________________________________________________
```

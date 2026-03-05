# FraiseQL Remediation Plan — Extension 15

**Assessor**: Independent reviewer (fresh eyes)
**Date**: 2026-03-05
**Prerequisite**: Extensions 1–14 reviewed; all issues below are new findings not covered there.

---

## Track AA: SQL Injection in Tenancy Module

### AA1 — `TenantContext::where_clause()` produces unescaped SQL [CRITICAL]

**File**: `crates/fraiseql-core/src/tenancy/mod.rs` ~L160, ~L214

**Description**: Two `where_clause()` methods interpolate the tenant ID directly into a SQL string literal using `format!()` without any escaping:

```rust
pub fn where_clause(&self) -> String {
    format!("tenant_id = '{}'", self.id)          // method on TenantContext
}

pub fn where_clause(tenant_id: &str) -> String {
    format!("tenant_id = '{}'", tenant_id)         // free function
}
```

A tenant with an ID of `acme'--` produces `tenant_id = 'acme'--'`, breaking the SQL syntax or silently ignoring the remaining query clauses. A tenant ID of `' OR '1'='1` allows cross-tenant data access.

**Severity**: Critical — this is in the multi-tenancy isolation boundary. A successful injection allows one tenant to read another tenant's data.

**Context**: Safe parameterized alternatives already exist in the same file (`where_clause_postgresql(param_index)`, `where_clause_parameterized()`). The bug is that the unsafe method is still exported and presumably used in some call sites.

**Remediation**:
1. Mark both unsafe `where_clause()` variants as `#[deprecated(note = "Use where_clause_postgresql or where_clause_parameterized instead")]`.
2. Audit all call sites of the unsafe methods; replace each with a parameterized variant.
3. Add a test case: `TenantContext::new("'; DROP TABLE users; --")` should produce a parameterized SQL, not raw SQL.
4. Consider making the unsafe methods `pub(crate)` to prevent external callers from reaching them.

**Verification**: `grep -r "\.where_clause()" crates/ --include="*.rs"` should return zero results using the unsafe variant after the fix.

---

## Track BB: Webhook Subscription Lifecycle Correctness

### BB1 — `on_unsubscribe` posts to the subscribe URL, not a dedicated URL [MEDIUM]

**File**: `crates/fraiseql-server/src/subscriptions/webhook_lifecycle.rs` ~L185–L200

**Description**: The `WebhookLifecycle::on_unsubscribe` implementation checks `self.on_subscribe_url` when deciding which endpoint to call, then POSTs to it:

```rust
async fn on_unsubscribe(&self, subscription_id: &str, connection_id: &str) {
    let Some(ref url) = self.on_subscribe_url else {   // BUG: wrong field
        return;
    };
    // POSTs an "unsubscribe" event body to self.on_subscribe_url
}
```

**Consequences**:
1. There is no way to configure a separate webhook endpoint that receives only unsubscribe events — the subscribe and unsubscribe events always go to the same URL.
2. If the operator configures only `on_unsubscribe_url` (if such a field is intended), unsubscribe events are silently dropped because the guard returns early.
3. External services relying on the unsubscribe webhook to clean up resources (billing, connection tracking, analytics) cannot distinguish between subscribe and unsubscribe events by URL.

**Severity**: Medium — silent behavioral failure; no data loss or security impact, but lifecycle webhook configuration is non-functional as documented.

**Remediation**:
1. Determine design intent: should subscribe and unsubscribe share a URL, or have separate fields?
   - If shared: rename the field to `on_subscribe_lifecycle_url` and document it handles both events (body distinguishes them).
   - If separate: add `on_unsubscribe_url: Option<String>` to the struct and fix the guard.
2. Add a test that configures separate URLs for subscribe and unsubscribe events and verifies both are called with the correct payloads.

---

## Track CC: Audit Log Backend Correctness

### CC1 — File audit backend opens a new file handle per event (resource exhaustion) [MEDIUM]

**File**: `crates/fraiseql-core/src/audit/file_backend.rs` ~L57–L91

**Description**: The `log_event` method opens the audit log file fresh on every call:

```rust
async fn log_event(&self, event: AuditEvent) -> AuditResult<()> {
    let _lock = self.write_lock.lock().await;

    let json_str = serde_json::to_string(&event)?;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&self.file_path)          // New OS file handle per event
        .await?;

    file.write_all(json_str.as_bytes()).await?;
    file.write_all(b"\n").await?;       // Two separate write syscalls
    file.sync_all().await?;
    Ok(())
    // handle dropped, not reused
}
```

**Issues**:
1. **Resource exhaustion**: Under high audit volume, each event opens and closes an OS file handle. The `Mutex` serializes FraiseQL writes but does not prevent OS-level resource pressure. `ulimit -n` constrains the available descriptors.
2. **Non-atomic log line**: The JSON body and the newline are written in two separate `write_all` calls. If the process is killed between these two calls, the audit log contains a partial JSON record with no terminating newline, which breaks line-oriented log parsers.
3. **`sync_all` does not guarantee durability of prior state**: `sync_all` flushes the open handle, but if the OS crashes between `write_all(json)` and `write_all(b"\n")`, the log is corrupt. For regulatory compliance purposes, partial records are equivalent to data loss.

**Severity**: Medium — audit backends are compliance-sensitive. Partial records in an audit log can fail a SOC 2 / ISO 27001 audit.

**Remediation**:
1. Reuse a single file handle stored in the struct: `file: tokio::sync::Mutex<tokio::fs::File>`.
2. Combine JSON and newline into a single `write_all` call: `file.write_all(format!("{json_str}\n").as_bytes()).await?`.
3. Add a test that simulates concurrent writes and verifies each line in the output file is valid JSON.
4. Document in the audit backend that `sync_all()` provides best-effort durability, not ACID guarantees.

---

### CC2 — PostgreSQL audit backend interpolates LIMIT/OFFSET into SQL instead of parameterizing [LOW]

**File**: `crates/fraiseql-core/src/audit/postgres_backend.rs` ~L217

**Description**: While values in WHERE clauses are correctly parameterized (`$1`, `$2`, ...), the LIMIT and OFFSET pagination values are formatted directly into the SQL string:

```rust
format!(" LIMIT {} OFFSET {}", limit, offset)
```

These values originate from the `AuditQueryOptions` struct. If those values ever reach the backend from an untrusted source (e.g., an HTTP pagination parameter), this becomes an injection vector. Even if they are currently internal, the pattern is inconsistent with the rest of the query construction and will cause a lint failure if a SQL-injection checker is added to CI.

**Severity**: Low — currently the values are `u64` casts, so injection is structurally blocked by the type system. The issue is pattern consistency and future-proofing.

**Remediation**: PostgreSQL supports parameterized LIMIT/OFFSET via `$N` placeholders since version 13. Use them.

---

## Track DD: Backpressure Controller Lifetime Soundness

### DD1 — `AdmissionPermit` stores a raw pointer as `usize`, defeating lifetime tracking [LOW]

**File**: `crates/fraiseql-server/src/resilience/backpressure.rs` ~L40–L75

**Description**: The `AdmissionPermit` struct is intended to tie its lifetime to the `AdmissionController` that issued it. The implementation achieves this via `PhantomData<&'a ()>` — but the actual "tie" is a `usize` cast of a raw pointer:

```rust
pub struct AdmissionPermit<'a> {
    _permit:     tokio::sync::OwnedSemaphorePermit,
    _controller: usize,              // raw pointer cast to integer
    _phantom:    std::marker::PhantomData<&'a ()>,
}

// In try_acquire():
Some(AdmissionPermit {
    _permit:     permit,
    _controller: std::ptr::from_ref::<Self>(self) as usize,
    _phantom:    std::marker::PhantomData,
})
```

**Analysis**:
- The `usize` is never dereferenced (the field is prefixed `_`), so there is no active use-after-free today.
- However, `PhantomData<&'a ()>` constrains `'a` to a unit reference, not to the `AdmissionController`'s actual heap lifetime. The Rust borrow checker does not verify that a permit cannot outlive its controller.
- The comment "ties permit lifetime to controller via PhantomData" is incorrect. The `PhantomData` adds a lifetime parameter to the struct but does not bind it to anything real.
- If the pattern is copied by a future contributor who adds actual pointer dereference, it will compile successfully and produce a use-after-free.

**Severity**: Low — currently inert, but the code comment is wrong and the pattern is fragile. A correct lifetime binding would be `PhantomData<&'a AdmissionController>` with the function signature returning `AdmissionPermit<'_>` borrowing from `&self`.

**Remediation**:
```rust
pub struct AdmissionPermit<'a> {
    _permit:     tokio::sync::OwnedSemaphorePermit,
    _controller: std::marker::PhantomData<&'a AdmissionController>,
}

// In try_acquire(&self) -> Option<AdmissionPermit<'_>>:
Some(AdmissionPermit {
    _permit:     permit,
    _controller: std::marker::PhantomData,
})
```

This makes the borrow checker enforce that no permit outlives its controller without storing a raw pointer.

---

## Track EE: Process-Level Observations (No Code Change Required)

### EE1 — 14 remediation plan extensions are not tracked in the repository [HIGH PROCESS RISK]

**Location**: `/tmp/fraiseql-remediation-plan*.md` (14 files, ~300 KB total)

**Description**: 110+ identified issues across 14 remediation plan documents are stored in `/tmp/` on a developer machine. They are not:
- Committed to the repository
- Converted to GitHub Issues
- Assigned to owners
- Linked to milestones or releases
- Visible to contributors who clone the repository

The planning process has produced thorough analysis. The execution process for converting that analysis into resolved issues is not visible.

**Risk**: Issues identified in extension 1 (E1: GET handler drops auth context, E2: unauthenticated RBAC API) are critical security vulnerabilities. If they remain in `/tmp/` and are not tracked, there is no mechanism to ensure they are resolved before the next release.

**Remediation**:
1. Convert each issue from extensions 1–14 to a GitHub Issue with severity label, relevant file paths, and acceptance criteria.
2. Group critical/high issues into a milestone: "Security hardening sprint."
3. Archive the extension plan files into `docs/quality/` in the repository for historical reference.
4. Retire the `/tmp/` convention; all future findings go directly to GitHub Issues.

### EE2 — `.claude/worktrees/` directory is untracked and likely stale [LOW]

**Location**: `.claude/worktrees/` (visible in git status as `??`)

**Description**: The `.claude/worktrees/` directory appears as an untracked file in git status. This is a Claude Code development artifact from a previous session. It should be added to `.gitignore` to prevent accidental commits and to avoid confusing future `git status` output.

**Remediation**: Add `.claude/worktrees/` to `.gitignore`.

---

## Summary

| ID | Component | Severity | Type | One-line |
|----|-----------|----------|------|---------|
| AA1 | tenancy/mod.rs | **Critical** | SQL Injection | `where_clause()` interpolates tenant ID into SQL without escaping |
| BB1 | webhook_lifecycle.rs | **Medium** | Logic error | `on_unsubscribe` checks `on_subscribe_url` field instead of a dedicated unsubscribe URL |
| CC1 | audit/file_backend.rs | **Medium** | Resource + Atomicity | File opened per event; two-syscall write is non-atomic; partial records possible |
| CC2 | audit/postgres_backend.rs | **Low** | Pattern inconsistency | LIMIT/OFFSET formatted into SQL instead of parameterized |
| DD1 | resilience/backpressure.rs | **Low** | Unsound pattern | `AdmissionPermit` documents lifetime tie via `PhantomData` but stores raw pointer as `usize`; binding is not actually enforced by borrow checker |
| EE1 | Process | **High** | Process | 110+ identified issues in `/tmp/`; none tracked as issues; no execution evidence |
| EE2 | `.gitignore` | **Low** | Hygiene | `.claude/worktrees/` is untracked; should be gitignored |

---

## Estimated Effort

| Issue | Effort |
|-------|--------|
| AA1 | 2–4 hours (audit all call sites + add deprecation + tests) |
| BB1 | 1 hour (design decision + one-line fix + test) |
| CC1 | 2 hours (refactor file backend to reuse handle + test) |
| CC2 | 30 min (parameterize LIMIT/OFFSET) |
| DD1 | 1 hour (correct `PhantomData` type + verify borrow checker accepts it) |
| EE1 | 1 day (bulk issue creation from extensions 1–14) |
| EE2 | 5 min |

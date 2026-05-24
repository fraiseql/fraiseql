# FraiseQL Wave-2 follow-ups

Items deferred from a previous wave because they grew larger than the wave's
scope budget. Each entry pins (a) the original finding, (b) the reason it was
deferred, (c) the suggested wave-2 approach.

## F050 — Collapse `FraiseQLError::Storage` into `FraiseQLError::File`

**Original finding:** `IMPROVEMENTS.md` F050 — `FraiseQLError::Storage` and
`FraiseQLError::File` carve the file domain across two variants with divergent
HTTP codes (500 vs 400) and divergent error categories (`"storage_error"` vs
`"file_error"`). Industrial recommendation: delete `FraiseQLError::Storage`
and migrate every caller to `FraiseQLError::File(FileError::*)`.

**Why deferred from Wave 1:** 118 call sites (not the ~60 the audit
estimated) with non-trivial semantic complexity:

- The `code` field on `FraiseQLError::Storage { message, code }` carries
  stable string discriminators (`"not_found"`, `"permission_denied"`,
  `"io_error"`, `"size_limit_exceeded"`, `"mime_type_not_allowed"`,
  `"invalid_key"`, `"not_implemented"`, `"not_supported"`).
- `fraiseql-storage/src/routes/mod.rs::storage_error_response` routes
  `Some("not_found")` to HTTP 404 and `Some("permission_denied")` to HTTP 403.
  Migrating the variant must preserve this routing.
- The current `FileError::Storage` has only `{ message, source: Option<Box<...>> }`
  — no `code` field. Migrating losslessly requires either:
  1. Extending `FileError` with backend-specific variants (`PermissionDenied`,
     `IoError`, `InvalidKey`, `NotImplemented`, `Unsupported`,
     `SizeLimitExceeded`, `MimeTypeNotAllowed`) — adds 7+ variants.
  2. Adding a `code: Option<String>` field to `FileError::Storage` — keeps
     surface small but re-introduces the string-routing anti-pattern.
  3. Adding a `FileError::Backend { provider, code, message, source }` variant
     — matches the IMPROVEMENTS.md hint about backend-specific errors.

Wave 1 honoured the IMPROVEMENTS.md hard constraint ("If F050 turns out to be
larger than expected (e.g., 100+ call sites with thorny semantics), STOP
after the first 10–20 sites… put the rest under a FOLLOW_UPS.md entry for
Wave 2 to absorb").

**Wave 1 outcome:** Variant left in place. Comprehensive rustdoc added
documenting ownership, semantics, and the planned collapse (closes F051
partially). Zero call sites migrated to avoid leaving the code in a
half-migrated state.

**Suggested Wave-2 approach:**

1. **Design call** (RFC or design discussion): pick one of the three options
   above. Option (3) — `FileError::Backend` — is the IMPROVEMENTS.md hint and
   matches industrial frameworks (cf. `aws-sdk-*` error shapes).
2. **Extend `FileError`** with the chosen shape, including a stable
   `error_code()` mapping that preserves the existing HTTP routing
   (`not_found` → 404, `permission_denied` → 403, everything else → 500
   *via the storage-backend category*, distinct from the user-fixable 400
   that other `FileError` variants return).
3. **Update `IntoResponse`** in `fraiseql-storage/src/routes/mod.rs` to match
   on the typed `FileError` variants instead of the `code` string.
4. **Migrate the 118 sites mechanically.** Most are `code: None` (~70 sites)
   and become `FileError::Storage { message, source: Some(Box::new(e)) }`.
   The `code: Some("…")` sites (~48 sites) each pick the matching typed
   variant from step 2.
5. **Delete `FraiseQLError::Storage`** once `grep -rn "FraiseQLError::Storage"
   crates/ --include="*.rs"` returns zero hits outside `fraiseql-error`
   itself.
6. **Update `IntoResponse`/`status_code`/`error_code`** in
   `crates/fraiseql-error/src/{http,core_error}.rs` to remove the
   `FraiseQLError::Storage` arms.
7. **Update `CHANGELOG.md`** with the breaking-change note (the variant
   removal is observable to any downstream `match` on `FraiseQLError`).

**Expected effort:** M (half day to one day) once the variant shape is
agreed.

**Verification gate:**
- `grep -rn "FraiseQLError::Storage" crates/ --include="*.rs"` returns 0
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo nextest run -p fraiseql-storage -p fraiseql-functions -p fraiseql-error`
- All storage-route HTTP tests still return the same status codes as before
  (404 for not_found, 403 for permission_denied, 500 for backend failures).

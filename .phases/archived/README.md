# Archived Phases

These phases are complete. Do not reopen or re-implement.

| File | What was done | Completed |
|------|--------------|-----------|
| `phase-01-format-and-cargo.md` (Cycle 1 only) | `fraiseql-server/Cargo.toml` reordered; `[[test]]` entry for `secrets_manager_integration_test` added | 2026-03-13 |
| `phase-02-clippy-and-docs.md` | `cargo clippy` → 0 errors; `cargo doc` → 0 warnings; `cargo test --test sql_snapshots` → 92 pass | 2026-03-14 |
| `phase-06-rest-transport.md` | REST transport feature (`rest-transport` Cargo flag); `router.rs`, `translator.rs`; integration tests in `ba8046eb1` | 2026-03-14 |
| `phase-07-openapi-and-edge-cases.md` | Static OpenAPI spec for admin APIs; dynamic OpenAPI from compiled schema (`openapi_gen`); partial response / null / 404 edge cases | 2026-03-14 |

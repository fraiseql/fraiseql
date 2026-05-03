# Phase 4: mTLS Integration Testing

## Objective

Verify that the `tls.rs` module correctly builds a reqwest client with mutual TLS, and
that `HttpEntityResolver` uses it when `mtls` config is provided.

## Success Criteria

- [ ] `build_mtls_client` produces a working reqwest Client with client cert + custom CA
- [ ] `HttpEntityResolver::new` with `Some(mtls_config)` uses the mTLS client
- [ ] `HttpEntityResolver::new` with `None` uses a standard HTTPS client (no regression)
- [ ] Invalid cert/key paths produce clear error messages (not panics)
- [ ] Private key material is zeroized after use (verify `Zeroizing` wrapper)

## TDD Cycles

### Cycle 1: Unit test for `build_mtls_client`

- **RED**: Test that `build_mtls_client` with valid PEM content returns `Ok(Client)`.
  Use self-signed test certs generated in the test (or checked-in fixtures).
- **GREEN**: Verify the current implementation handles concatenated cert+key PEM.
- **REFACTOR**: Improve error messages if they're generic.
- **CLEANUP**: Lint.

### Cycle 2: Error handling for bad inputs

- **RED**: Test with invalid PEM, missing file, wrong format. Assert specific error
  variants (not generic "internal error").
- **GREEN**: Verify the `FraiseQLError::Internal` messages are descriptive.
- **REFACTOR**: Consider a dedicated error variant if the messages are too generic.
- **CLEANUP**: Lint.

### Cycle 3: Zeroization

- **RED**: Verify that `Zeroizing<Vec<u8>>` is used for PEM content reads. (This is a
  code-review cycle, not a runtime test — Zeroizing's Drop is the guarantee.)
- **GREEN**: Audit `tls.rs` for any code path that reads key material without Zeroizing.
- **REFACTOR**: Fix any paths that bypass zeroization.
- **CLEANUP**: Lint.

### Cycle 4: Integration with HttpEntityResolver

- **RED**: Test that when `mtls` is `Some(config)`, the resolver's internal client differs
  from the default (at minimum: it doesn't error on construction).
- **GREEN**: Verify the plumbing in `http_resolver.rs`.
- **REFACTOR**: N/A.
- **CLEANUP**: Lint.

## Dependencies

- None (independent of Phases 1–3)

## Note

Full end-to-end mTLS testing (actual TLS handshake with a server) requires either:
- A test server with `rustls` accepting client certs (complex setup)
- An `#[ignore]` test that targets a real mTLS endpoint

For this phase, focus on unit-level correctness. Full handshake testing is a follow-up.

## Status
[ ] Not Started

# ADR-0008: Clippy Pedantic as Warn with Justified Allows

## Status: Accepted

## Context

`clippy::pedantic` catches legitimate bugs (unused variables, overly complex patterns) but generates false positives in web server code (e.g., `module_name_repetitions` for consistent naming, `too_many_lines` for necessary controller methods). Configuring pedantic as `deny` forces suppressions without rationale. Ignoring pedantic entirely misses real issues.

## Decision

Configure clippy pedantic at **warn level** with justified suppressions:

```toml
[lints.clippy]
pedantic = "warn"  # Everything, but...

[lints.clippy]
# ... then selectively allow with // Reason: comments
module_name_repetitions = "allow"  # Reason: consistent public API naming
```

Every `#[allow(clippy::...)]` must include `// Reason:` comment explaining the suppression. Module-level allows for crate-wide patterns. Review suppressions quarterly for changes in actual code complexity.

## Consequences

**Positive:**

- Catches real bugs while maintaining velocity
- Every suppression documented and auditable
- Prevents silent accumulation of warnings
- Future maintainers understand rationale

**Negative:**

- Requires discipline; developers must write rationale
- Slightly more annotation noise
- Temptation to over-suppress rather than refactor

## Alternatives Considered

1. **Pedantic as deny**: Too strict; forces excessive refactoring for false positives
2. **Pedantic as allow**: Too lax; misses real issues
3. **No lint configuration**: Inconsistent code quality; difficult to onboard developers

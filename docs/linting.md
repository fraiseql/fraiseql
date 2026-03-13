# Lint Policy

FraiseQL enforces `clippy::pedantic` at `deny` level. All warnings are treated as errors in CI.

## When you encounter a pedantic warning

1. **Fix it** if the fix is straightforward (missing docs, explicit imports, `f64::from(x)` instead of `x as f64`)
2. **Allow it at the call site** if the fix would harm readability — add `#[allow(clippy::lint_name)]` with a mandatory `// Reason:` comment
3. **Never** add workspace-level allows without a PR discussion

Every `#[allow]` MUST follow this format:

```rust
#[allow(clippy::module_name_repetitions)]
// Reason: `FraiseQLError` is the exported canonical name and must match the crate name.
pub enum FraiseQLError { ... }
```

## Common cases and recommended actions

| Lint | Recommended action |
|------|--------------------|
| `module_name_repetitions` | Allow when the type is a primary public export (canonical name) |
| `too_many_arguments` | Prefer builder pattern; allow constructors with `// Reason: constructor` |
| `cast_precision_loss` | Use `f64::from(x)` when possible; allow with comment otherwise |
| `cast_possible_truncation` | Convert to `try_into().expect("reason")` or allow with reason |
| `missing_errors_doc` | Fix: add `# Errors` section to every public fallible function |
| `missing_panics_doc` | Fix: add `# Panics` section, or convert to `-> Result` |
| `wildcard_imports` | Fix: use explicit imports |
| `items_after_statements` | Fix: hoist `let` declarations above imperative code |
| `doc_markdown` | Fix: wrap code terms in backticks |

## Workspace-level allows (existing, with justification)

The following are allowed at the workspace level because fixing them adds no value or causes API breakage:

| Lint | Reason |
|------|--------|
| `if_not_else` | Style preference; reviewers are familiar with both forms |
| `multiple_crate_versions` | Transitive dependencies cause this; not actionable |
| `must_use_candidate` | Applied selectively; blanket deny causes too many false positives |
| `option_if_let_else` | Style preference; `if let` is often clearer than `map_or_else` |
| `or_fun_call` | Explicit form is often clearer in context |
| `redundant_closure_for_method_calls` | Style preference |
| `return_self_not_must_use` | Builder pattern compatibility |
| `significant_drop_tightening` | Too aggressive; causes false positives with guards |
| `similar_names` | Domain models use similar identifiers (e.g., `from`, `from_schema`) |
| `struct_excessive_bools` | Schema structs legitimately have many boolean flags |
| `too_many_lines` | Long functions are sometimes unavoidable in parsers/compilers |
| `uninlined_format_args` | Style preference; older form is familiar to contributors |
| `unnecessary_wraps` | API consistency over strictness |
| `unused_async` | Sometimes needed for trait conformance |
| `unused_enumerate_index` | Sometimes index is needed in future iterations |
| `unused_self` | Sometimes needed for API compatibility |
| `use_self` | Style preference |

## Enforcement

```bash
# Verify zero pedantic violations:
cargo clippy --workspace --all-targets -- -D warnings
```

This runs in CI on every PR. The PR cannot merge with any clippy warning.

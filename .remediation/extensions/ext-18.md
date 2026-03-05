# FraiseQL Remediation Plan — Extension XVIII
## SQL Operator Correctness Bugs

**Assessment date**: 2026-03-05
**Scope**: `fraiseql-wire` SQL generation layer
**Priority**: HIGH (bugs produce runtime database errors or silently wrong results)

---

### FF1 — `In`/`Nin` with empty Vec generates invalid SQL [HIGH]

**File**: `crates/fraiseql-wire/src/operators/sql_gen.rs:153–182`
**Also**: `crates/fraiseql-wire/src/operators/where_operator.rs:471–531` (validate)

**Observed behaviour**:

```rust
// sql_gen.rs:153-164
WhereOperator::In(field, values) => {
    let placeholders: Vec<String> = values
        .iter()
        .map(|v| { /* ... */ })
        .collect();
    Ok(format!("{} IN ({})", field_sql, placeholders.join(", ")))
}
```

When `values` is empty, `placeholders.join(", ")` is `""`, producing:

```sql
column_name IN ()
```

`IN ()` is a **syntax error** in PostgreSQL, MySQL, and SQL Server. SQLite also rejects it.
The same applies to `WhereOperator::Nin` (line 167–183).

**Root cause**: The `validate()` method (line 471–531) handles `In(f, _)` and `Nin(f, _)`
by delegating to `f.validate()` — it never checks `values.is_empty()`.

**Impact**: Any caller that constructs an `In`/`Nin` operator from a user-supplied list
(e.g., "filter where id in selection") will receive a cryptic PostgreSQL syntax error
(`ERROR: syntax error at or near ")"`) instead of a clear application-level error.

**Fix**: In `validate()`, add:

```rust
WhereOperator::In(f, values) | WhereOperator::Nin(f, values) => {
    if values.is_empty() {
        return Err(format!(
            "{} operator requires at least one value; got an empty list",
            self.name()
        ));
    }
    f.validate()
}
```

`WhereOperator::ArrayOverlaps` generates `field && ARRAY[]` for an empty vec; PostgreSQL
accepts that (empty array is a valid typed literal), so it does **not** need the same guard.

**Missing test** (should be added):
```rust
#[test]
fn in_operator_rejects_empty_values() {
    let op = WhereOperator::In(
        Field::DirectColumn("id".to_string()),
        vec![],
    );
    assert!(op.validate().is_err());
}
```

---

### FF2 — LIKE metacharacter escaping absent from six semantic string operators [MEDIUM]

**File**: `crates/fraiseql-wire/src/operators/sql_gen.rs`

Six operators that promise **literal substring/prefix/suffix matching** embed the
user-supplied string directly into a LIKE pattern without escaping `%` or `_`:

| Operator | Line | Generated SQL |
|---|---|---|
| `Contains(f, s)` | 185 | `f LIKE '%' \|\| $n::text \|\| '%'` |
| `Icontains(f, s)` | 257 | `f ILIKE '%' \|\| $n::text \|\| '%'` |
| `Startswith(f, s)` | 268 | `f LIKE $n` (param = `"{}%".format(s)`) |
| `Istartswith(f, s)` | 276 | `f ILIKE $n` (param = `"{}%".format(s)`) |
| `Endswith(f, s)` | 284 | `f LIKE $n` (param = `"%{}".format(s)`) |
| `Iendswith(f, s)` | 292 | `f ILIKE $n` (param = `"%{}".format(s)`) |

**Bug**: if `s` contains `%` or `_`, the generated pattern is semantically wrong:

```rust
// Contains("a%b") — should match the literal string "a%b"
// Generated: LIKE '%' || 'a%b'::text || '%'
// Actual match: any string containing 'a' followed by anything followed by 'b'
```

```rust
// Startswith("pre_fix") — should match strings starting with "pre_fix"
// Generated: LIKE 'pre_fix%'
// Actual match: any string starting with "pre" + any char + "fix"
```

**`Like` and `Ilike` are correct** — they take raw patterns and are documented as such.
The bug is specific to the six higher-level semantic operators.

**Fix**: add a helper and apply it in each operator arm:

```rust
/// Escapes LIKE metacharacters so the string is treated as a literal.
fn escape_like_literal(s: &str) -> String {
    s.replace('\\', "\\\\")
     .replace('%', "\\%")
     .replace('_', "\\_")
}
```

For `Contains` / `Icontains`, apply the helper to the parameter value before inserting
it; also add `ESCAPE '\\'` to the format string:

```
field LIKE '%' || $n::text || '%' ESCAPE '\'
```

For `Startswith` / `Endswith` variants, apply the helper before appending the trailing /
leading `%` wildcard. All six arms need `ESCAPE '\\'` appended to the generated SQL.

**Affected test gaps**: the existing test suite exercises these operators for basic
matching but does not include a test where the argument contains `%` or `_`.

---

### FF3 — Vector distance `threshold` formatted directly into SQL; NaN/∞ produce invalid SQL [LOW]

**File**: `crates/fraiseql-wire/src/operators/sql_gen.rs`
Lines 336–339, 351–354, 366–368, 381–384 (all six vector distance operators)

```rust
Ok(format!(
    "l2_distance({}::vector, ${}::vector) < {}",
    field_sql, param_num, threshold   // ← f32 formatted inline
))
```

`threshold` is typed as `f32`. Rust's `Display` for `f32::INFINITY` produces `"inf"` and
for `f32::NAN` produces `"NaN"`. Both are not valid SQL numeric literals:

```sql
-- Generated when threshold = f32::INFINITY:
l2_distance(data::vector, $1::vector) < inf
-- PostgreSQL: ERROR: syntax error at or near "inf"
```

The `validate()` method does not check threshold values; the type system allows any `f32`.

**Impact**: Callers passing `f32::NAN` or `f32::INFINITY` receive an opaque database
syntax error instead of a clear validation failure. This is not an injection vector
(Rust enforces the type) but produces confusing runtime failures.

**Fix options**:
1. Parameterise `threshold` (cleanest): `params.insert(param_num2, Value::Number(threshold as f64))` and use `$n` in the format string.
2. Add a `validate()` check: `if threshold.is_nan() || threshold.is_infinite() { return Err(...) }`.

Option 1 is preferred as it also removes the risk of locale-dependent float formatting
(some Rust targets format `1.5` as `1,5`).

---

### FF4 — `WhereOperator` is a public, non-exhaustive 60-variant enum with no `#[non_exhaustive]` attribute [MEDIUM]

**File**: `crates/fraiseql-wire/src/operators/where_operator.rs:26`

```rust
#[derive(Debug, Clone)]
pub enum WhereOperator {   // ← missing #[non_exhaustive]
    Eq(Field, Value),
    // … 60+ variants …
    Lca { field: Field, others: Vec<String> },
}
```

This is a **public API surface** crate (`fraiseql-wire` re-exports it). Adding any new
operator variant (which happens routinely as new SQL features are added) is a **breaking
change** for any downstream crate that performs an exhaustive `match` on `WhereOperator`.

**Example breakage** (downstream user code):
```rust
// Compiles today; breaks silently if e.g. WhereOperator::Regex is added:
match operator {
    WhereOperator::Eq(..) => { /* … */ }
    WhereOperator::Neq(..) => { /* … */ }
    // … all 60 current variants …
}
// Missing new variant → compile error in user's crate, not in fraiseql-wire
```

**Fix**: annotate the enum:

```rust
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum WhereOperator { … }
```

This is a one-line fix with zero runtime cost. The same audit should be applied to
`Field` (`field.rs`) and `Value` (`field.rs`) which are also public enums that may gain
new variants.

---

### Summary

| ID | Severity | File | Lines | Issue |
|---|---|---|---|---|
| FF1 | HIGH | `sql_gen.rs` | 153–182 | `IN ()`/`NOT IN ()` invalid SQL for empty vec |
| FF2 | MEDIUM | `sql_gen.rs` | 185–298 | LIKE metachar `%`/`_` unescaped in 6 semantic operators |
| FF3 | LOW | `sql_gen.rs` | 336–384 | NaN/∞ threshold formatted into SQL literal |
| FF4 | MEDIUM | `where_operator.rs` | 26 | Missing `#[non_exhaustive]` on 60-variant public enum |

**None of the above overlap with Extensions I–XVII.**

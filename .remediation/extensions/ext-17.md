# FraiseQL — Rapport d'Étonnement & Remediation Plan Extension 17

*Written 2026-03-05. Seventeenth independent assessor.*
*Extends all sixteen preceding plans without duplicating them.*
*Benchmarks out of scope (handled by velocitybench).*
*All findings confirmed against HEAD (latest commit: `140eea10c`).*
*Scope: complexity analysis, fragment resolution, GraphQL introspection correctness,*
*subscription manager concurrency, and APQ variable normalization.*

---

## Context and Methodology

All sixteen existing plans were reviewed in full before this assessment. Each finding was
verified by reading source code directly. The following known issues from prior extensions
are **not** repeated:
- SQL injection in Arrow Flight, window queries, tenancy (Extensions II, VIII, XV)
- Authentication bypass on GET/RBAC/design-API handlers (Extensions I, X, XVI)
- MCP `require_auth` flag no-op (Extension XI)
- Observer failover health-check threshold never consulted (Extension III)
- Proc-macro incorrect async tracing (Extension IX)
- Vault per-call `reqwest::Client` creation (Extension XIII)
- `custom_scalar.rs` zero tests (Extension XIV)
- Field encryption wiring and codegen drop (Extension XVI — T2)
- APQ `unwrap_or_default()` on infallible path (Extension XVI — T7)
- `rustls` 0.21.x CVE (Extension XIV)

This assessment focused on four modules previous assessors did not deeply examine:
`fraiseql-core/src/graphql/` (complexity and fragments), `fraiseql-core/src/schema/introspection/`,
and `fraiseql-core/src/runtime/subscription/`.

---

## What Works Well

- **APQ storage (memory)**: TTL expiry, LRU eviction, and concurrent access via
  `std::sync::Mutex` are implemented correctly. The tests cover the key eviction edge cases
  (access-order tie-breaking, expired-before-eviction priority). Well done.
- **Subscription filter evaluation** (`manager.rs`, `evaluate_filter_condition`): All six
  `FilterOperator` variants (Eq, Ne, Gt, Gte, Lt, Lte, Contains, StartsWith, EndsWith) are
  handled with correct semantics, including the null/missing case for `Eq`. The JSON pointer
  path resolution via `get_json_pointer_value` is clean.
- **Fragment circular reference detection**: The `visited_fragments` mechanism in
  `fragment_resolver.rs` correctly prevents infinite recursion for simple A→B→A cycles. The
  backtracking (insert before recurse, remove after) allows "diamond" patterns
  (A→B→D, A→C→D) without false positives. The correctness of this mechanism is
  demonstrated by the `test_circular_reference_detection` test.
- **Introspection scalar coverage**: `type_resolver.rs::builtin_scalars()` correctly includes
  all five GraphQL built-in scalars plus the six FraiseQL custom scalars (DateTime, Date, Time,
  UUID, JSON, Decimal), each with appropriate `specifiedByURL` per GraphQL spec §3.5.5. This
  is specification-correct behavior.

---

## Findings

---

### CC1 — `ComplexityAnalyzer` counts alphabetic characters, not field identifiers [HIGH]

**File:** `crates/fraiseql-core/src/graphql/complexity.rs`, lines 71–74

**Evidence:**

```rust
// complexity.rs:55-77
for ch in query.chars() {
    match ch {
        '{' => { current_depth += 1; ... },
        '}' => { if current_depth > 0 { current_depth -= 1; } ... },
        '(' | ')' => { /* argument delimiters */ },
        c if in_braces && c.is_alphabetic() => {
            // Count this as a potential field start
            field_count += 1;   // ← counts EVERY alphabetic character
        },
        _ => {},
    }
}
```

The comment on line 72 says "Count this as a potential field start", but the condition
`c.is_alphabetic()` matches every alphabetic character inside braces — not just the first
character of each identifier. For the query `{ users { id name email } }`:

| Characters matched | Count |
|--------------------|-------|
| u, s, e, r, s (from "users") | 5 |
| i, d (from "id") | 2 |
| n, a, m, e (from "name") | 4 |
| e, m, a, i, l (from "email") | 5 |
| **Total `field_count`** | **16** |
| **Actual field count** | **3** |

The DoS limit `max_fields: 100` is therefore nonsensical: a query with three fields having
long names (`{ authorizations { authenticated_user { extended_profile } } }`) reports
`field_count = 54`, triggering rejection; while a query with 20 short fields stays under
the limit.

**No test asserts on `field_count`**: every test uses `_fields` (ignored) or checks only
`depth` and `score`. The discrepancy has existed undetected because the tests were written
to not observe the broken metric.

**Impact:** The `max_fields` complexity limit provides false security. The score metric
(`max_depth * field_count`) is wildly inflated (legitimate 3-field query gets score 48
instead of 6), causing false rejections of valid queries with long field names. Meanwhile,
a genuinely expensive query with many short identifiers (`{ a b c d e f ... }`) is
undercounted.

**Root cause:** The identifier-start detection requires tracking whether the previous
character was alphanumeric (to detect word boundaries), not just whether the current
character is alphabetic.

**Fix approach:** Parse identifiers rather than counting characters. The simplest correct
fix uses the existing pattern:

```rust
// Track whether we are mid-identifier (inside a word)
let mut in_word = false;
for ch in query.chars() {
    match ch {
        '{' => { in_braces = true; current_depth += 1; ... },
        '}' => { if current_depth > 0 { current_depth -= 1; }; in_word = false; ... },
        c if in_braces && (c.is_alphanumeric() || c == '_') => {
            if !in_word {
                field_count += 1;  // only count start of each identifier
                in_word = true;
            }
        },
        _ => { in_word = false; },
    }
}
```

Note: this is still character-based and does not account for string literals, comments, or
fragment spreads. A more thorough fix would use the `graphql-parser` crate to perform
AST-based analysis. That approach also enables fragment-aware complexity (see CC2).

**Acceptance:**
- `analyze_complexity("{ users { id name email } }")` returns `field_count = 3` (not 16).
- `analyze_complexity("{ a b c d e f g h i j }")` returns `field_count = 10`.
- A new test `test_field_count_accuracy` asserts the exact field count (not `_fields`).
- The score for a 3-field, depth-2 query is `2 * 3 = 6`, not `2 * 16 = 32`.

---

### CC2 — Fragment resolver does not increment depth for inline fragments or regular fields [HIGH]

**File:** `crates/fraiseql-core/src/graphql/fragment_resolver.rs`, lines 115–119, 153–154

**Evidence:**

```rust
fn resolve_selections(
    &self,
    selections: &[FieldSelection],
    depth: u32,
    visited_fragments: &mut HashSet<String>,
) -> Result<Vec<FieldSelection>, FragmentError> {
    if depth > self.max_depth { return Err(FragmentDepthExceeded); }

    for selection in selections {
        if let Some(fragment_name) = selection.name.strip_prefix("...") {
            if fragment_name.starts_with("on ") {
                // Inline fragment
                field.nested_fields = self.resolve_selections(
                    &field.nested_fields,
                    depth,             // ← LINE 117: depth NOT incremented
                    visited_fragments,
                )?;
            }
            // ... named fragment spreads: depth + 1 (correct)
        } else {
            // Regular field
            field.nested_fields = self.resolve_selections(
                &field.nested_fields,
                depth,                // ← LINE 154: depth NOT incremented
                visited_fragments,
            )?;
        }
    }
}
```

Only named fragment spreads (`...FragmentName`) increment the depth counter (line 144:
`depth + 1`). Inline fragments (`... on Type { ... }`) and regular nested fields do
neither. This creates a depth-bypass: the limit `max_depth: 10` counts only fragment
chain hops, not actual nesting depth.

**Attack vector:**
```graphql
# Fragment of depth 1 — named spread hits max_depth at 10
# But this bypasses: any nesting via inline fragments
fragment DeepInline on User {
  ... on User {
    ... on User {
      ... on User {
        ... on User {          # 50 levels deep, depth counter stays at 1
          id name email
        }
      }
    }
  }
}
query { user { ...DeepInline } }
```

The server resolves each `... on User` by recursing into `nested_fields` with `depth`
unchanged. The depth limit is never triggered regardless of nesting level.

**Note on current tests:** `test_depth_limit` only exercises named fragment chain depth
(Fragment0 → Fragment1 → ... → Fragment11), which correctly hits the limit. There is no
test for inline fragment depth bypass.

**Fix:** Increment depth in both inline fragment and regular field recursion paths:

```rust
// Inline fragment recursion (line 115-119):
field.nested_fields = self.resolve_selections(
    &field.nested_fields,
    depth + 1,   // was: depth
    visited_fragments,
)?;

// Regular field recursion (line 153-154):
field.nested_fields = self.resolve_selections(
    &field.nested_fields,
    depth + 1,   // was: depth
    visited_fragments,
)?;
```

**Note:** After this fix, the test `test_depth_limit` at line 396 (which chains 12 named
fragments) will still pass, since named spreads already increment depth. Any test that
relies on unlimited regular field nesting will need updating.

**Acceptance:**
- A new test `test_inline_fragment_depth_bypass_prevented` creates a fragment with 15
  nested inline `... on Type` levels and asserts `FragmentDepthExceeded` when `max_depth = 10`.
- `test_nested_fragment_spreads` (existing, line 293) continues to pass.
- `test_circular_reference_detection` (existing, line 437) continues to pass.

---

### CC3 — Introspection `type_ref()` hardcodes `TypeKind::Scalar` for all named types [MEDIUM]

**File:** `crates/fraiseql-core/src/schema/introspection/field_resolver.rs`, lines 85–102

**Evidence:**

```rust
/// Create a named scalar/object type reference node.
///
/// The `kind` is set to `Scalar` as a placeholder; clients use `name` to resolve
/// the real kind from the type map.
pub fn type_ref(name: &str) -> IntrospectionType {
    IntrospectionType {
        kind: TypeKind::Scalar,  // ← hardcoded for ALL named types
        name: Some(name.to_string()),
        ...
    }
}
```

This function is called from `schema_builder.rs` for **every** field type reference:

- Line 203: `let return_type = type_ref(&query.return_type);` — query return types
- Line 271: `of_type: Some(Box::new(type_ref(&connection_type)))` — Relay connection types
- Line 382: `of_type: Some(Box::new(type_ref("ID")))` — ID field type
- Line 406: `let return_type = type_ref(&mutation.return_type);` — mutation return types
- Line 425: `let return_type = type_ref(&subscription.return_type);` — subscription return types
- `field_resolver.rs:45,46,47,48,49,50`: all of `Object`, `Enum`, `Input`, `Interface`, `Union`,
  `Scalar` field types call `type_ref()` → all get `TypeKind::Scalar`

The doc comment acknowledges the placeholder: "clients use `name` to resolve the real kind
from the type map." This is correct for introspection clients that use two-pass resolution
(`__schema { types { name kind } }` first, then dereference type refs by name). However:

1. **GraphQL specification §4.1** requires that `__Type.kind` be correct in all positions
   where `__Type` appears, including inline field type references. There is no provision for
   a "placeholder" kind in the spec.
2. **Single-pass tools** (including graphql-codegen, some Apollo Client configurations, and
   schema-registry validators) read `kind` from the inline type reference without doing a
   second lookup. They receive `SCALAR` for object fields and may generate incorrect code
   (treating object types as scalars) or emit incorrect schema validations.
3. **`schema_builder.rs::type_ref` is exported** (pub fn via `IntrospectionBuilder::type_ref`
   at line 118): external code calling it gets the same hardcoded-Scalar kind.

**Correct behavior:** `type_ref` should map the type name to its actual kind by consulting
the schema, or the field's `FieldType` variant should determine the kind directly:

```rust
// In field_type_to_introspection (field_resolver.rs:29-83):
FieldType::Object(name) => IntrospectionType {
    kind: TypeKind::Object,   // correct kind from the variant
    name: Some(name.to_string()),
    ...
},
FieldType::Enum(name) => IntrospectionType {
    kind: TypeKind::Enum,     // correct kind
    name: Some(name.to_string()),
    ...
},
FieldType::Input(name) => IntrospectionType {
    kind: TypeKind::InputObject,
    name: Some(name.to_string()),
    ...
},
FieldType::Interface(name) => IntrospectionType {
    kind: TypeKind::Interface,
    name: Some(name.to_string()),
    ...
},
FieldType::Union(name) => IntrospectionType {
    kind: TypeKind::Union,
    name: Some(name.to_string()),
    ...
},
FieldType::Scalar(name) => IntrospectionType {
    kind: TypeKind::Scalar,   // correct: it IS a scalar
    name: Some(name.to_string()),
    ...
},
```

The `type_ref(name)` helper (used in `schema_builder.rs` where the FieldType is not
available) should be deprecated or replaced with a `type_ref_with_kind(name, kind)` variant.

**Acceptance:**
- `IntrospectionBuilder::build(schema)` for a schema with a `User` object-type field
  produces `{ type { kind: "OBJECT", name: "User" } }` for that field, not `SCALAR`.
- `graphql-codegen` or a GraphQL SDL printer applied to the introspection result emits
  correct types (smoke test with an integration fixture).
- The public `IntrospectionBuilder::type_ref` method is removed or replaced with
  `type_ref_with_kind`.

---

### CC4 — Subscription manager has TOCTOU gap between connection cleanup and new subscriptions [LOW]

**File:** `crates/fraiseql-core/src/runtime/subscription/manager.rs`, lines 176–188, 129–136

**Evidence:**

```rust
// unsubscribe_connection (lines 176-188)
pub fn unsubscribe_connection(&self, connection_id: &str) {
    if let Some((_, subscription_ids)) = self.subscriptions_by_connection.remove(connection_id) {
        // ← WINDOW: subscriptions_by_connection["conn-X"] is GONE
        // ← any concurrent subscribe("conn-X") re-creates it here
        for id in subscription_ids {                   // ← removes OLD ids
            self.subscriptions.remove(&id);
        }
    }
}

// subscribe (lines 129-136)
self.subscriptions.insert(id, active);
self.subscriptions_by_connection
    .entry(connection_id.to_string())
    .or_default()
    .push(id);
```

Between `subscriptions_by_connection.remove(connection_id)` and the loop removing old
subscription IDs, a concurrent `subscribe(connection_id, ...)` call can:
1. Insert a new `ActiveSubscription` into `subscriptions` (new id, e.g., sub-3).
2. Insert `[sub-3]` into `subscriptions_by_connection["conn-X"]`.

The cleanup loop then removes only the OLD ids (sub-1, sub-2). The new entry sub-3 remains
in `subscriptions` without a corresponding connection index entry.

**Consequence:** The orphaned `ActiveSubscription` for sub-3 never receives cleanup. It:
- Occupies memory indefinitely (until server restart).
- Continues to receive and match events via `publish_event`'s iteration of all subscriptions.
- Delivers events to sub-3's client connection — which has already disconnected.

**Scope:** This race requires concurrent subscribe and disconnect for the same connection
simultaneously. In the current server architecture (one WebSocket task per connection),
this is unlikely but possible during reconnection windows in load-balanced deployments.

**Fix:** Use a two-phase approach that prevents new subscriptions during cleanup:

```rust
pub fn unsubscribe_connection(&self, connection_id: &str) {
    // Collect IDs while leaving the entry in place, preventing new additions
    let ids_to_remove: Vec<SubscriptionId> = {
        if let Some(mut subs) = self.subscriptions_by_connection.get_mut(connection_id) {
            std::mem::take(&mut *subs)
        } else {
            return;
        }
    };
    // Now remove the empty entry and the subscriptions
    self.subscriptions_by_connection.remove(connection_id);
    for id in ids_to_remove {
        self.subscriptions.remove(&id);
    }
}
```

This reduces the window but does not eliminate it (DashMap operations are not globally
transactional). A fully race-free implementation would require a per-connection lock or
a single-writer model for subscription lifecycle events.

**Acceptance:**
- A stress test repeatedly calling `subscribe` and `unsubscribe_connection` concurrently
  for the same connection ID verifies that `subscription_count()` returns 0 after all
  disconnect calls complete.
- No subscription survives a full connection lifecycle (subscribe → N events → disconnect).

---

### CC5 — APQ variable normalization relies on implicit serde_json BTreeMap ordering [LOW]

**File:** `crates/fraiseql-core/src/apq/hasher.rs`, lines 117–127

**Evidence:**

```rust
// hash_query_with_variables (lines 117-127)
// Step 3: Normalize variables - serialize to JSON with sorted keys
// This ensures {"a":1,"b":2} and {"b":2,"a":1} produce the same hash
let variables_json = serde_json::to_string(variables).unwrap_or_default();
```

The comment claims "serialize to JSON with sorted keys" but the mechanism is implicit:
`serde_json::Value::Object` uses `serde_json::Map`, which — absent the `preserve_order`
feature — is backed by `BTreeMap` (alphabetical key order). The test at line 363 correctly
verifies this:

```rust
fn test_hash_query_with_variables_key_order_independence() {
    let vars1 = json!({"a": 1, "b": 2, "c": 3});
    let vars2 = json!({"c": 3, "a": 1, "b": 2});
    assert_eq!(hash1, hash2, "Variable key order must not affect hash");
}
```

**Fragility:** The `serde_json` crate exposes `preserve_order` as a declared feature
(confirmed in the `Cargo.lock` fingerprint files). If any transitive dependency enables
`preserve_order`, `serde_json::Map` switches from `BTreeMap` to `IndexMap`, preserving
insertion order. At that point:
- `json!({"a": 1, "b": 2})` serializes as `{"a":1,"b":2}`.
- `json!({"b": 2, "a": 1})` serializes as `{"b":2,"a":1}`.
- Both hash to different values.
- The test `test_hash_query_with_variables_key_order_independence` fails.
- Cache misses occur for equivalent queries with different variable key orders.

**The security comment on line 100** says "Different variable values ALWAYS produce different
hashes", which is only true if key order is stable. Implicit reliance on a Cargo-feature
default is a known source of subtle regressions when the dependency tree changes.

**Fix:** Make the sort explicit:

```rust
fn hash_query_with_variables(query: &str, variables: &JsonValue) -> String {
    // ...
    // Explicitly sort keys rather than relying on serde_json's BTreeMap default
    let variables_json = sort_json_keys(variables);
    let combined = format!("{query_hash}:{variables_json}");
    // ...
}

/// Serialize a JSON value with object keys sorted recursively.
fn sort_json_keys(value: &JsonValue) -> String {
    match value {
        JsonValue::Object(map) => {
            let mut sorted: Vec<(&String, &JsonValue)> = map.iter().collect();
            sorted.sort_by_key(|(k, _)| k.as_str());
            let entries: Vec<String> = sorted
                .iter()
                .map(|(k, v)| format!("\"{}\":{}", k, sort_json_keys(v)))
                .collect();
            format!("{{{}}}", entries.join(","))
        },
        JsonValue::Array(arr) => {
            format!("[{}]", arr.iter().map(sort_json_keys).collect::<Vec<_>>().join(","))
        },
        other => serde_json::to_string(other).expect("infallible for non-object/non-array"),
    }
}
```

**Acceptance:**
- `hash_query_with_variables` does not call `serde_json::to_string(variables)` directly.
- The key-ordering independence test passes even when `serde_json` is compiled with
  `--features preserve_order`.
- `cargo test -p fraiseql-core -- test_hash_query_with_variables_key_order_independence`
  still passes after adding `serde_json = { version = "1", features = ["preserve_order"] }`
  as a dev-dependency in fraiseql-core.

---

### CC6 — APQ in-memory storage: `std::sync::Mutex` blocks async executor under contention [LOW]

**File:** `crates/fraiseql-core/src/apq/memory_storage.rs`, lines 36–40, 72–124

**Evidence:**

```rust
pub struct InMemoryApqStorage {
    entries: std::sync::Mutex<HashMap<String, StoredQuery>>,
    // ...
}

#[async_trait]
impl ApqStorage for InMemoryApqStorage {
    async fn get(&self, hash: &str) -> Result<Option<String>, ApqError> {
        let mut map = self.entries.lock()  // ← std::sync::Mutex::lock()
            .map_err(|e| ApqError::StorageError(e.to_string()))?;
        // ...
    }
```

`InMemoryApqStorage` is used as the default APQ backend when Redis is not configured.
It wraps the entry map in `std::sync::Mutex`, not `tokio::sync::Mutex`. The difference:
- `std::sync::Mutex::lock()` **blocks the OS thread** while waiting for the lock.
- `tokio::sync::Mutex::lock()` **yields to the async executor** while waiting.

In a high-concurrency Tokio runtime (the default for FraiseQL), multiple async tasks
calling `InMemoryApqStorage::get()` concurrently contend for the same `std::sync::Mutex`.
Each blocked task pins an OS thread. With `num_cpus` worker threads and APQ-hot workloads,
all worker threads can be pinned to the Mutex, preventing other async tasks from running
— effectively a latency spike or, in extreme cases, a full executor stall.

**Note:** The individual lock-holding sections are synchronous (no `.await` while holding
the lock), so there is no deadlock risk. The issue is throughput and latency under
contention, not correctness.

**Recommended fix:**

Option A (minimal change): replace `std::sync::Mutex` with `tokio::sync::Mutex` in
`InMemoryApqStorage`. Methods become `async fn` naturally (already are via `async_trait`).

Option B (higher throughput): use a `DashMap<String, StoredQuery>` (already a workspace
dependency in `fraiseql-server`), which shards the lock and eliminates most contention.
The LRU eviction logic would need adaptation to work with `DashMap` (e.g., a separate
`Mutex<VecDeque<String>>` for the LRU order).

**Acceptance:**
- `InMemoryApqStorage` does not use `std::sync::Mutex`.
- A concurrency test verifies that 100 concurrent tasks can call `get` and `set` without
  executor blocking (measurable via `tokio::time::timeout` on the concurrent batch).

---

## Severity Summary

| ID | File | Issue | Severity |
|----|------|-------|----------|
| CC1 | `graphql/complexity.rs:73` | Field count counts alphabetic characters, not identifiers — DoS limits are meaningless | **High** |
| CC2 | `graphql/fragment_resolver.rs:117,154` | Inline fragments and regular field nesting don't increment depth counter — depth limit bypassed | **High** |
| CC3 | `schema/introspection/field_resolver.rs:91` | `type_ref()` hardcodes `TypeKind::Scalar` for Object/Enum/Input/Interface/Union — violates GraphQL spec §4.1 | **Medium** |
| CC4 | `runtime/subscription/manager.rs:177` | TOCTOU gap between connection cleanup and concurrent subscribe — orphaned subscription entries | **Low** |
| CC5 | `apq/hasher.rs:119` | Variable key normalization relies on implicit serde_json BTreeMap ordering — fragile across Cargo feature changes | **Low** |
| CC6 | `apq/memory_storage.rs:37` | `std::sync::Mutex` blocks async executor threads under APQ contention | **Low** |

---

## Recommended Execution Order

1. **CC2** (fragment depth bypass): one-line fix in two places; add test. No API change.
2. **CC1** (complexity field count): more involved — requires identifier-tracking logic.
   The fix is self-contained in `complexity.rs`. Update tests to assert on field count.
3. **CC3** (introspection kind): requires touching `field_resolver.rs` and `schema_builder.rs`.
   The change is mechanical (replace `type_ref(name)` calls with variant-specific kinds).
   Risk: any tests that assert `TypeKind::Scalar` for object fields must be updated.
4. **CC5** (APQ key ordering): add explicit `sort_json_keys` function; verify with
   `preserve_order` feature enabled. Affects only `hasher.rs` and its tests.
5. **CC6** (Mutex in async): replace `std::sync::Mutex` with `tokio::sync::Mutex` or
   `DashMap`; add concurrency test.
6. **CC4** (subscription TOCTOU): lower priority given current sequential WebSocket model.
   Address when subscription load-balancing is added.

---

## Non-Issues Investigated and Cleared

- **`InMemoryApqStorage::get()` borrow checker concern**: The call to `map.get_mut(hash)`
  followed by `map.remove(hash)` when `entry.is_expired()` is valid under Rust's
  Non-Lexical Lifetimes (NLL). After `entry.is_expired()` returns, NLL ends the mutable
  borrow (`entry` is no longer live), making the subsequent `map.remove(hash)` safe. This
  compiles correctly.
- **Fragment `visited_fragments.remove()` re-entry concern**: The backtracking pattern
  (insert before recursion, remove after) is intentional and correct. It prevents
  false-positive circular errors in diamond dependency graphs (A→B→D, A→C→D) while
  still catching true circular references (A→B→A). Verified against the test at line 437.
- **Subscription `sequence_counter` overflow**: `AtomicU64::fetch_add` with `Ordering::SeqCst`
  will wrap around at `u64::MAX` (18.4 × 10¹⁸). At one event per millisecond, this
  takes 584 million years. Not a practical concern.
- **APQ `hash_query_with_variables` empty-variables behavior**: Empty or null variables
  falling back to the query-only hash is intentional (documented and tested). For
  parameterless queries (which constitute the majority of APQ-eligible queries), this is
  correct and avoids unnecessary variable serialization overhead.

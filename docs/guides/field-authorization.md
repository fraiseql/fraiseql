# Dynamic field-level authorization

FraiseQL has two layers of field-level access control:

| Layer | Question it answers | Where it's declared |
|-------|--------------------|---------------------|
| **Static** — `requires_scope` | "Does this principal hold scope `X`?" | `field(requires_scope="read:User.salary", on_deny="reject"\|"mask")` |
| **Dynamic** — `FieldAuthorizer` | "May *this* principal read *this* field of *this* row, given the field's arguments?" | `field(authorize=True)` + an app-supplied `FieldAuthorizer` |

The static layer is a compile-time scope check. The **dynamic** layer (this guide,
issue #423) expresses relational / contextual rules the static layer cannot — rules that
depend on the **row** being resolved, the **principal**, and the **field arguments**.
Examples:

- show `User.email` only to the **owner** of that specific row, or an admin;
- redact `salary` unless `arguments.reason` is present **and** the caller is in the same
  org as the row;
- deny with a **domain-specific code**.

## The trait

```rust
use fraiseql_core::security::{FieldAuthorizer, FieldAuthzRequest, FieldAuthzDecision};
use fraiseql_core::schema::FieldDenyPolicy;
use fraiseql_core::error::Result;

struct OwnerOnly;

impl FieldAuthorizer for OwnerOnly {
    fn authorize_field(&self, req: &FieldAuthzRequest<'_>) -> Result<FieldAuthzDecision> {
        let owner = req.parent.and_then(|p| p.get("owner_id")).and_then(|v| v.as_str());
        if owner == Some(req.principal.user_id.as_str()) {
            Ok(FieldAuthzDecision::Allow)
        } else {
            Ok(FieldAuthzDecision::Deny {
                code:    "not_owner".to_string(),
                on_deny: FieldDenyPolicy::Mask, // null this field on this row
            })
        }
    }
}
```

`FieldAuthzRequest` carries the `principal` (`SecurityContext`), the `type_name` and
`field_name`, the **full** `parent` row (not just the selected fields — so a policy can
key on columns the client never selected, like `owner_id`), and the field `arguments`.

## Wiring it up

Register the authorizer on `RuntimeConfig`, exactly parallel to `with_rls_policy`:

```rust
use fraiseql_core::runtime::RuntimeConfig;
use std::sync::Arc;

let config = RuntimeConfig::default().with_field_authorizer(Arc::new(OwnerOnly));
```

Mark the field policy-gated in the schema. The authoring contract is the `authorize`
flag on the field, which the compiler reads into `schema.compiled.json`:

```json
{
  "name": "email",
  "type": "String",
  "nullable": true,
  "authorize": true
}
```

Only fields with `authorize: true` are passed to the authorizer, so non-gated fields incur
**zero** per-row overhead. (A per-SDK `@authorize_field` decorator that emits this flag is a
tracked follow-up; today, set `authorize` in the authored schema directly.)

## Semantics

- **Fail-closed.** Any `Err` from `authorize_field` — or a `Deny { on_deny: Reject }` —
  fails the whole query with HTTP **403 `FORBIDDEN`**; the field value is never served.
  Reserve `Err` for policy-evaluation failures (e.g. an unreachable policy backend); use
  `Deny` for ordinary, expected denials.
- **Mask vs reject.** `Deny { on_deny: Mask }` succeeds but returns `null` for that field
  on that row (per row — other rows may keep it). `Deny { on_deny: Reject }` fails the
  whole query.
- **AND-composition with `requires_scope`.** A gated field is visible only if **both** the
  static scope gate and the dynamic authorizer allow it. The static gate runs first; a
  statically-masked field is already `null` and the dynamic authorizer is not consulted
  for it.
- **Deny code.** The `code` on a `Deny { … on_deny: Reject }` is folded into the 403
  error message; it is not echoed back as a separate field.

## Path coverage

A field PEP is only as strong as its least-guarded projection path. The authorizer is
enforced per row on the **authenticated query** and **mutation** paths. Every other
projection path **fails closed** (403) when a policy-gated field could be projected, so a
missed path cannot silently leak a gated field's value:

| Path | Behaviour with a gated field |
|------|------------------------------|
| Authenticated query (`execute_with_security`) | **Enforced** per row |
| Mutation (success entity + error metadata) | **Enforced** per row |
| Unauthenticated query (`execute`) | Fail closed (no principal to authorize against) |
| REST direct projection | Fail closed |
| Relay list / `node` lookup | Fail closed (type-level) |
| Federation `_entities` | Fail closed (schema-level) |
| Aggregate / window | Not applicable — these project synthetic aggregate result types, which never carry an entity's gated field |

> **Performance note.** When a query selects a gated field, the runtime fetches the full
> row (it skips the SQL projection hint) so the authorizer sees a complete `parent`, and it
> bypasses the response cache (a per-row, per-principal decision is not safely cacheable).
> Both effects apply only to queries that actually select a gated field.

## Current limitations (tracked follow-ups)

These fail **closed** today and are tracked for a future release:

- **Nested gated fields.** Only gated fields at the top level of the entity are enforced;
  a gated field selected inside a sub-selection (e.g. `posts { author { email } }`) fails
  closed.
- **Per-row enforcement on Relay and federation `_entities`.** These paths fail closed
  rather than enforcing per row.
- **SDK `@authorize_field` surface.** The compiled-schema `authorize` flag is the authoring
  contract today; richer per-SDK decorators are a follow-up.

## See also

- [Operation-level authorization](operation-authorization.md) — the operation-level counterpart (`Authorizer`).
- [`RLSPolicy`](../architecture/overview.md) — row-level (whole-row) security.
- `field(requires_scope=…)` — static field scopes.

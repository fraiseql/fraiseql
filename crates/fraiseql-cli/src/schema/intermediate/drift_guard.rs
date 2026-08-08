//! Refuse intermediate schemas that declare something under a key the compiler does not
//! read, and say what to write instead.
//!
//! Every entry in the tables below is a key a **shipped** SDK emits today, for a control
//! that the compiler reads under a different name. Because `IntermediateSchema` uses
//! `#[serde(default)]` with no `deny_unknown_fields`, each one bound to an empty default
//! and the compile reported success:
//!
//! * `#806` — `TypeScript`, Go and Java emit `inject_params`; the compiler read `inject`. Result:
//!   every query and mutation those SDKs produced compiled with **no tenant predicate**, and
//!   `fraiseql compile` printed `✓ Schema compiled successfully`.
//! * `#807` — Go, C# and F# emit `scope`/`scopes`; the Rust authoring SDK emits `requiresScope`;
//!   Java and Elixir emit the plural `requires_scopes`. The compiler reads `requires_scope`.
//!   Result: the compiled field carries `requires_scope: None`, which the runtime field filter
//!   treats as *public and always accessible* — on a column the author explicitly gated and the SDK
//!   validated the grammar of.
//!
//! `#806`'s key half is retired structurally: the intermediate wire key is now
//! `inject_params`, the same name the compiled schema uses, so an SDK author reading a
//! compiled artifact to learn the name gets it right. This guard covers the rest — the
//! spellings that remain wrong, and the old `inject` key — and it **fails the compile**
//! rather than aliasing them. An alias would keep six spellings working and leave the
//! seventh SDK free to invent a seventh; an error tells the author exactly what to write,
//! once.
//!
//! `#890` is the same drift one layer out from security: `return_array` is the
//! `[queries.*]` TOML spelling of the key the JSON/intermediate surface calls
//! `returns_list`. That one no longer silently drops — `deny_unknown_fields` on
//! `IntermediateQuery` has refused it since `#755` — but it is refused with a serde message
//! listing all seventeen valid field names, which never tells an author who wrote the TOML
//! spelling that the two authoring surfaces disagree about one word. This module supplies
//! that sentence; serde supplies the refusal.
//!
//! Scope is deliberately narrow: these specific keys, on these specific structures. The
//! general "no unknown fields anywhere in the compiled-schema seam" invariant is a larger
//! change that belongs with the rest of that class.

use serde_json::Value;

/// Injection keys that do not bind, and the key to use instead.
const DRIFTED_INJECT_KEYS: &[(&str, &str)] = &[("inject", "inject_params")];

/// Operation-shape keys that do not bind, and the key to use instead (`#890`).
///
/// Distinct from the security tables above: a drop here does not open an access hole, it
/// changes the *cardinality* the compiled query advertises. It earns a place in this guard
/// because the failure is the same one — one concept, two authoring spellings — and because
/// the author who hits it is following the other surface's documentation correctly.
const DRIFTED_SHAPE_KEYS: &[(&str, &str)] = &[("return_array", "returns_list")];

/// Field-scope keys that do not bind, and the key to use instead.
const DRIFTED_SCOPE_KEYS: &[&str] = &[
    "scope",
    "scopes",
    "requiresScope",
    "requiresScopes",
    "requires_scopes",
];

/// The key the compiler reads for a field-level scope requirement.
const CANONICAL_SCOPE_KEY: &str = "requires_scope";

/// Reject a raw intermediate schema that declares something under a drifted key.
///
/// Called on the raw JSON *before* deserialization, because after deserialization the
/// evidence is gone: that is the entire defect — the key vanishes into a default and
/// nothing downstream can tell "the author declared no scope" from "the author declared a
/// scope the compiler could not see".
///
/// # Errors
///
/// Returns a message naming the operation or field, the key found, and the key to use.
pub fn reject_drifted_keys(raw: &Value) -> Result<(), String> {
    for collection in ["queries", "mutations"] {
        for operation in raw.get(collection).and_then(Value::as_array).into_iter().flatten() {
            let name = operation.get("name").and_then(Value::as_str).unwrap_or("<unnamed>");
            for (found, canonical) in DRIFTED_INJECT_KEYS {
                if operation.get(*found).is_some() {
                    return Err(format!(
                        "{collection} → '{name}' declares server-side parameter injection under \
                         `{found}`, which the compiler does not read — it would compile to a query \
                         with no injected filter and no diagnostic. Rename it to `{canonical}`. \
                         Values may be either \"jwt:<claim>\" or {{\"source\": \"jwt\", \"claim\": \
                         \"<claim>\"}}."
                    ));
                }
            }
            for (found, canonical) in DRIFTED_SHAPE_KEYS {
                if operation.get(*found).is_some() {
                    return Err(format!(
                        "{collection} → '{name}' declares list-ness under `{found}`, which is the \
                         TOML `[queries.*]` spelling — this authoring surface reads `{canonical}`. \
                         Rename it to `{canonical}`. The two surfaces name one concept two ways; \
                         only `{canonical}` binds here."
                    ));
                }
            }
        }
    }

    for type_def in raw.get("types").and_then(Value::as_array).into_iter().flatten() {
        let type_name = type_def.get("name").and_then(Value::as_str).unwrap_or("<unnamed>");
        for field in type_def.get("fields").and_then(Value::as_array).into_iter().flatten() {
            let field_name = field.get("name").and_then(Value::as_str).unwrap_or("<unnamed>");
            for found in DRIFTED_SCOPE_KEYS {
                if field.get(*found).is_some() {
                    return Err(format!(
                        "field '{type_name}.{field_name}' declares a scope requirement under \
                         `{found}`, which the compiler does not read — the compiled field would \
                         carry no scope at all and the runtime would serve it to callers with no \
                         scopes. Rename it to `{CANONICAL_SCOPE_KEY}` (a single scope string; \
                         multiple required scopes are not supported by the runtime field filter)."
                    ));
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests;

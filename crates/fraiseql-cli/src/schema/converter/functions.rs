//! Compile-time validation of the `functions` section (#1325).
//!
//! Every check here used to run at **server boot** or not at all. A declaration
//! that can never fire — a `before:mutation:` naming no mutation, an
//! `after:mutation:` naming a type no mutation returns, a function whose module is
//! not on disk — produced a clean `✓ Schema compiled successfully` and then either
//! a boot failure in production or, worse, a server that started fine and silently
//! never ran the function.
//!
//! There are three kinds of check here, and where each one *can* live differs:
//!
//! * **Trigger grammar, `when` predicates, and the `http:` / `after:storage` refusals** are
//!   [`TriggerRegistry::validate_definitions`] — one rule, called here at compile time and again by
//!   the server's schema loader. The server keeps calling it because a compiled schema is an input
//!   it does not produce: a hand-written or stale artifact must still fail at boot. That is two
//!   *call sites*, not two copies; the loader's own `VALID_TRIGGER_PREFIXES` list was a copy, and
//!   it had already fallen two trigger kinds behind.
//! * **Cross-references into the schema** can only run here. The server never sees the authored
//!   names — by the time it loads, `after:mutation:Order` is just a string, and the only evidence
//!   that no mutation returns an `Order` is the function that never fires.
//! * **The module on disk** is checked at both ends, but on different evidence: the compiler sees
//!   the project layout and the server sees the deployment one, and they need not be the same
//!   directory. See `modules_are_present` below.

use anyhow::{Result, bail};
use fraiseql_core::schema::CompiledSchema;
use fraiseql_functions::{
    FunctionsConfig,
    triggers::registry::{ParsedTrigger, QueryFunctionBinding, TriggerRegistry},
};

/// Validate the compiled `functions` section against the schema it ships with.
///
/// Reports **every** failing declaration rather than the first, so one compile names
/// the full list — a project adding five functions at once should not need five
/// round trips.
///
/// `functions` is an `Option` because one of the checks is about the schema's side
/// of the pairing: a query declaring `function = "<name>"` in a project with **no**
/// functions section is precisely the case a `Some`-only call site waves through,
/// and it is the one an author reaches by deleting a function and forgetting the
/// query (#1329).
///
/// # Errors
///
/// Returns an error naming each function that cannot fire as declared, and each
/// query that does not pair with one.
pub fn validate_against_schema(
    functions: Option<&FunctionsConfig>,
    schema: &CompiledSchema,
) -> Result<()> {
    let definitions: &[fraiseql_functions::FunctionDefinition] =
        functions.map_or(&[], |f| f.definitions.as_slice());

    // The shared rule first: a trigger that does not parse cannot be cross-referenced,
    // and its diagnosis is more useful than a follow-on "names no mutation".
    if let Err(error) = TriggerRegistry::validate_definitions(definitions) {
        bail!("{}", error.message);
    }

    // The query ↔ `request:query` pairing, in both directions (#1329). The same
    // rule the server's loader applies at boot, from its one implementation.
    let bindings: Vec<QueryFunctionBinding<'_>> = query_function_bindings(schema);
    if let Err(error) = TriggerRegistry::validate_query_bindings(&bindings, definitions) {
        bail!("{}", error.message);
    }

    let mut failures: Vec<String> = Vec::new();

    for definition in definitions {
        // `validate_definitions` above already proved every trigger parses.
        let Ok(parsed) = ParsedTrigger::parse(&definition.trigger) else {
            continue;
        };

        match parsed {
            ParsedTrigger::BeforeMutation { mutation_name } => {
                if !schema.mutations.iter().any(|m| m.name == mutation_name) {
                    failures.push(format!(
                        "  function `{}`: trigger `{}` names no declared mutation. The chain runs \
                         keyed on the mutation's name; declared mutations are: {}",
                        definition.name,
                        definition.trigger,
                        name_list(schema.mutations.iter().map(|m| m.name.as_str())),
                    ));
                }
            },
            ParsedTrigger::AfterMutation { entity_type, .. } => {
                // The dispatcher keys on the *mutation's return type*, not its name
                // (`plan_after_mutation_dispatch` builds the entity event from
                // `definition.return_type`). A trigger naming anything else is dead.
                if !schema.mutations.iter().any(|m| m.return_type == entity_type) {
                    failures.push(format!(
                        "  function `{}`: trigger `{}` names `{entity_type}`, which no declared \
                         mutation returns — the trigger would never fire. after:mutation matches \
                         the mutation's RETURN TYPE, not its name (and `auto_error_union` rewrites \
                         that to the union); types returned by a mutation are: {}",
                        definition.name,
                        definition.trigger,
                        name_list(schema.mutations.iter().map(|m| m.return_type.as_str())),
                    ));
                } else {
                    failures.extend(unknown_predicate_fields(definition, &entity_type, schema));
                }
            },
            // `after:capture` targets a `@subscribable` entity written outside
            // FraiseQL, `cron` names no schema object, and `after:ingest` names an
            // inbound source — none of them cross-reference the schema. `http:` and
            // `after:storage` never reach here: `validate_definitions` refuses them.
            // `request:query` does cross-reference the schema, and is checked above
            // by `validate_query_bindings` — in the other direction as well, which
            // this loop's shape (one pass over the definitions) cannot express.
            _ => {},
        }
    }

    if let Some(functions) = functions {
        if let Err(module_failures) = modules_are_present(functions) {
            failures.extend(module_failures);
        }
    }

    if !failures.is_empty() {
        bail!(
            "the compiled `functions` section declares {} function(s) that cannot run as \
             written:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
    Ok(())
}

/// The `when` predicates name fields that exist on the triggering entity type.
///
/// A predicate is evaluated against the row image, which on this path is the GraphQL
/// projection of the mutation's return type — so its `field` is a compiled field
/// name. A predicate naming a field that does not exist never matches, and a
/// function that never fires looks exactly like one whose condition was not met.
///
/// Scoped to `after:mutation` deliberately. `after:capture` images come from the
/// change-log reader and are keyed by database column, a different name space the
/// compiled type does not describe — a check written against the wrong one would
/// refuse correct declarations.
fn unknown_predicate_fields(
    definition: &fraiseql_functions::FunctionDefinition,
    entity_type: &str,
    schema: &CompiledSchema,
) -> Vec<String> {
    let Some(type_def) = schema.types.iter().find(|t| t.name == entity_type) else {
        // The trigger named a type no mutation returns — already reported, or the
        // type is synthesized. Either way there is nothing to check fields against.
        return Vec::new();
    };

    definition
        .when
        .iter()
        .filter(|predicate| !type_def.fields.iter().any(|f| f.name == predicate.field))
        .map(|predicate| {
            format!(
                "  function `{}`: `when` predicate names field `{}`, which `{entity_type}` does \
                 not have — the predicate can never match, so the function would never fire. \
                 `{entity_type}` has: {}",
                definition.name,
                predicate.field,
                name_list(type_def.fields.iter().map(|f| f.name.as_str())),
            )
        })
        .collect()
}

/// Every declared function has a module file its runtime can load.
///
/// **Only when `module_dir` is present at compile time.** The compiler is then
/// looking at the real project layout, and a missing module is a typo it can name.
/// When the directory is absent the compiler is plainly not looking at the
/// deployment layout — a CI job compiling before the `.wasm` artifacts are fetched,
/// say — and refusing would block a legitimate workflow over a fact it cannot
/// observe. This is the same trade `--database` makes for column validation.
fn modules_are_present(functions: &FunctionsConfig) -> Result<(), Vec<String>> {
    if !functions.module_dir.is_dir() {
        tracing::warn!(
            module_dir = %functions.module_dir.display(),
            "functions module_dir does not exist at compile time — module presence and \
             runtime/extension agreement are unverified; the server checks them at boot"
        );
        return Ok(());
    }

    let failures: Vec<String> = functions
        .definitions
        .iter()
        .filter(|definition| definition.resolve_module_path(&functions.module_dir).is_none())
        .map(|definition| {
            format!(
                "  function `{}`: no module for the {:?} runtime at {} — the server loads \
                 `<module_dir>/<name>.<ext>`, so the file name must match the declared function \
                 name and the extension must match the declared runtime",
                definition.name,
                definition.runtime,
                definition.module_path_pattern(&functions.module_dir),
            )
        })
        .collect();

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}

/// The function-backed queries in a compiled schema, as bindings to check (#1329).
///
/// Built from the **compiled** schema rather than the intermediate one, so it sees
/// the same `function` value the server will read at boot — the two call sites of
/// `validate_query_bindings` are then answering a question about the same bytes.
fn query_function_bindings(schema: &CompiledSchema) -> Vec<QueryFunctionBinding<'_>> {
    schema
        .queries
        .iter()
        .filter_map(|query| {
            query.function.as_deref().map(|function| QueryFunctionBinding {
                query: query.name.as_str(),
                function,
            })
        })
        .collect()
}

/// Render a de-duplicated, sorted name list for a diagnostic, or say there are none.
fn name_list<'a>(names: impl Iterator<Item = &'a str>) -> String {
    let mut names: Vec<&str> = names.collect();
    names.sort_unstable();
    names.dedup();
    if names.is_empty() {
        "(none declared)".to_string()
    } else {
        names.join(", ")
    }
}

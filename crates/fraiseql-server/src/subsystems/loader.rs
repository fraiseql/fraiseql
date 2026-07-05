//! Load function modules from disk and assemble the functions-runtime subsystem.
//!
//! The compiled schema declares each function (name, trigger, runtime) and a
//! `module_dir`; the compiled/authored module lives on disk as
//! `<module_dir>/<name>.<ext>` (`.wasm` for WASM, `.js`/`.ts` for Deno). This
//! module reads those files, builds the runtime observer with the compiled-in
//! runtimes registered, and assembles the [`FunctionsSubsystem`] the server turns
//! into before-mutation hooks.
//!
//! **Fail-loud:** a declared function whose module file is missing or unreadable,
//! or whose runtime is not compiled into this build, aborts startup — a declared
//! function that can never run is a misconfiguration, not something to skip
//! silently (it is the very class of bug that leaves after:mutation work
//! mysteriously never firing).

use std::{collections::HashMap, path::Path};

use fraiseql_error::{FraiseQLError, Result};
use fraiseql_functions::{
    FunctionModule, FunctionObserver, RuntimeType, triggers::TriggerRegistry,
    types::FunctionDefinition,
};

use super::FunctionsSubsystem;
use crate::schema::loader::FunctionsConfig;

/// Build the functions-runtime subsystem from the compiled-schema functions config.
///
/// Loads each declared function's module from `config.module_dir`, registers the
/// runtimes compiled into this build, and assembles the observer + trigger
/// registry.
///
/// # Errors
///
/// Returns [`FraiseQLError::Configuration`] if a declared module file is missing or
/// unreadable, a function targets a runtime not compiled in, the trigger set is
/// invalid, or a runtime engine fails to initialize.
pub fn build_functions_subsystem(config: FunctionsConfig) -> Result<FunctionsSubsystem> {
    let module_registry = load_modules(&config)?;

    let trigger_registry =
        TriggerRegistry::load_from_definitions(&config.definitions).map_err(|error| {
            FraiseQLError::Configuration {
                message: format!("invalid function triggers: {error}"),
            }
        })?;

    let mut observer = FunctionObserver::new();

    // Register the runtimes compiled into this build. `functions-runtime` always
    // pulls the WASM runtime; the Deno runtime is opt-in (`functions-runtime-deno`).
    observer.register_runtime(
        RuntimeType::Wasm,
        fraiseql_functions::runtime::wasm::WasmRuntime::new(
            &fraiseql_functions::runtime::wasm::WasmConfig::default(),
        )
        .map_err(|error| FraiseQLError::Configuration {
            message: format!("failed to initialize the WASM function runtime: {error}"),
        })?,
    );
    #[cfg(feature = "functions-runtime-deno")]
    observer.register_runtime(
        RuntimeType::Deno,
        fraiseql_functions::runtime::deno::DenoRuntime::new(
            &fraiseql_functions::runtime::deno::DenoConfig::default(),
        )
        .map_err(|error| FraiseQLError::Configuration {
            message: format!("failed to initialize the Deno function runtime: {error}"),
        })?,
    );

    Ok(FunctionsSubsystem {
        observer: std::sync::Arc::new(observer),
        trigger_registry,
        module_registry,
        config,
    })
}

/// Load every declared function's module, keyed by function name.
fn load_modules(config: &FunctionsConfig) -> Result<HashMap<String, FunctionModule>> {
    let mut registry = HashMap::with_capacity(config.definitions.len());
    for definition in &config.definitions {
        let module = load_one_module(&config.module_dir, definition)?;
        registry.insert(definition.name.clone(), module);
    }
    Ok(registry)
}

/// Load one function's module from `<module_dir>/<name>.<ext>`, trying each
/// extension the function's runtime supports.
fn load_one_module(module_dir: &Path, definition: &FunctionDefinition) -> Result<FunctionModule> {
    if !runtime_compiled_in(definition.runtime) {
        return Err(FraiseQLError::Configuration {
            message: format!(
                "function {:?} targets the {:?} runtime, which is not compiled into this build \
                 (enable the corresponding `functions-runtime*` feature)",
                definition.name, definition.runtime
            ),
        });
    }

    for extension in definition.runtime.supported_extensions() {
        let path = module_dir.join(format!("{}{extension}", definition.name));
        if !path.exists() {
            continue;
        }
        return build_module(definition, &path);
    }

    Err(FraiseQLError::Configuration {
        message: format!(
            "function {:?} declares the {:?} runtime but no module file was found at {}/{}.{{{}}}",
            definition.name,
            definition.runtime,
            module_dir.display(),
            definition.name,
            definition
                .runtime
                .supported_extensions()
                .iter()
                .map(|ext| ext.trim_start_matches('.'))
                .collect::<Vec<_>>()
                .join(","),
        ),
    })
}

/// Read `path` into a [`FunctionModule`] appropriate for the function's runtime:
/// raw bytecode for WASM, source text for Deno.
fn build_module(definition: &FunctionDefinition, path: &Path) -> Result<FunctionModule> {
    match definition.runtime {
        RuntimeType::Wasm => {
            let bytecode = std::fs::read(path).map_err(|error| FraiseQLError::Configuration {
                message: format!(
                    "failed to read WASM module for function {:?} at {}: {error}",
                    definition.name,
                    path.display()
                ),
            })?;
            Ok(FunctionModule::from_bytecode(definition.name.clone(), bytecode.into()))
        },
        RuntimeType::Deno => {
            let source =
                std::fs::read_to_string(path).map_err(|error| FraiseQLError::Configuration {
                    message: format!(
                        "failed to read Deno module for function {:?} at {}: {error}",
                        definition.name,
                        path.display()
                    ),
                })?;
            Ok(FunctionModule::from_source(definition.name.clone(), source, RuntimeType::Deno))
        },
        // `RuntimeType` is non-exhaustive; a future runtime lands with its own arm.
        other => Err(FraiseQLError::Configuration {
            message: format!(
                "function {:?} declares an unsupported runtime {other:?}",
                definition.name
            ),
        }),
    }
}

/// Whether the given runtime is compiled into this build.
const fn runtime_compiled_in(runtime: RuntimeType) -> bool {
    match runtime {
        // `functions-runtime` always pulls the WASM runtime.
        RuntimeType::Wasm => true,
        RuntimeType::Deno => cfg!(feature = "functions-runtime-deno"),
        // Unknown future runtime → not compiled in.
        _ => false,
    }
}

#[cfg(test)]
mod tests;

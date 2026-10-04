//! Load function modules from disk and assemble the functions-runtime subsystem.
//!
//! The compiled schema declares each function (name, trigger, runtime) and a
//! `module_dir`; the compiled/authored module lives on disk as
//! `<module_dir>/<name>.<ext>` (`.wasm` for WASM, `.js`/`.ts` for Deno). This
//! module reads those files, builds the runtime observer with the compiled-in
//! runtimes registered, and assembles the [`FunctionsSubsystem`] the server turns
//! into before-mutation hooks.
//!
//! A scheduled source's connector (#1399) is loaded the same way, from
//! `<module_dir>/<function>.<ext>` as a Deno module: it is bound by name and has no
//! function definition, because no trigger describes "run by a source".
//!
//! **Fail-loud:** a declared function or connector whose module file is missing or unreadable,
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

/// A scheduled source's connector: the Deno module a source runs, bound by name.
#[derive(Debug, Clone, Copy)]
pub struct SourceConnector<'a> {
    /// The source that runs it, for diagnostics.
    pub source:   &'a str,
    /// The connector's name — its module is `<module_dir>/<function>.<ext>`.
    pub function: &'a str,
}

/// Build the functions-runtime subsystem from the compiled-schema functions config.
///
/// Loads each declared function's module, and each source connector's, from
/// `config.module_dir`, registers the runtimes compiled into this build, and
/// assembles the observer + trigger registry. A connector that is also a declared
/// function shares that function's module.
///
/// # Errors
///
/// Returns [`FraiseQLError::Configuration`] if a declared or connector module file is
/// missing or unreadable, a module targets a runtime not compiled in, the trigger set
/// is invalid, or a runtime engine fails to initialize.
pub fn build_functions_subsystem(
    config: FunctionsConfig,
    connectors: &[SourceConnector<'_>],
) -> Result<FunctionsSubsystem> {
    let module_registry = load_modules(&config, connectors)?;

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

/// Load every declared function's module and every connector's, keyed by name.
fn load_modules(
    config: &FunctionsConfig,
    connectors: &[SourceConnector<'_>],
) -> Result<HashMap<String, FunctionModule>> {
    let mut registry = HashMap::with_capacity(config.definitions.len() + connectors.len());
    for definition in &config.definitions {
        let module = load_one_module(&config.module_dir, definition)?;
        registry.insert(definition.name.clone(), module);
    }
    for connector in connectors {
        if !registry.contains_key(connector.function) {
            let module = load_connector(&config.module_dir, *connector)?;
            registry.insert(connector.function.to_string(), module);
        }
    }
    Ok(registry)
}

/// Load a source connector's module: a Deno module at `<module_dir>/<function>.<ext>`.
fn load_connector(module_dir: &Path, connector: SourceConnector<'_>) -> Result<FunctionModule> {
    let runtime = RuntimeType::Deno;
    if !runtime_compiled_in(runtime) {
        return Err(FraiseQLError::Configuration {
            message: format!(
                "source {:?} runs the Deno connector {:?}, but the Deno runtime is not compiled \
                 into this build (enable `functions-runtime-deno`)",
                connector.source, connector.function
            ),
        });
    }
    let Some(path) = runtime.resolve_module_path(module_dir, connector.function) else {
        return Err(FraiseQLError::Configuration {
            message: format!(
                "source {:?} runs the connector {:?}, but no module was found at {} — a \
                 connector is loaded from `<module_dir>/<function>.<ext>`, so the file name \
                 must match the source's `function`",
                connector.source,
                connector.function,
                runtime.module_path_pattern(module_dir, connector.function),
            ),
        });
    };
    build_module(connector.function, runtime, &path)
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

    // One definition of where a function's code lives, shared with the compiler's
    // compile-time check and the `functions invoke` harness (#1325).
    if let Some(path) = definition.resolve_module_path(module_dir) {
        return build_module(&definition.name, definition.runtime, &path);
    }

    Err(FraiseQLError::Configuration {
        message: format!(
            "function {:?} declares the {:?} runtime but no module file was found at {}",
            definition.name,
            definition.runtime,
            definition.module_path_pattern(module_dir),
        ),
    })
}

/// Read `path` into a [`FunctionModule`] for `runtime`: raw bytecode for WASM, source
/// text for Deno.
fn build_module(name: &str, runtime: RuntimeType, path: &Path) -> Result<FunctionModule> {
    match runtime {
        RuntimeType::Wasm => {
            let bytecode = std::fs::read(path).map_err(|error| FraiseQLError::Configuration {
                message: format!(
                    "failed to read WASM module for function {name:?} at {}: {error}",
                    path.display()
                ),
            })?;
            Ok(FunctionModule::from_bytecode(name.to_string(), bytecode.into()))
        },
        RuntimeType::Deno => {
            let source =
                std::fs::read_to_string(path).map_err(|error| FraiseQLError::Configuration {
                    message: format!(
                        "failed to read Deno module for function {name:?} at {}: {error}",
                        path.display()
                    ),
                })?;
            Ok(FunctionModule::from_source(name.to_string(), source, RuntimeType::Deno))
        },
        // `RuntimeType` is non-exhaustive; a future runtime lands with its own arm.
        other => Err(FraiseQLError::Configuration {
            message: format!("function {name:?} declares an unsupported runtime {other:?}"),
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

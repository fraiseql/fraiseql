//! The one function-module loader: the server's and `fraiseql functions invoke`'s.
//!
//! A function's module is `<module_dir>/<name>.<ext>` ([`RuntimeType::resolve_module_path`]).
//! A WASM module is read as bytes. A Deno module is read as text, its shared modules are
//! walked and read (`graph::collect`, built with `runtime-deno`), and its integrity hash covers
//! the function and every one of them
//! ([`hash::function_hash`](crate::modules::hash::function_hash)). Both callers load
//! through [`load_function_module`], so the harness runs exactly what the server would.

use std::path::PathBuf;

use super::layout::{SharedDirError, check_shared_dir};
#[cfg(feature = "runtime-deno")]
use super::{graph, hash::function_hash};
use crate::types::{FunctionModule, FunctionsConfig, RuntimeType};

/// Why a function's module cannot be loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LoadError {
    /// No module file for the function's runtime under `module_dir`.
    NoModule {
        /// The function.
        name:    String,
        /// Its runtime.
        runtime: RuntimeType,
        /// The paths tried: `<module_dir>/<name>.{ext,…}`.
        pattern: String,
    },
    /// The module file could not be read.
    Read {
        /// The function.
        name:    String,
        /// The file.
        path:    PathBuf,
        /// The I/O error.
        message: String,
    },
    /// The declared shared directory cannot be used.
    SharedDir(SharedDirError),
    /// The function's imports cannot be loaded.
    #[cfg(feature = "runtime-deno")]
    Graph {
        /// The function.
        name:  String,
        /// Why.
        error: graph::GraphError,
    },
    /// A Deno module, in a build without the Deno runtime.
    DenoNotCompiled {
        /// The function.
        name: String,
    },
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoModule {
                name,
                runtime,
                pattern,
            } => write!(
                f,
                "function {name:?} declares the {runtime:?} runtime but no module file was found \
                 at {pattern}"
            ),
            Self::Read {
                name,
                path,
                message,
            } => write!(
                f,
                "failed to read the module for function {name:?} at {}: {message}",
                path.display()
            ),
            Self::SharedDir(error) => write!(f, "{error}"),
            #[cfg(feature = "runtime-deno")]
            Self::Graph { name, error } => {
                write!(f, "function {name:?} cannot load its shared modules: {error}")
            },
            Self::DenoNotCompiled { name } => write!(
                f,
                "function {name:?} is a Deno module, but the Deno runtime is not compiled into \
                 this build"
            ),
        }
    }
}

impl std::error::Error for LoadError {}

/// Load the module named `name` for `runtime` from `config`'s directories.
///
/// `config` carries the directories as the deployment resolved them
/// ([`apply_env_overrides`](super::dirs::apply_env_overrides)).
///
/// A Deno function's shared modules are read and transpiled here, once. Its code is
/// transpiled here too, only to find its imports: [`FunctionModule::bytecode`] keeps
/// the source, which the runtime prepares as its configuration says. A function whose
/// code does not parse loads with no shared modules, and its invocation reports the
/// syntax error, as it did before functions could import.
///
/// # Errors
///
/// A [`LoadError`] naming the function and what is wrong.
pub fn load_function_module(
    config: &FunctionsConfig,
    name: &str,
    runtime: RuntimeType,
) -> Result<FunctionModule, LoadError> {
    let Some(path) = runtime.resolve_module_path(&config.module_dir, name) else {
        return Err(LoadError::NoModule {
            name: name.to_string(),
            runtime,
            pattern: runtime.module_path_pattern(&config.module_dir, name),
        });
    };
    let read_error = |error: std::io::Error| LoadError::Read {
        name:    name.to_string(),
        path:    path.clone(),
        message: error.to_string(),
    };
    match runtime {
        RuntimeType::Wasm => {
            let bytecode = std::fs::read(&path).map_err(read_error)?;
            Ok(FunctionModule::from_bytecode(name.to_string(), bytecode.into()))
        },
        RuntimeType::Deno => {
            let source = std::fs::read_to_string(&path).map_err(read_error)?;
            let shared_root = config
                .shared_dir
                .as_deref()
                .map(|shared_dir| check_shared_dir(&config.module_dir, shared_dir))
                .transpose()
                .map_err(LoadError::SharedDir)?;
            load_deno(name, source, shared_root.as_deref())
        },
    }
}

#[cfg(feature = "runtime-deno")]
fn load_deno(
    name: &str,
    source: String,
    shared_root: Option<&std::path::Path>,
) -> Result<FunctionModule, LoadError> {
    let graph =
        match crate::runtime::deno::transpile::transpile_typescript(&source) {
            Ok(code) => graph::collect(&format!("function {name:?}"), &code, shared_root).map_err(
                |error| LoadError::Graph {
                    name: name.to_string(),
                    error,
                },
            )?,
            Err(_) => graph::ModuleGraph::default(),
        };
    let sources: std::collections::BTreeMap<_, _> = graph
        .modules
        .iter()
        .map(|(key, module)| (key.clone(), module.source.as_str()))
        .collect();
    let source_hash = function_hash(source.as_bytes(), &sources);
    let shared = shared_root.map(|_| {
        std::sync::Arc::new(
            graph.modules.into_iter().map(|(key, module)| (key, module.code)).collect(),
        )
    });
    Ok(FunctionModule {
        name: name.to_string(),
        source_hash,
        bytecode: source.into(),
        runtime: RuntimeType::Deno,
        shared,
    })
}

#[cfg(not(feature = "runtime-deno"))]
fn load_deno(
    name: &str,
    _source: String,
    _shared_root: Option<&std::path::Path>,
) -> Result<FunctionModule, LoadError> {
    Err(LoadError::DenoNotCompiled {
        name: name.to_string(),
    })
}

#[cfg(test)]
#[path = "load_tests.rs"]
mod tests;

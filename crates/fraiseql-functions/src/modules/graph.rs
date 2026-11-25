//! A function's module graph: every shared module it imports, read once, at load time.
//!
//! The walk starts at the function's code and follows each import it finds: static
//! `import`, `export … from`, and `import()` with a string literal. It reads the code
//! the isolate will run, after type-stripping, so an import the transpiler erases
//! (`import type`, or one used only as a type) is never followed: it never reaches
//! the runtime. Each shared module is transpiled here, once. Every
//! specifier goes through [`resolve_specifier`] and then [`locate`]; the first refusal
//! stops the load and names the module that wrote the import. Each module is read once:
//! a key already in the graph is not followed again, so an import cycle ends.
//!
//! An `import()` whose argument is computed cannot be followed here. It is not
//! refused at load, because it may name a module the graph already holds; inside the
//! isolate it resolves through the same lexical rule and finds only what this walk read.
//!
//! Limits bound what a load reads: [`MAX_MODULE_BYTES`] per shared module,
//! [`MAX_GRAPH_BYTES`] for all of them together, [`MAX_MODULES`] of them. The
//! function's own file is not counted: it was loaded without a limit before shared
//! modules existed, and a bundled function can be larger than one shared module.

use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    io::Read,
    path::{Path, PathBuf},
};

use deno_ast::{
    MediaType, ModuleSpecifier, ParseParams,
    swc::{
        ast::{Callee, Expr, Lit, ModuleDecl, Str},
        ecma_visit::{Visit, VisitWith},
    },
};

use super::resolve::{ModuleKey, Refusal, locate, resolve_specifier};
use crate::runtime::deno::transpile::transpile_module;

/// The most one shared module may hold, in bytes: one mebibyte.
pub const MAX_MODULE_BYTES: u64 = 1024 * 1024;

/// The most all of a function's shared modules may hold together, in bytes: eight
/// mebibytes.
pub const MAX_GRAPH_BYTES: u64 = 8 * 1024 * 1024;

/// The most shared modules one function may import, directly or not.
pub const MAX_MODULES: usize = 256;

/// The shared modules a function imports, by key, as the source text read at load.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModuleGraph {
    /// Every shared module reached from the function, directly or not.
    pub modules: BTreeMap<ModuleKey, GraphModule>,
}

/// One shared module as its load read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphModule {
    /// The file's text, which the function's integrity hash covers.
    pub source: String,
    /// The `JavaScript` the isolate runs: the source with `TypeScript` types stripped.
    pub code:   String,
}

/// Why a function's module graph cannot be loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum GraphError {
    /// An import broke the import rule.
    Refused {
        /// The module that wrote the import: the function, or a shared module.
        importer:  String,
        /// The specifier as written.
        specifier: String,
        /// Which rule it broke.
        refusal:   Refusal,
    },
    /// A shared module does not parse.
    Parse {
        /// The module.
        module:  String,
        /// The parser's diagnostic.
        message: String,
    },
    /// A shared module is larger than [`MAX_MODULE_BYTES`].
    ModuleTooLarge {
        /// The module.
        module: ModuleKey,
        /// The limit, in bytes.
        limit:  u64,
    },
    /// The shared modules together are larger than [`MAX_GRAPH_BYTES`].
    GraphTooLarge {
        /// The limit, in bytes.
        limit: u64,
    },
    /// The function imports more than [`MAX_MODULES`] shared modules.
    TooManyModules {
        /// The limit.
        limit: usize,
    },
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused {
                importer,
                specifier,
                refusal,
            } => write!(f, "{importer} imports {specifier:?}: {refusal}"),
            Self::Parse { module, message } => write!(f, "{module} does not parse: {message}"),
            Self::ModuleTooLarge { module, limit } => write!(
                f,
                "shared module {module} is larger than the {limit}-byte limit for one module"
            ),
            Self::GraphTooLarge { limit } => {
                write!(f, "the function's shared modules together exceed the {limit}-byte limit")
            },
            Self::TooManyModules { limit } => {
                write!(f, "the function imports more than the limit of {limit} shared modules")
            },
        }
    }
}

impl std::error::Error for GraphError {}

/// Walk the imports of `entry_code` and read every shared module it reaches.
///
/// `entry_code` is the function's code as the isolate runs it (type-stripped).
/// `entry_name` names the function in a refusal. `shared_root` is the canonical shared
/// directory ([`check_shared_dir`](super::check_shared_dir)); `None` means none is
/// declared, and any import is refused.
///
/// # Errors
///
/// The first [`GraphError`] the walk meets.
pub fn collect(
    entry_name: &str,
    entry_code: &str,
    shared_root: Option<&Path>,
) -> Result<ModuleGraph, GraphError> {
    let mut walk = Walk {
        shared_root,
        graph: ModuleGraph::default(),
        bytes: 0,
        queue: VecDeque::new(),
        read: HashMap::new(),
    };
    for import in imports(entry_name, entry_code)? {
        walk.follow(entry_name, &import)?;
    }
    while let Some(key) = walk.queue.pop_front() {
        let code = walk.graph.modules[&key].code.clone();
        let importer = key.to_string();
        for import in imports(&importer, &code)? {
            walk.follow(&importer, &import)?;
        }
    }
    Ok(walk.graph)
}

struct Walk<'a> {
    shared_root: Option<&'a Path>,
    graph:       ModuleGraph,
    bytes:       u64,
    queue:       VecDeque<ModuleKey>,
    /// Each file read so far, by canonical path: two keys reaching one file through
    /// a symlink inside the shared directory read and count it once.
    read:        HashMap<PathBuf, GraphModule>,
}

impl Walk<'_> {
    /// Resolve one import written by `importer`, reading the module if it is new.
    fn follow(&mut self, importer: &str, import: &Import) -> Result<(), GraphError> {
        let refused = |refusal: Refusal| GraphError::Refused {
            importer: importer.to_string(),
            specifier: import.specifier.clone(),
            refusal,
        };
        let Some(shared_root) = self.shared_root else {
            return Err(refused(Refusal::NoSharedDir));
        };
        if let Some(refusal) = import.refusal.clone() {
            return Err(refused(refusal));
        }
        let key = resolve_specifier(&import.specifier).map_err(refused)?;
        if self.graph.modules.contains_key(&key) {
            return Ok(());
        }
        if self.graph.modules.len() >= MAX_MODULES {
            return Err(GraphError::TooManyModules { limit: MAX_MODULES });
        }
        let path = locate(shared_root, &key).map_err(refused)?;
        let module = if let Some(module) = self.read.get(&path) {
            module.clone()
        } else {
            let source = read_module(&path).map_err(|error| match error {
                ReadError::Refused(refusal) => refused(refusal),
                ReadError::TooLarge => GraphError::ModuleTooLarge {
                    module: key.clone(),
                    limit:  MAX_MODULE_BYTES,
                },
            })?;
            self.bytes += source.len() as u64;
            if self.bytes > MAX_GRAPH_BYTES {
                return Err(GraphError::GraphTooLarge {
                    limit: MAX_GRAPH_BYTES,
                });
            }
            let code =
                transpile_module(&source, &key.url()).map_err(|message| GraphError::Parse {
                    module: key.to_string(),
                    message,
                })?;
            let module = GraphModule { source, code };
            self.read.insert(path, module.clone());
            module
        };
        self.graph.modules.insert(key.clone(), module);
        self.queue.push_back(key);
        Ok(())
    }
}

enum ReadError {
    Refused(Refusal),
    TooLarge,
}

/// Read a located module as UTF-8, never more than one byte past the limit.
fn read_module(path: &Path) -> Result<String, ReadError> {
    let file = std::fs::File::open(path).map_err(|_| ReadError::Refused(Refusal::Missing))?;
    let mut bytes = Vec::new();
    file.take(MAX_MODULE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ReadError::Refused(Refusal::NotAFile))?;
    if bytes.len() as u64 > MAX_MODULE_BYTES {
        return Err(ReadError::TooLarge);
    }
    String::from_utf8(bytes).map_err(|_| ReadError::Refused(Refusal::NotUtf8))
}

/// One import found in a module: the specifier as written, and a refusal the import's
/// form already earns (attributes, or a specifier that is not a valid string).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Import {
    pub(crate) specifier: String,
    pub(crate) refusal:   Option<Refusal>,
}

/// Every import `code` writes, in source order.
///
/// Parsed as `TypeScript`, a superset of the `JavaScript` the transpiler emits, so the
/// scan also reads untranspiled source the same way.
pub(crate) fn imports(module: &str, code: &str) -> Result<Vec<Import>, GraphError> {
    let parse_error = |message: String| GraphError::Parse {
        module: module.to_string(),
        message,
    };
    let specifier = ModuleSpecifier::parse("file:///fraiseql/import-scan.ts")
        .map_err(|error| parse_error(error.to_string()))?;
    let parsed = deno_ast::parse_module(ParseParams {
        specifier,
        text: code.into(),
        media_type: MediaType::TypeScript,
        capture_tokens: false,
        scope_analysis: false,
        maybe_syntax: None,
    })
    .map_err(|diagnostic| parse_error(diagnostic.to_string()))?;
    let mut collector = Collector::default();
    parsed.program_ref().unwrap_module().visit_with(&mut collector);
    Ok(collector.found)
}

#[derive(Default)]
struct Collector {
    found: Vec<Import>,
}

impl Collector {
    fn push(&mut self, source: &Str, has_attributes: bool) {
        let (specifier, refusal) = match source.value.as_atom() {
            Some(atom) => (atom.to_string(), None),
            None => (source.value.to_atom_lossy().to_string(), Some(Refusal::NotAString)),
        };
        let refusal = refusal.or_else(|| has_attributes.then_some(Refusal::ImportAttributes));
        self.found.push(Import { specifier, refusal });
    }
}

impl Visit for Collector {
    fn visit_module_decl(&mut self, node: &ModuleDecl) {
        match node {
            ModuleDecl::Import(import) => self.push(&import.src, import.with.is_some()),
            ModuleDecl::ExportAll(export) => self.push(&export.src, export.with.is_some()),
            ModuleDecl::ExportNamed(export) => {
                if let Some(source) = &export.src {
                    self.push(source, export.with.is_some());
                }
            },
            _ => {},
        }
        node.visit_children_with(self);
    }

    fn visit_call_expr(&mut self, node: &deno_ast::swc::ast::CallExpr) {
        if matches!(node.callee, Callee::Import(_)) {
            if let Some(argument) = node.args.first().filter(|argument| argument.spread.is_none()) {
                let has_options = node.args.len() > 1;
                match &*argument.expr {
                    Expr::Lit(Lit::Str(source)) => self.push(source, has_options),
                    Expr::Tpl(template) if template.exprs.is_empty() => {
                        if let Some(cooked) =
                            template.quasis.first().and_then(|q| q.cooked.as_ref())
                        {
                            self.push(
                                &Str {
                                    span:  template.span,
                                    value: cooked.clone(),
                                    raw:   None,
                                },
                                has_options,
                            );
                        }
                    },
                    _ => {},
                }
            }
        }
        node.visit_children_with(self);
    }
}

#[cfg(test)]
#[path = "graph_tests.rs"]
mod tests;

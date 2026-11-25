//! The isolate's module loader: the function, and the shared modules read at load.
//!
//! A function runs as an ES module. The isolate's main module is a short driver that
//! imports the function's default export and calls it; the function's own source is
//! loaded as a module of its own, unchanged. Everything the isolate can import is in
//! memory, captured when the function was loaded: no import reaches the filesystem or
//! the network at run time.
//!
//! Resolution goes through [`resolve_specifier`], the same lexical rule the load-time
//! graph applies, followed by a lookup in the preloaded map. A specifier the rule
//! accepts but the map does not hold, such as a computed `import()` naming a module the
//! function never imports literally, is refused. Every refusal carries
//! [`PERMANENT_ERROR_MARKER`], so a bad import dead-letters instead of being retried.

use std::sync::Arc;

use deno_core::{
    ModuleLoadResponse, ModuleLoader, ModuleSource, ModuleSourceCode, ModuleSpecifier, ModuleType,
    RequestedModuleType, ResolutionKind,
    anyhow::{self, anyhow},
};

pub use crate::modules::SharedModules;
use crate::{
    modules::resolve::{ModuleKey, Refusal, resolve_specifier},
    types::PERMANENT_ERROR_MARKER,
};

/// The isolate's main module: the driver that calls the function.
pub(super) const DRIVER_URL: &str = "file:///fraiseql/main.js";

/// The function's own module. The transpiler's inline source map names the same file,
/// so a stack trace points into the author's source.
pub(super) const FUNCTION_URL: &str = "file:///fraiseql-function.ts";

/// Serves the function and its shared modules from memory.
pub(super) struct FunctionModuleLoader {
    /// The function's `JavaScript`.
    pub(super) function: String,
    /// Its shared modules; `None` when no shared directory is declared.
    pub(super) shared:   Option<Arc<SharedModules>>,
}

impl FunctionModuleLoader {
    /// A refused import, naming the module that wrote it.
    fn refused(referrer: &str, specifier: &str, reason: &dyn std::fmt::Display) -> anyhow::Error {
        let importer = if referrer == FUNCTION_URL {
            "the function".to_string()
        } else {
            ModuleKey::from_url(referrer)
                .map_or_else(|| referrer.to_string(), |key| key.to_string())
        };
        anyhow!("{PERMANENT_ERROR_MARKER} {importer} imports {specifier:?}: {reason}")
    }
}

impl ModuleLoader for FunctionModuleLoader {
    fn resolve(
        &self,
        specifier: &str,
        referrer: &str,
        kind: ResolutionKind,
    ) -> Result<ModuleSpecifier, anyhow::Error> {
        // The driver is the main module, given as code; it resolves to itself.
        if matches!(kind, ResolutionKind::MainModule) && specifier == DRIVER_URL {
            return Ok(ModuleSpecifier::parse(DRIVER_URL)?);
        }
        // Only the driver reaches the function's module, by its exact URL.
        if referrer == DRIVER_URL && specifier == FUNCTION_URL {
            return Ok(ModuleSpecifier::parse(FUNCTION_URL)?);
        }
        let Some(shared) = &self.shared else {
            return Err(Self::refused(referrer, specifier, &Refusal::NoSharedDir));
        };
        let key = resolve_specifier(specifier)
            .map_err(|refusal| Self::refused(referrer, specifier, &refusal))?;
        if !shared.contains_key(&key) {
            return Err(Self::refused(
                referrer,
                specifier,
                &"it was not loaded with the function: only a module the function's code imports \
                  by a literal specifier is loaded, so a computed `import()` reaches only those",
            ));
        }
        Ok(ModuleSpecifier::parse(&key.url())?)
    }

    fn load(
        &self,
        module_specifier: &ModuleSpecifier,
        _maybe_referrer: Option<&ModuleSpecifier>,
        _is_dyn_import: bool,
        requested_module_type: RequestedModuleType,
    ) -> ModuleLoadResponse {
        let url = module_specifier.as_str();
        let code = if !matches!(requested_module_type, RequestedModuleType::None) {
            Err(Self::refused(url, url, &Refusal::ImportAttributes))
        } else if url == FUNCTION_URL {
            Ok(self.function.clone())
        } else {
            ModuleKey::from_url(url)
                .and_then(|key| self.shared.as_ref()?.get(&key).cloned())
                .ok_or_else(|| Self::refused(url, url, &"no such module was loaded"))
        };
        ModuleLoadResponse::Sync(code.map(|code| {
            ModuleSource::new(
                ModuleType::JavaScript,
                ModuleSourceCode::String(code.into()),
                module_specifier,
                None,
            )
        }))
    }
}

/// The driver: import the function's default export, call it with the event, and leave
/// the result or the error in `globalThis` for the executor to read.
///
/// The call is an `async` IIFE the module does not await, so module evaluation ends at
/// the function's first `await` and the event loop drives the rest, as for a script.
pub(super) fn driver(event_json: &str) -> String {
    // Injected so a guest can mark a thrown error permanent structurally
    // (`throw Object.assign(new Error(msg), { fraiseqlPermanent: true })`) and the
    // runtime dead-letters it immediately. Host-op errors already carry the marker
    // in their message; this folds the property form into the same signal.
    format!(
        r#"import __fn from "{FUNCTION_URL}";
(async () => {{
    try {{
        const __event = {event_json};
        const __result = await __fn(__event);
        globalThis.__fraiseql_result = JSON.stringify(__result);
        globalThis.__fraiseql_error = null;
    }} catch (e) {{
        globalThis.__fraiseql_result = null;
        const __permanent = e && e.fraiseqlPermanent === true;
        globalThis.__fraiseql_error = (__permanent ? "{PERMANENT_ERROR_MARKER} " : "") + String(e);
    }}
}})();
"#
    )
}

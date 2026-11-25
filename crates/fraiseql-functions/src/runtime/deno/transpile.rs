//! Real `TypeScript` type-stripping for guest functions.
//!
//! `deno_ast` (`swc` underneath) parses the guest as `TypeScript` and emits plain
//! `JavaScript` with the `TypeScript`-only constructs removed or lowered, so an
//! author can write ordinary `.ts` — `interface`, `: Type`, generics, `as`,
//! `enum`, parameter properties — instead of restricting themselves to the
//! type-annotation-free subset that V8 accepts directly.
//!
//! This is a real `AST` transpile, not a string munge: `enum`s become runtime
//! objects, parameter properties expand to assignments, and the emitted code
//! carries an inline source map so a runtime stack trace still points back into
//! the author's original source. It is only reached when
//! [`DenoConfig::enable_typescript`](super::DenoConfig::enable_typescript) is
//! set (the default); with it off the `JavaScript` is executed byte-for-byte
//! unchanged.

use deno_ast::{
    EmitOptions, MediaType, ModuleSpecifier, ParseParams, SourceMapOption, TranspileModuleOptions,
    TranspileOptions,
    swc::ast::{ExportSpecifier, ModuleDecl, ModuleExportName, ModuleItem},
};

/// The synthetic specifier the guest source is parsed under. It surfaces in
/// syntax-error messages and the inline source map as
/// `file:///fraiseql-function.ts:<line>:<col>`.
const GUEST_SPECIFIER: &str = "file:///fraiseql-function.ts";

/// Strip `TypeScript` types from `source`, returning executable `JavaScript`.
///
/// The returned `JavaScript` preserves ES module syntax (an author's
/// `export default` survives, so the wrapper can still find the entry point)
/// and carries an inline source map back to the original source.
///
/// # Errors
///
/// Returns a `SyntaxError: …`-prefixed string — including the offending line and
/// column — when the source does not parse or cannot be transpiled. The prefix
/// matches the classification the executor already applies to malformed guests,
/// so an untranspilable function dead-letters as a permanent 4xx rather than
/// being retried.
pub fn transpile_typescript(source: &str) -> Result<String, String> {
    transpile_module(source, GUEST_SPECIFIER)
}

/// [`transpile_typescript`] for the module at `url`: a shared module is parsed under its
/// own URL, so a diagnostic and its source map name that module, not the function.
///
/// # Errors
///
/// As [`transpile_typescript`].
pub fn transpile_module(source: &str, url: &str) -> Result<String, String> {
    let specifier = ModuleSpecifier::parse(url)
        .map_err(|e| format!("SyntaxError: invalid module specifier: {e}"))?;

    let parsed = deno_ast::parse_module(ParseParams {
        specifier,
        text: source.into(),
        media_type: MediaType::TypeScript,
        capture_tokens: false,
        scope_analysis: false,
        maybe_syntax: None,
    })
    // `ParseDiagnostic`'s `Display` is already `SyntaxError: <message>` with the
    // `<specifier>:<line>:<col>` location, so it is used verbatim.
    .map_err(|diagnostic| diagnostic.to_string())?;

    let emitted = parsed
        .transpile(
            &TranspileOptions::default(),
            &TranspileModuleOptions::default(),
            &EmitOptions {
                source_map: SourceMapOption::Inline,
                ..Default::default()
            },
        )
        .map_err(|e| format!("SyntaxError: TypeScript transpile failed: {e}"))?
        .into_source();

    Ok(emitted.text)
}

/// Check that the function module has a default export: the function the driver calls.
///
/// Any form counts: `export default <expression>`, `export default function …`, and an
/// export list naming one, `export { handler as default }` (how esbuild's
/// `--format=esm` output names a bundle's entry point), including one that re-exports a
/// shared module's (`export { default } from "@shared/handler.ts"`). An `export * from`
/// does not: it never re-exports a default.
///
/// Found by parsing the `JavaScript` the isolate will run, so the words `export default`
/// in a comment or a string literal are not mistaken for the entry point.
///
/// # Errors
///
/// Returns a `SyntaxError: …`-prefixed string when the source does not parse or has no
/// default export: either way there is no function to call.
pub fn default_export(source: &str) -> Result<(), String> {
    let specifier = ModuleSpecifier::parse(GUEST_SPECIFIER)
        .map_err(|e| format!("SyntaxError: invalid guest specifier: {e}"))?;

    let parsed = deno_ast::parse_module(ParseParams {
        specifier,
        text: source.into(),
        media_type: MediaType::JavaScript,
        capture_tokens: false,
        scope_analysis: false,
        maybe_syntax: None,
    })
    .map_err(|diagnostic| diagnostic.to_string())?;

    let exports_default = |name: &ModuleExportName| matches!(name, ModuleExportName::Ident(ident) if &*ident.sym == "default");
    let has_default = parsed.program_ref().unwrap_module().body.iter().any(|item| match item {
        ModuleItem::ModuleDecl(
            ModuleDecl::ExportDefaultExpr(_) | ModuleDecl::ExportDefaultDecl(_),
        ) => true,
        ModuleItem::ModuleDecl(ModuleDecl::ExportNamed(export)) => {
            export.specifiers.iter().any(|specifier| match specifier {
                ExportSpecifier::Named(named) => named
                    .exported
                    .as_ref()
                    .map_or_else(|| exports_default(&named.orig), exports_default),
                ExportSpecifier::Default(_) | ExportSpecifier::Namespace(_) => false,
            })
        },
        _ => false,
    });
    if has_default {
        Ok(())
    } else {
        Err(
            "SyntaxError: the function module has no default export, so there is no function \
             to call. Export it with `export default`."
                .to_string(),
        )
    }
}

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

use std::ops::Range;

use deno_ast::{
    EmitOptions, MediaType, ModuleSpecifier, ParseParams, SourceMapOption, SourceRangedForSpanned,
    TranspileModuleOptions, TranspileOptions,
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
    let specifier = ModuleSpecifier::parse(GUEST_SPECIFIER)
        .map_err(|e| format!("SyntaxError: invalid guest specifier: {e}"))?;

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

/// Where the module's `export default` item is, and the expression or declaration it
/// exports, as byte ranges of `source`.
///
/// An export list naming a local binding as the default, `export { handler as default };`,
/// counts too: it is how esbuild's `--format=esm` output names a bundle's entry point.
/// One that re-exports another module's (`... from "./x"`) does not, as there is no module
/// loader to reach it.
///
/// Found by parsing the `JavaScript` the isolate will run, so the words `export default`
/// in a comment or a string literal are not mistaken for the entry point.
///
/// # Errors
///
/// Returns a `SyntaxError: …`-prefixed string when the source does not parse or has no
/// default export: either way there is no function to call.
pub fn default_export(source: &str) -> Result<(Range<usize>, Range<usize>), String> {
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

    let start = parsed.text_info_lazy().range().start;
    parsed
        .program_ref()
        .unwrap_module()
        .body
        .iter()
        .find_map(|item| match item {
            ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultExpr(export)) => Some((
                export.range().as_byte_range(start),
                export.expr.range().as_byte_range(start),
            )),
            ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultDecl(export)) => Some((
                export.range().as_byte_range(start),
                export.decl.range().as_byte_range(start),
            )),
            ModuleItem::ModuleDecl(ModuleDecl::ExportNamed(export)) if export.src.is_none() => {
                export.specifiers.iter().find_map(|specifier| match specifier {
                    ExportSpecifier::Named(named)
                        if matches!(&named.exported, Some(ModuleExportName::Ident(name)) if &*name.sym == "default") =>
                    {
                        Some((
                            export.range().as_byte_range(start),
                            named.orig.range().as_byte_range(start),
                        ))
                    },
                    _ => None,
                })
            },
            _ => None,
        })
        .ok_or_else(|| {
            "SyntaxError: the function module has no default export, so there is no function \
             to call. Export it with `export default`."
                .to_string()
        })
}

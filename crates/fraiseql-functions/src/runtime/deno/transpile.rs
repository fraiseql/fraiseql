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

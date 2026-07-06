//! Tests for real `TypeScript` type-stripping ([`super::transpile::transpile_typescript`]).

use super::transpile::transpile_typescript;

#[test]
fn strips_scalar_type_annotations() {
    let js = transpile_typescript("const n: number = 41 + 1;\nconst s: string = `x`;\n")
        .expect("plain annotated TS transpiles");
    // The runtime values survive; the `: number` / `: string` annotations do not.
    assert!(js.contains("41 + 1"), "value expression preserved: {js}");
    assert!(!js.contains(": number"), "number annotation stripped: {js}");
    assert!(!js.contains(": string"), "string annotation stripped: {js}");
}

#[test]
fn strips_interfaces_generics_and_as() {
    let src = r"
interface User { id: number; name: string }
function pick<T>(xs: T[], i: number): T { return xs[i]; }
const u = { id: 1, name: 'a' } as User;
export default async (event: { ids: number[] }): Promise<User> =>
    pick<User>([u], event.ids.length - event.ids.length);
";
    let js = transpile_typescript(src).expect("interfaces/generics/as transpile");
    assert!(!js.contains("interface"), "interface declaration erased: {js}");
    assert!(!js.contains(": T[]"), "generic param annotation erased: {js}");
    assert!(!js.contains("as User"), "`as` assertion erased: {js}");
    // The value-level identifiers remain.
    assert!(js.contains("pick"), "function name preserved: {js}");
}

#[test]
fn preserves_export_default_for_the_wrapper() {
    // The executor's wrapper finds the entry point by rewriting `export default`,
    // so type-stripping must not drop or rename it.
    let js = transpile_typescript(
        "export default async function handler(e: { n: number }): Promise<number> { return e.n; }",
    )
    .expect("annotated default export transpiles");
    assert!(js.contains("export default"), "export default kept: {js}");
}

#[test]
fn enum_is_lowered_to_runtime_object() {
    // A TS `enum` is not erasable — it must become a real runtime binding. This
    // is what distinguishes a real transpile from naive type-stripping.
    let js = transpile_typescript("enum Color { Red, Green }\nexport default () => Color.Green;")
        .expect("enum transpiles");
    assert!(!js.contains("enum Color"), "enum keyword lowered away: {js}");
    assert!(js.contains("Color"), "enum binding still referenced at runtime: {js}");
    assert!(js.contains("Green"), "enum member preserved: {js}");
}

#[test]
fn plain_javascript_round_trips() {
    // Valid JS is valid TS: transpiling it is a semantic identity (swc may
    // reformat, so we assert on tokens, not bytes).
    let js = transpile_typescript(
        "export default async (event) => {\n  return { doubled: event.n * 2 };\n};",
    )
    .expect("plain JS transpiles");
    assert!(js.contains("export default"), "export default kept: {js}");
    assert!(js.contains("event.n * 2"), "logic preserved: {js}");
}

#[test]
fn flagship_annotated_example_transpiles() {
    // The shipped annotated example must stay valid strippable TypeScript — this
    // guards it (triple-slash reference directive, interfaces, union type, `as`)
    // against rotting into something the runtime would reject.
    let path =
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/native-functions/deal-scoring.ts");
    let src = std::fs::read_to_string(path).expect("read deal-scoring.ts example");
    let js = transpile_typescript(&src).expect("annotated example transpiles");
    assert!(js.contains("export default"), "entry point preserved: {js}");
    assert!(!js.contains("interface Deal"), "types stripped: {js}");
}

#[test]
fn syntax_error_is_reported_with_location() {
    // A genuine parse failure must surface as a located SyntaxError so the
    // executor classifies it as a permanent 4xx (dead-letter, never retry).
    let err = transpile_typescript("const x: = ;").expect_err("malformed source is rejected");
    assert!(err.starts_with("SyntaxError"), "SyntaxError-prefixed: {err}");
    assert!(
        err.contains("fraiseql-function.ts:"),
        "carries the specifier:line:col location: {err}"
    );
}

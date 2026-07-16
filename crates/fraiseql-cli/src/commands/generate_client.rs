//! `fraiseql generate-client` — generate consumer-side typed clients from a
//! compiled schema (`schema.compiled.json`).
//!
//! This is distinct from `fraiseql generate`, which emits server-side **authoring**
//! code (FraiseQL type definitions in another language, fed back into the
//! compiler). `generate-client` consumes the compiler's *output* to build a client
//! that *calls* the API. The generation itself lives in the `fraiseql-codegen`
//! crate; this command is the thin filesystem wrapper.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use fraiseql_core::schema::CompiledSchema;

/// Supported client target languages.
#[derive(Debug, Clone, Copy)]
pub enum ClientLanguage {
    /// TypeScript client (interfaces + typed query/mutation functions).
    TypeScript,
}

/// Run `generate-client <language>`.
///
/// `schema_path` is auto-detected from conventional locations when `None`.
/// Refuses to overwrite an existing generated tree unless `force` is set.
///
/// # Errors
///
/// Returns an error if the schema cannot be found, read, or parsed, if the output
/// directory already contains a generated client and `force` is not set, or if any
/// file cannot be written.
pub fn run(
    language: ClientLanguage,
    schema_path: Option<&Path>,
    out_dir: &Path,
    force: bool,
) -> Result<()> {
    let schema_path = match schema_path {
        Some(p) => p.to_path_buf(),
        None => auto_detect_compiled_schema()?,
    };

    let raw = std::fs::read_to_string(&schema_path)
        .with_context(|| format!("Failed to read compiled schema {}", schema_path.display()))?;
    let schema: CompiledSchema = serde_json::from_str(&raw).with_context(|| {
        format!("Failed to parse {} as a compiled schema", schema_path.display())
    })?;

    let mut files = match language {
        ClientLanguage::TypeScript => fraiseql_codegen::client::typescript::generate(&schema)
            .map_err(|e| anyhow::anyhow!("client generation failed: {e}"))?,
    };

    // `functions.d.ts` (phase 08): typed guest payloads + host-op declarations, when
    // the compiled schema declares functions. Parsed from the same raw JSON — the
    // `functions` section is not part of `CompiledSchema`.
    if matches!(language, ClientLanguage::TypeScript) {
        let specs = functions_type_specs(&raw, &schema);
        if !specs.is_empty() {
            let dts = fraiseql_codegen::client::typescript::generate_functions_dts(&schema, &specs)
                .map_err(|e| anyhow::anyhow!("functions.d.ts generation failed: {e}"))?;
            files.insert(std::path::PathBuf::from("functions.d.ts"), dts);
        }
    }

    if out_dir.exists() && !force && contains_generated_client(out_dir) {
        anyhow::bail!(
            "Output directory {} already contains a generated client. Pass --force to overwrite.",
            out_dir.display()
        );
    }

    for (rel_path, content) in &files {
        let full_path = out_dir.join(rel_path);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create {}", parent.display()))?;
        }
        std::fs::write(&full_path, content)
            .with_context(|| format!("Failed to write {}", full_path.display()))?;
        println!("  wrote {}", full_path.display());
    }

    println!(
        "\nGenerated {} files into {}. Type-check with `tsc --strict --noEmit {}/index.ts`.",
        files.len(),
        out_dir.display(),
        out_dir.display(),
    );
    Ok(())
}

/// Resolve the `functions.d.ts` type specs from the compiled schema's `functions`
/// section (parsed from the raw JSON — it is not part of [`CompiledSchema`]).
///
/// Each function's trigger determines its payload shape: `after:mutation` /
/// `after:capture` on entity `E` → `{ event_kind, old: E|null, new: E|null }` (typed
/// to `E` when it names a schema type, else `unknown`); `cron` → schedule context;
/// `after:ingest` → the inbound-message shape. Triggers with no author-facing payload
/// yet (`http`, `before:mutation`, `after:storage`) are skipped.
fn functions_type_specs(
    raw: &str,
    schema: &CompiledSchema,
) -> Vec<fraiseql_codegen::client::typescript::FunctionTypeSpec> {
    use fraiseql_codegen::client::typescript::{FunctionPayloadShape, FunctionTypeSpec};

    /// The subset of the `functions` section we need.
    #[derive(serde::Deserialize)]
    struct FunctionsLite {
        #[serde(default)]
        definitions: Vec<FunctionDefLite>,
    }
    #[derive(serde::Deserialize)]
    struct FunctionDefLite {
        name:    String,
        trigger: String,
    }

    let Some(functions) = serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|value| value.get("functions").cloned())
        .filter(|value| !value.is_null())
        .and_then(|value| serde_json::from_value::<FunctionsLite>(value).ok())
    else {
        return Vec::new();
    };

    let is_type = |name: &str| schema.types.iter().any(|ty| ty.name == name);
    let entity_shape = |parts: &[&str]| {
        // `after:{mutation,capture}:<Entity>[:op]` — the 3rd segment is the entity type.
        let entity = parts.get(2).filter(|name| is_type(name)).map(|name| (*name).to_string());
        FunctionPayloadShape::Entity { entity }
    };

    functions
        .definitions
        .into_iter()
        .filter_map(|def| {
            let parts: Vec<&str> = def.trigger.split(':').collect();
            let shape = match (parts.first().copied(), parts.get(1).copied()) {
                (Some("after"), Some("mutation" | "capture")) => entity_shape(&parts),
                (Some("cron"), _) => FunctionPayloadShape::Cron,
                (Some("after"), Some("ingest")) => FunctionPayloadShape::Ingest,
                // http / before:mutation / after:storage: no author-facing payload yet.
                _ => return None,
            };
            Some(FunctionTypeSpec {
                name: def.name,
                shape,
            })
        })
        .collect()
}

/// Whether a directory already holds a generated client (detected via the
/// auto-generated sentinel in `types.ts`).
fn contains_generated_client(out_dir: &Path) -> bool {
    let marker = out_dir.join("types.ts");
    std::fs::read_to_string(marker)
        .is_ok_and(|content| content.contains("AUTO-GENERATED by fraiseql-codegen"))
}

/// Search conventional locations for a `schema.compiled.json`.
fn auto_detect_compiled_schema() -> Result<PathBuf> {
    let candidates = [
        "schema.compiled.json",
        "target/fraiseql/schema.compiled.json",
        "build/schema.compiled.json",
    ];
    for candidate in candidates {
        let path = PathBuf::from(candidate);
        if path.exists() {
            return Ok(path);
        }
    }
    anyhow::bail!(
        "No compiled schema found. Compile first (`fraiseql compile`) or pass an explicit path: \
         fraiseql generate-client typescript --schema <path> --out <dir>"
    )
}

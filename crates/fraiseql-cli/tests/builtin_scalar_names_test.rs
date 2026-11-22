//! One reading of a built-in scalar's name, through the real `compile`.
//!
//! `FieldType::parse` is how the engine reads a type name it finds as a string, and
//! `compile` is what an author's name has to get through. If the two disagree, a name the
//! engine documents as a scalar is refused as undeclared, or one `compile` accepts means
//! something else to the engine. So every spelling in question goes through `compile` in
//! each position a type name takes, and must be accepted exactly when `FieldType::parse`
//! reads it as a scalar, as the type `FieldType::parse` gives it.
//!
//! **Execution engine:** none · **Infrastructure:** none · **Parallelism:** one test; it
//! changes the working directory.
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use fraiseql_cli::commands::compile::{CompileOptions, compile_to_schema};
use fraiseql_core::schema::{BUILTIN_SCALARS, CompiledSchema, FieldType, RICH_SCALARS};
use tempfile::TempDir;

/// The spellings in question: every built-in's name, in its own case and folded either
/// way, every alias the engine's parser has read, and a rich scalar in both cases.
fn spellings() -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for (name, _) in BUILTIN_SCALARS {
        names.extend([
            (*name).to_string(),
            name.to_lowercase(),
            name.to_uppercase(),
        ]);
    }
    names.extend(
        [
            "integer",
            "int8",
            "int64",
            "long",
            "double",
            "bool",
            "timestamp",
            "jsonb",
            "numeric",
            "bigdecimal",
            "bit_vector",
            "half_vector",
            "halfvec",
            "sparse_vector",
            "sparsevec",
            // A typo, which every position refuses.
            "Strng",
        ]
        .map(str::to_string),
    );
    for rich in RICH_SCALARS.iter().take(3) {
        names.extend([(*rich).to_string(), rich.to_lowercase()]);
    }
    names.sort();
    names.dedup();
    names
}

/// A position a type name takes in a schema.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Position {
    Field,
    ListItem,
    Argument,
    InputField,
}

impl Position {
    const ALL: [Self; 4] = [
        Self::Field,
        Self::ListItem,
        Self::Argument,
        Self::InputField,
    ];
}

impl std::fmt::Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Field => "field",
            Self::ListItem => "list item",
            Self::Argument => "argument",
            Self::InputField => "input field",
        })
    }
}

/// A schema naming `ty` in one `position`: an object field, a list item, a query argument
/// or an input field.
fn schema_naming(ty: &str, position: Position) -> String {
    let (field, argument, input) = match position {
        Position::Field => (ty.to_string(), "String".to_string(), "String".to_string()),
        Position::ListItem => (format!("[{ty}!]"), "String".to_string(), "String".to_string()),
        Position::Argument => ("String".to_string(), ty.to_string(), "String".to_string()),
        Position::InputField => ("String".to_string(), "String".to_string(), ty.to_string()),
    };
    // A vector field declares its dimensions, and a bit vector a bit metric.
    let field_vector = match FieldType::parse(&field) {
        FieldType::BitVector => {
            r#", "vector_config": {"dimensions": 3, "distance_metric": "hamming"}"#
        },
        ty if is_vector(&ty) => r#", "vector_config": {"dimensions": 3}"#,
        _ => "",
    };
    format!(
        r#"{{
  "types": [{{"name": "Probe", "sql_source": "v_probe", "fields": [
    {{"name": "id", "type": "ID", "nullable": false}},
    {{"name": "value", "type": "{field}", "nullable": true{field_vector}}}
  ]}}],
  "input_types": [{{"name": "ProbeInput", "fields": [
    {{"name": "value", "type": "{input}", "nullable": true}}
  ]}}],
  "queries": [{{"name": "probe", "return_type": "Probe", "returns_list": true,
    "sql_source": "v_probe", "nullable": false,
    "arguments": [{{"name": "value", "type": "{argument}", "nullable": true}}]}}]
}}"#
    )
}

/// The type `compile` gave the name in `position`, or why it refused it.
async fn compiled_type(ty: &str, position: Position) -> Result<FieldType, String> {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("schema.json"), schema_naming(ty, position)).unwrap();
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    let result = compile_to_schema(CompileOptions::new("schema.json")).await;
    std::env::set_current_dir(original).unwrap();
    let schema: CompiledSchema = result.map_err(|e| format!("{e:#}"))?.0.schema;
    let probe = || schema.types.iter().find(|t| t.name == "Probe").unwrap();
    let value = || probe().fields.iter().find(|f| f.name == "value").unwrap().field_type.clone();
    Ok(match position {
        Position::Field => value(),
        Position::ListItem => match value() {
            FieldType::List { item, .. } => *item,
            other => return Err(format!("not compiled as a list: {other:?}")),
        },
        Position::Argument => schema.queries[0].arguments[0].arg_type.clone(),
        Position::InputField => {
            let input = schema.input_types.iter().find(|t| t.name == "ProbeInput").unwrap();
            FieldType::parse(&input.fields[0].field_type)
        },
    })
}

const fn is_vector(ty: &FieldType) -> bool {
    matches!(
        ty,
        FieldType::Vector | FieldType::BitVector | FieldType::HalfVector | FieldType::SparseVector
    )
}

/// A name the engine reads as no scalar is a reference to something the author declares.
const fn is_scalar(ty: &FieldType) -> bool {
    !matches!(
        ty,
        FieldType::Object(_)
            | FieldType::Enum(_)
            | FieldType::Input(_)
            | FieldType::Interface(_)
            | FieldType::Union(_)
            | FieldType::List { .. }
    )
}

#[tokio::test]
async fn compile_accepts_a_type_name_exactly_when_the_engine_reads_it_as_a_scalar() {
    let mut disagreements = Vec::new();
    for name in spellings() {
        let engine = FieldType::parse(&name);
        for position in Position::ALL {
            // A vector is a column's type, never a list's item.
            if position == Position::ListItem && is_vector(&engine) {
                continue;
            }
            let compiled = compiled_type(&name, position).await;
            match (is_scalar(&engine), compiled) {
                (true, Ok(ty)) if ty == engine => {},
                (false, Err(why))
                    if why.contains("not a declared type")
                        || why.contains("references unknown type") => {},
                (true, other) => disagreements.push(format!(
                    "`{name}` as {position}: the engine reads {engine:?}, compile gave {other:?}"
                )),
                (false, other) => disagreements.push(format!(
                    "`{name}` as {position}: the engine reads no scalar, compile gave {other:?}"
                )),
            }
        }
    }
    assert!(disagreements.is_empty(), "{}", disagreements.join("\n"));
}

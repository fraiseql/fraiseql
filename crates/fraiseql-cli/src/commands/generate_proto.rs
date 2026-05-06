//! `generate-proto` command: produce service.proto, vr_migrations.sql, and descriptor.binpb.

use std::{fs, path::Path};

use anyhow::Context;
use fraiseql_core::{
    db::dialect::{MySqlDialect, PostgresDialect, SqlDialect, SqlServerDialect, SqliteDialect},
    schema::CompiledSchema,
};

use crate::{
    codegen::{proto_gen, row_views},
    output::OutputFormatter,
};

/// Resolve a SQL dialect from its CLI string name.
///
/// # Errors
///
/// Returns an error if the dialect name is not recognised.
pub(crate) fn resolve_dialect(name: &str) -> anyhow::Result<Box<dyn SqlDialect>> {
    match name {
        "postgres" | "postgresql" => Ok(Box::new(PostgresDialect)),
        "mysql" => Ok(Box::new(MySqlDialect)),
        "sqlite" => Ok(Box::new(SqliteDialect)),
        "sqlserver" => Ok(Box::new(SqlServerDialect)),
        other => Err(anyhow::anyhow!(
            "Unknown dialect '{other}'. Expected: postgres, mysql, sqlite, sqlserver"
        )),
    }
}

/// Build a serialized `FileDescriptorSet` from the generated proto source.
///
/// Constructs a [`prost_types::FileDescriptorProto`] with package, syntax,
/// and dependency information, then encodes it into a binary protobuf that
/// gRPC reflection servers can serve at runtime.
///
/// # Errors
///
/// Returns an error if protobuf encoding fails.
pub(crate) fn build_file_descriptor_set(proto_source: &str, package: &str) -> anyhow::Result<Vec<u8>> {
    use prost::Message;
    use prost_types::{FileDescriptorProto, FileDescriptorSet};

    let mut file = FileDescriptorProto {
        name: Some("service.proto".to_string()),
        package: Some(package.to_string()),
        syntax: Some("proto3".to_string()),
        ..FileDescriptorProto::default()
    };

    // Add well-known type dependencies detected in the proto source.
    if proto_source.contains("google/protobuf/timestamp.proto") {
        file.dependency.push("google/protobuf/timestamp.proto".to_string());
    }
    if proto_source.contains("google/protobuf/struct.proto") {
        file.dependency.push("google/protobuf/struct.proto".to_string());
    }

    let fds = FileDescriptorSet { file: vec![file] };

    let mut buf = Vec::with_capacity(fds.encoded_len());
    fds.encode(&mut buf).context("Failed to encode FileDescriptorSet")?;
    Ok(buf)
}

/// Run the `generate-proto` command.
///
/// Reads a compiled schema and writes three files to the output directory:
/// - `service.proto` — proto3 service definition
/// - `vr_migrations.sql` — row-shaped view DDL for the gRPC transport
/// - `descriptor.binpb` — serialized `FileDescriptorSet` for gRPC reflection
///
/// # Errors
///
/// Returns an error if the schema cannot be loaded, the dialect is unknown,
/// or the output files cannot be written.
pub fn run(
    schema_path: &str,
    output_dir: &str,
    package: &str,
    dialect_name: &str,
    formatter: &OutputFormatter,
) -> anyhow::Result<()> {
    formatter.progress("Loading compiled schema...");

    let content = fs::read_to_string(schema_path).context("Failed to read compiled schema file")?;
    let schema: CompiledSchema =
        serde_json::from_str(&content).context("Failed to parse compiled schema JSON")?;

    let dialect = resolve_dialect(dialect_name)?;

    // Resolve include/exclude from grpc config if present
    let (include_types, exclude_types) = schema
        .grpc_config
        .as_ref()
        .map(|g| (g.include_types.clone(), g.exclude_types.clone()))
        .unwrap_or_default();

    // 1. Generate service.proto
    formatter.progress("Generating service.proto...");
    let proto_source =
        proto_gen::generate_proto_file(&schema, package, &include_types, &exclude_types);

    // 2. Generate vr_migrations.sql
    formatter.progress("Generating vr_migrations.sql...");
    let row_view_ddl = row_views::generate_all_row_views(
        dialect.as_ref(),
        &schema.types,
        &include_types,
        &exclude_types,
    );

    // 3. Build descriptor.binpb
    formatter.progress("Building descriptor.binpb...");
    let descriptor_bytes = build_file_descriptor_set(&proto_source, package)?;

    // Write output files
    let out_path = Path::new(output_dir);
    fs::create_dir_all(out_path).context("Failed to create output directory")?;

    let proto_path = out_path.join("service.proto");
    fs::write(&proto_path, &proto_source)
        .with_context(|| format!("Failed to write {}", proto_path.display()))?;

    let sql_path = out_path.join("vr_migrations.sql");
    fs::write(&sql_path, &row_view_ddl)
        .with_context(|| format!("Failed to write {}", sql_path.display()))?;

    let desc_path = out_path.join("descriptor.binpb");
    fs::write(&desc_path, &descriptor_bytes)
        .with_context(|| format!("Failed to write {}", desc_path.display()))?;

    formatter.section("Generated files");
    formatter.progress(&format!("  {}", proto_path.display()));
    formatter.progress(&format!("  {}", sql_path.display()));
    formatter.progress(&format!("  {}", desc_path.display()));

    Ok(())
}


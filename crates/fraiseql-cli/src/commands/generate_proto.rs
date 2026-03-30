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
fn resolve_dialect(name: &str) -> anyhow::Result<Box<dyn SqlDialect>> {
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
fn build_file_descriptor_set(proto_source: &str, package: &str) -> anyhow::Result<Vec<u8>> {
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

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use fraiseql_core::schema::{
        CompiledSchema, EnumDefinition, EnumValueDefinition, FieldDefinition, FieldDenyPolicy,
        FieldType, TypeDefinition,
    };
    use tempfile::TempDir;

    use super::*;

    fn make_field(name: &str, ft: FieldType, nullable: bool) -> FieldDefinition {
        FieldDefinition {
            name: name.into(),
            field_type: ft,
            nullable,
            description: None,
            default_value: None,
            vector_config: None,
            alias: None,
            deprecation: None,
            requires_scope: None,
            on_deny: FieldDenyPolicy::default(),
            encryption: None,
        }
    }

    fn make_type(name: &str, fields: Vec<FieldDefinition>) -> TypeDefinition {
        TypeDefinition {
            name: name.into(),
            sql_source: name.to_lowercase().into(),
            jsonb_column: "data".to_string(),
            fields,
            description: None,
            sql_projection_hint: None,
            implements: vec![],
            requires_role: None,
            is_error: false,
            relay: false,
        }
    }

    fn make_query(
        name: &str,
        return_type: &str,
        returns_list: bool,
    ) -> fraiseql_core::schema::QueryDefinition {
        serde_json::from_value(serde_json::json!({
            "name": name,
            "return_type": return_type,
            "returns_list": returns_list,
        }))
        .expect("test query definition")
    }

    fn write_schema_file(dir: &Path, schema: &CompiledSchema) -> String {
        let json = serde_json::to_string_pretty(schema).expect("serialize schema");
        let path = dir.join("schema.compiled.json");
        let mut f = fs::File::create(&path).expect("create schema file");
        f.write_all(json.as_bytes()).expect("write schema file");
        path.to_string_lossy().into_owned()
    }

    // ── resolve_dialect ──────────────────────────────────────────────────

    #[test]
    fn test_resolve_dialect_postgres() {
        assert!(resolve_dialect("postgres").is_ok());
        assert!(resolve_dialect("postgresql").is_ok());
    }

    #[test]
    fn test_resolve_dialect_mysql() {
        assert!(resolve_dialect("mysql").is_ok());
    }

    #[test]
    fn test_resolve_dialect_sqlite() {
        assert!(resolve_dialect("sqlite").is_ok());
    }

    #[test]
    fn test_resolve_dialect_sqlserver() {
        assert!(resolve_dialect("sqlserver").is_ok());
    }

    #[test]
    fn test_resolve_dialect_unknown() {
        match resolve_dialect("oracle") {
            Ok(_) => panic!("expected error for oracle"),
            Err(e) => assert!(e.to_string().contains("Unknown dialect")),
        }
    }

    // ── build_file_descriptor_set ───────────────────────────────────────

    #[test]
    fn test_descriptor_bytes_non_empty() {
        let proto = "syntax = \"proto3\";\npackage test.v1;\n";
        let bytes = build_file_descriptor_set(proto, "test.v1").expect("encode");
        assert!(!bytes.is_empty());
    }

    #[test]
    fn test_descriptor_includes_timestamp_dep() {
        let proto = "import \"google/protobuf/timestamp.proto\";\n";
        let bytes = build_file_descriptor_set(proto, "test.v1").expect("encode");
        let as_str = String::from_utf8_lossy(&bytes);
        assert!(as_str.contains("google/protobuf/timestamp.proto"));
    }

    #[test]
    fn test_descriptor_includes_struct_dep() {
        let proto = "import \"google/protobuf/struct.proto\";\n";
        let bytes = build_file_descriptor_set(proto, "test.v1").expect("encode");
        let as_str = String::from_utf8_lossy(&bytes);
        assert!(as_str.contains("google/protobuf/struct.proto"));
    }

    #[test]
    fn test_descriptor_no_deps_when_absent() {
        let proto = "syntax = \"proto3\";\n";
        let bytes = build_file_descriptor_set(proto, "test.v1").expect("encode");
        let as_str = String::from_utf8_lossy(&bytes);
        assert!(!as_str.contains("google/protobuf/timestamp.proto"));
    }

    // ── run (integration) ────────────────────────────────────────────────

    #[test]
    fn test_run_generates_three_files() {
        let tmp = TempDir::new().expect("temp dir");
        let mut schema = CompiledSchema::new();
        schema.types.push(make_type(
            "User",
            vec![
                make_field("id", FieldType::Id, false),
                make_field("name", FieldType::String, false),
            ],
        ));
        schema.queries.push(make_query("get_user", "User", false));

        let schema_path = write_schema_file(tmp.path(), &schema);
        let out_dir = tmp.path().join("out");
        let formatter = OutputFormatter::new(false, true);

        run(&schema_path, &out_dir.to_string_lossy(), "test.v1", "postgres", &formatter)
            .expect("run should succeed");

        assert!(out_dir.join("service.proto").exists());
        assert!(out_dir.join("vr_migrations.sql").exists());
        assert!(out_dir.join("descriptor.binpb").exists());

        // Verify proto content
        let proto = fs::read_to_string(out_dir.join("service.proto")).expect("read proto");
        assert!(proto.contains("package test.v1;"));
        assert!(proto.contains("message User {"));
        assert!(proto.contains("service TestService {"));
    }

    #[test]
    fn test_run_with_enum_and_datetime() {
        let tmp = TempDir::new().expect("temp dir");
        let mut schema = CompiledSchema::new();
        schema.enums.push(EnumDefinition {
            name:        "Status".to_string(),
            values:      vec![EnumValueDefinition {
                name:        "ACTIVE".to_string(),
                description: None,
                deprecation: None,
            }],
            description: None,
        });
        schema.types.push(make_type(
            "Event",
            vec![
                make_field("id", FieldType::Id, false),
                make_field("created_at", FieldType::DateTime, false),
                make_field("status", FieldType::Enum("Status".to_string()), false),
            ],
        ));
        schema.queries.push(make_query("get_event", "Event", false));

        let schema_path = write_schema_file(tmp.path(), &schema);
        let out_dir = tmp.path().join("out");
        let formatter = OutputFormatter::new(false, true);

        run(&schema_path, &out_dir.to_string_lossy(), "fraiseql.v1", "postgres", &formatter)
            .expect("run should succeed");

        let proto = fs::read_to_string(out_dir.join("service.proto")).expect("read proto");
        assert!(proto.contains("import \"google/protobuf/timestamp.proto\""));
        assert!(proto.contains("enum Status {"));

        // Descriptor should include timestamp dependency
        let desc = fs::read(out_dir.join("descriptor.binpb")).expect("read descriptor");
        let desc_str = String::from_utf8_lossy(&desc);
        assert!(desc_str.contains("google/protobuf/timestamp.proto"));
    }

    #[test]
    fn test_run_mysql_dialect() {
        let tmp = TempDir::new().expect("temp dir");
        let mut schema = CompiledSchema::new();
        schema.types.push(make_type(
            "User",
            vec![
                make_field("id", FieldType::Id, false),
                make_field("name", FieldType::String, false),
            ],
        ));
        schema.queries.push(make_query("get_user", "User", false));

        let schema_path = write_schema_file(tmp.path(), &schema);
        let out_dir = tmp.path().join("out");
        let formatter = OutputFormatter::new(false, true);

        run(&schema_path, &out_dir.to_string_lossy(), "test.v1", "mysql", &formatter)
            .expect("run with mysql should succeed");

        let sql = fs::read_to_string(out_dir.join("vr_migrations.sql")).expect("read sql");
        assert!(sql.contains("JSON_EXTRACT"));
    }

    #[test]
    fn test_run_bad_schema_path() {
        let tmp = TempDir::new().expect("temp dir");
        let out_dir = tmp.path().join("out");
        let formatter = OutputFormatter::new(false, true);

        let result = run(
            "/nonexistent/schema.compiled.json",
            &out_dir.to_string_lossy(),
            "test.v1",
            "postgres",
            &formatter,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_run_bad_dialect() {
        let tmp = TempDir::new().expect("temp dir");
        let mut schema = CompiledSchema::new();
        schema
            .types
            .push(make_type("User", vec![make_field("id", FieldType::Id, false)]));

        let schema_path = write_schema_file(tmp.path(), &schema);
        let out_dir = tmp.path().join("out");
        let formatter = OutputFormatter::new(false, true);

        let result = run(&schema_path, &out_dir.to_string_lossy(), "test.v1", "oracle", &formatter);
        assert!(result.is_err());
        match result {
            Ok(()) => panic!("expected error for oracle dialect"),
            Err(e) => assert!(e.to_string().contains("Unknown dialect")),
        }
    }
}

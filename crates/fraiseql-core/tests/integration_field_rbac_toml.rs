//! RED Phase: Tests for field-level RBAC via TOML configuration.
//!
//! These tests verify that TOML configuration can define role definitions
//! and scope mappings, which are then compiled into schema.compiled.json
//! for runtime field filtering.

#[test]
fn test_toml_role_definitions_parsing() {
    // RED: This test documents the expected TOML structure for role definitions
    // Example TOML that should be parseable:
    //
    // [[security.role_definitions]]
    // name = "admin"
    // description = "Administrator with all scopes"
    // scopes = ["admin:*"]
    //
    // [[security.role_definitions]]
    // name = "user"
    // description = "Regular user with limited scopes"
    // scopes = ["read:User.*", "write:Post.content"]
    //
    // [[security.role_definitions]]
    // name = "viewer"
    // description = "Read-only viewer"
    // scopes = ["read:*"]

    let toml_content = r#"
[[security.role_definitions]]
name = "admin"
description = "Administrator with all scopes"
scopes = ["admin:*"]

[[security.role_definitions]]
name = "user"
description = "Regular user with limited scopes"
scopes = ["read:User.*", "write:Post.content"]

[[security.role_definitions]]
name = "viewer"
description = "Read-only viewer"
scopes = ["read:*"]
"#;

    let parsed: toml::Table = toml::from_str(toml_content).expect("Valid TOML");

    // Verify security section exists
    assert!(parsed.contains_key("security"), "TOML should contain security section");

    let security = parsed.get("security").expect("security section");
    assert!(security.is_table(), "security should be a table");

    let security_table = security.as_table().expect("security table");
    assert!(
        security_table.contains_key("role_definitions"),
        "security should contain role_definitions"
    );
}

#[test]
fn test_role_definition_structure() {
    // RED: This test defines the expected structure of a role definition
    let toml_content = r#"
[[security.role_definitions]]
name = "admin"
description = "Administrator with all scopes"
scopes = ["admin:*"]

[[security.role_definitions]]
name = "user"
scopes = ["read:User.*"]
"#;

    let parsed: toml::Table = toml::from_str(toml_content).expect("Valid TOML");
    let security = parsed.get("security").expect("security");
    let security_table = security.as_table().expect("table");
    let roles = security_table
        .get("role_definitions")
        .expect("role_definitions")
        .as_array()
        .expect("array");

    // Verify first role: admin
    let admin_role = &roles[0];
    assert_eq!(
        admin_role.get("name").and_then(|v| v.as_str()),
        Some("admin"),
        "First role should be admin"
    );
    assert_eq!(
        admin_role.get("description").and_then(|v| v.as_str()),
        Some("Administrator with all scopes"),
        "Admin should have description"
    );
    let admin_scopes = admin_role.get("scopes").and_then(|v| v.as_array()).expect("admin scopes");
    assert_eq!(admin_scopes.len(), 1, "Admin should have 1 scope");
    assert_eq!(admin_scopes[0].as_str(), Some("admin:*"), "Admin scope should be admin:*");

    // Verify second role: user
    let user_role = &roles[1];
    assert_eq!(
        user_role.get("name").and_then(|v| v.as_str()),
        Some("user"),
        "Second role should be user"
    );
    assert!(user_role.get("description").is_none(), "User should not have description");
    let user_scopes = user_role.get("scopes").and_then(|v| v.as_array()).expect("user scopes");
    assert_eq!(user_scopes.len(), 1, "User should have 1 scope");
    assert_eq!(user_scopes[0].as_str(), Some("read:User.*"), "User scope should be read:User.*");
}

#[test]
fn test_multiple_scopes_per_role() {
    // RED: Roles should support multiple scopes
    let toml_content = r#"
[[security.role_definitions]]
name = "editor"
scopes = ["read:*", "write:Post.*", "write:Comment.*"]
"#;

    let parsed: toml::Table = toml::from_str(toml_content).expect("Valid TOML");
    let security = parsed.get("security").expect("security");
    let security_table = security.as_table().expect("table");
    let roles = security_table
        .get("role_definitions")
        .expect("role_definitions")
        .as_array()
        .expect("array");

    let editor_role = &roles[0];
    let scopes = editor_role.get("scopes").and_then(|v| v.as_array()).expect("scopes");

    assert_eq!(scopes.len(), 3, "Editor should have 3 scopes");
    assert_eq!(scopes[0].as_str(), Some("read:*"));
    assert_eq!(scopes[1].as_str(), Some("write:Post.*"));
    assert_eq!(scopes[2].as_str(), Some("write:Comment.*"));
}

#[test]
fn test_environment_overrides_for_roles() {
    // RED: TOML should support environment-specific role overrides
    // Example with environment override:
    //
    // [security.role_definitions]
    // admin_default_scopes = ["admin:*"]
    //
    // [security.role_definitions.production]
    // admin_scopes = ["admin:*", "audit:log"]  # Add audit scope in production
    //
    // [security.role_definitions.staging]
    // admin_scopes = ["admin:*"]  # Default for staging

    let toml_content = r#"
[security.role_definitions.default]
name = "admin"
scopes = ["admin:*"]

[security.role_definitions.production]
name = "admin"
scopes = ["admin:*", "audit:log_access"]

[security.role_definitions.staging]
name = "admin"
scopes = ["admin:*"]
"#;

    let parsed: toml::Table = toml::from_str(toml_content).expect("Valid TOML");
    let security = parsed.get("security").expect("security");
    let security_table = security.as_table().expect("table");
    let role_defs = security_table.get("role_definitions").expect("role_definitions");
    let role_defs_table = role_defs.as_table().expect("table");

    // Verify we have environment-specific overrides
    assert!(role_defs_table.contains_key("default"), "Should have default environment");
    assert!(role_defs_table.contains_key("production"), "Should have production environment");
    assert!(role_defs_table.contains_key("staging"), "Should have staging environment");

    // Verify production has additional audit scope
    let prod = role_defs_table.get("production").expect("production");
    let prod_table = prod.as_table().expect("table");
    let prod_scopes = prod_table.get("scopes").and_then(|v| v.as_array()).expect("scopes");
    assert_eq!(prod_scopes.len(), 2, "Production should have 2 scopes");
    assert_eq!(prod_scopes[1].as_str(), Some("audit:log_access"));
}

#[test]
fn test_complex_toml_with_role_definitions() {
    // RED: Complete TOML with multiple sections and role definitions
    let toml_content = r#"
[fraiseql]
version = "2.0"
database_url = "postgresql://localhost/fraiseql"

[fraiseql.server]
host = "localhost"
port = 8000

[security]
default_role = "user"

[[security.role_definitions]]
name = "admin"
description = "Full access"
scopes = ["admin:*"]

[[security.role_definitions]]
name = "user"
description = "Standard user"
scopes = ["read:User.*", "write:Post.content", "write:Comment.content"]

[[security.role_definitions]]
name = "moderator"
description = "Content moderator"
scopes = ["read:*", "write:Post.*", "write:Comment.*", "admin:delete_content"]

[[security.role_definitions]]
name = "viewer"
description = "Read-only access"
scopes = ["read:*"]
"#;

    let parsed: toml::Table = toml::from_str(toml_content).expect("Valid TOML");

    // Verify fraiseql section
    let fraiseql = parsed.get("fraiseql").expect("fraiseql");
    let fraiseql_table = fraiseql.as_table().expect("table");
    assert_eq!(fraiseql_table.get("version").and_then(|v| v.as_str()), Some("2.0"));

    // Verify security section
    let security = parsed.get("security").expect("security");
    let security_table = security.as_table().expect("table");
    assert_eq!(security_table.get("default_role").and_then(|v| v.as_str()), Some("user"));

    // Verify all 4 roles are present
    let roles = security_table
        .get("role_definitions")
        .expect("role_definitions")
        .as_array()
        .expect("array");
    assert_eq!(roles.len(), 4, "Should have 4 roles defined");

    // Verify each role has name and scopes
    for role in roles {
        assert!(
            role.get("name").and_then(|v| v.as_str()).is_some(),
            "Each role must have a name"
        );
        assert!(
            role.get("scopes").and_then(|v| v.as_array()).is_some(),
            "Each role must have scopes array"
        );
    }
}

#[test]
fn test_role_definition_validation() {
    // RED: Invalid role definitions should be detectable
    // Examples of invalid TOML:
    // - Role without name
    // - Role without scopes
    // - Invalid scope format
    // - Duplicate role names

    // Valid role
    let valid_toml = r#"
[[security.role_definitions]]
name = "admin"
scopes = ["admin:*"]
"#;
    assert!(toml::from_str::<toml::Table>(valid_toml).is_ok(), "Valid role should parse");

    // Invalid: missing name
    let no_name_toml = r#"
[[security.role_definitions]]
scopes = ["admin:*"]
"#;
    assert!(
        toml::from_str::<toml::Table>(no_name_toml).is_ok(),
        "TOML parser accepts missing name (validation is separate)"
    );

    // Invalid: missing scopes
    let no_scopes_toml = r#"
[[security.role_definitions]]
name = "admin"
"#;
    assert!(
        toml::from_str::<toml::Table>(no_scopes_toml).is_ok(),
        "TOML parser accepts missing scopes (validation is separate)"
    );
}

#[test]
fn test_toml_scope_format_validation() {
    // RED: Scope values should follow naming convention
    // Valid formats:
    // - read:Type.field
    // - read:Type.*
    // - read:*
    // - custom:identifier
    // Invalid formats will be caught at runtime compilation

    let valid_scopes = vec![
        "read:User.email",
        "read:User.*",
        "read:*",
        "write:Post.title",
        "admin:*",
        "hr:view_compensation",
        "pii:view",
    ];

    for scope in valid_scopes {
        // Verify we can parse it as a TOML string value
        let toml_str = format!(r#"scopes = ["{}"]"#, scope);
        assert!(
            toml::from_str::<toml::Table>(&toml_str).is_ok(),
            "Valid scope '{}' should parse as TOML",
            scope
        );
    }
}

#[test]
fn test_role_definitions_compilation_to_schema() {
    // RED: Role definitions from TOML should be compilable to compiled schema
    // The compiled schema should include:
    // - role_definitions array
    // - Each role with: name, description?, scopes[]
    // - Ready for runtime field filtering

    // This test documents the compilation flow:
    // 1. Parse TOML role definitions
    // 2. Merge with schema.json field scopes
    // 3. Generate schema.compiled.json with both
    // 4. Runtime loads and enforces

    let toml_content = r#"
[[security.role_definitions]]
name = "admin"
scopes = ["admin:*"]

[[security.role_definitions]]
name = "user"
scopes = ["read:User.*", "write:Post.content"]
"#;

    let parsed: toml::Table = toml::from_str(toml_content).expect("Valid TOML");
    let security = parsed.get("security").expect("security");

    // Convert to JSON (what compiled schema will contain)
    let json_value = serde_json::to_value(security).expect("Convert to JSON");

    // Verify JSON structure matches expected compiled schema format
    assert!(json_value.get("role_definitions").is_some());
    let role_defs = json_value
        .get("role_definitions")
        .and_then(|v| v.as_array())
        .expect("role_definitions array");
    assert_eq!(role_defs.len(), 2);

    // Each role should have name and scopes in JSON
    for role in role_defs {
        assert!(role.get("name").is_some(), "Role must have name in JSON");
        assert!(role.get("scopes").is_some(), "Role must have scopes in JSON");
    }
}

// GREEN Phase: Role Definition struct tests

#[test]
fn test_role_definition_struct() {
    use fraiseql_core::schema::RoleDefinition;

    let mut role = RoleDefinition::new("admin".to_string(), vec!["admin:*".to_string()]);
    assert_eq!(role.name, "admin");
    assert_eq!(role.scopes, vec!["admin:*"]);
    assert!(role.description.is_none());

    role = role.with_description("Administrator with all access".to_string());
    assert_eq!(role.description, Some("Administrator with all access".to_string()));
}

#[test]
fn test_role_has_scope_exact_match() {
    use fraiseql_core::schema::RoleDefinition;

    let role = RoleDefinition::new(
        "user".to_string(),
        vec![
            "read:User.email".to_string(),
            "write:Post.content".to_string(),
        ],
    );

    assert!(role.has_scope("read:User.email"));
    assert!(role.has_scope("write:Post.content"));
    assert!(!role.has_scope("admin:*"));
}

#[test]
fn test_role_has_scope_wildcard_all() {
    use fraiseql_core::schema::RoleDefinition;

    let role = RoleDefinition::new("admin".to_string(), vec!["*".to_string()]);

    // Wildcard "*" matches everything
    assert!(role.has_scope("read:User.email"));
    assert!(role.has_scope("write:Post.title"));
    assert!(role.has_scope("admin:*"));
    assert!(role.has_scope("any:scope:format"));
}

#[test]
fn test_role_has_scope_wildcard_prefix() {
    use fraiseql_core::schema::RoleDefinition;

    let role = RoleDefinition::new(
        "reader".to_string(),
        vec!["read:*".to_string(), "admin:delete".to_string()],
    );

    // "read:*" matches any scope starting with "read:"
    assert!(role.has_scope("read:User.email"));
    assert!(role.has_scope("read:Post.title"));
    assert!(role.has_scope("read:Comment.*"));
    assert!(!role.has_scope("write:User.name"));
    assert!(role.has_scope("admin:delete"));
    assert!(!role.has_scope("admin:*"));
}

#[test]
fn test_security_config_struct() {
    use fraiseql_core::schema::{RoleDefinition, SecurityConfig};

    let mut config = SecurityConfig::new();
    assert!(config.role_definitions.is_empty());

    let admin_role = RoleDefinition::new("admin".to_string(), vec!["admin:*".to_string()]);
    let user_role = RoleDefinition::new("user".to_string(), vec!["read:User.*".to_string()]);

    config.add_role(admin_role);
    config.add_role(user_role);

    assert_eq!(config.role_definitions.len(), 2);
    assert_eq!(config.find_role("admin").unwrap().name, "admin");
    assert_eq!(config.find_role("user").unwrap().name, "user");
    assert!(config.find_role("nonexistent").is_none());
}

#[test]
fn test_security_config_get_role_scopes() {
    use fraiseql_core::schema::{RoleDefinition, SecurityConfig};

    let mut config = SecurityConfig::new();
    let role = RoleDefinition::new(
        "editor".to_string(),
        vec![
            "read:*".to_string(),
            "write:Post.*".to_string(),
            "write:Comment.*".to_string(),
        ],
    );

    config.add_role(role);

    let scopes = config.get_role_scopes("editor");
    assert_eq!(scopes.len(), 3);
    assert_eq!(scopes[0], "read:*");
    assert_eq!(scopes[1], "write:Post.*");
    assert_eq!(scopes[2], "write:Comment.*");

    assert!(config.get_role_scopes("nonexistent").is_empty());
}

#[test]
fn test_security_config_role_has_scope() {
    use fraiseql_core::schema::{RoleDefinition, SecurityConfig};

    let mut config = SecurityConfig::new();
    let admin_role = RoleDefinition::new("admin".to_string(), vec!["admin:*".to_string()]);

    config.add_role(admin_role);

    assert!(config.role_has_scope("admin", "admin:delete"));
    assert!(config.role_has_scope("admin", "admin:anything"));
    assert!(!config.role_has_scope("user", "admin:*"));
}

#[test]
fn test_security_config_from_json() {
    use fraiseql_core::schema::SecurityConfig;

    let json_str = r#"{
      "role_definitions": [
        {
          "name": "admin",
          "description": "Full access",
          "scopes": ["admin:*"]
        },
        {
          "name": "user",
          "scopes": ["read:User.*", "write:Post.content"]
        }
      ],
      "default_role": "user"
    }"#;

    let config: SecurityConfig = serde_json::from_str(json_str).expect("Parse JSON");
    assert_eq!(config.role_definitions.len(), 2);
    assert_eq!(config.default_role, Some("user".to_string()));

    let admin = config.find_role("admin").expect("Find admin");
    assert_eq!(admin.name, "admin");
    assert_eq!(
        admin.description,
        Some("Full access".to_string()),
        "Description should be preserved"
    );
    assert_eq!(admin.scopes, vec!["admin:*"]);

    let user = config.find_role("user").expect("Find user");
    assert!(user.description.is_none(), "User role should have no description");
    assert_eq!(user.scopes, vec!["read:User.*", "write:Post.content"]);
}

#[test]
fn test_compiled_schema_with_security_config() {
    use fraiseql_core::schema::CompiledSchema;

    let schema_json = r#"{
      "types": [
        {
          "name": "User",
          "sql_source": "v_user",
          "fields": [
            {"name": "id", "field_type": "ID", "nullable": false},
            {"name": "email", "field_type": "String", "nullable": false, "requiresScope": "read:User.email"}
          ]
        }
      ],
      "queries": [],
      "mutations": [],
      "subscriptions": [],
      "security": {
        "role_definitions": [
          {
            "name": "admin",
            "scopes": ["admin:*"]
          },
          {
            "name": "user",
            "scopes": ["read:User.*", "write:Post.content"]
          }
        ],
        "default_role": "user"
      }
    }"#;

    let schema = CompiledSchema::from_json(schema_json).expect("Parse schema");

    // Verify schema was loaded
    assert_eq!(schema.types.len(), 1);

    // Verify security config was loaded
    let config = schema.security_config().expect("Get security config");
    assert_eq!(config.role_definitions.len(), 2);

    // Verify role lookups work
    let admin_role = schema.find_role("admin").expect("Find admin role");
    assert_eq!(admin_role.name, "admin");
    assert_eq!(admin_role.scopes, vec!["admin:*"]);

    // Verify scope checking on schema
    assert!(
        schema.role_has_scope("admin", "admin:delete"),
        "admin:* should match admin:delete"
    );
    assert!(
        schema.role_has_scope("admin", "admin:view_logs"),
        "admin:* should match admin:view_logs"
    );
    assert!(
        schema.role_has_scope("user", "read:User.email"),
        "user should have read:User.* which matches read:User.email"
    );
    assert!(
        schema.role_has_scope("user", "write:Post.content"),
        "user should have write:Post.content"
    );
    assert!(
        !schema.role_has_scope("user", "admin:delete"),
        "user should not have admin scopes"
    );

    // Verify get_role_scopes
    let user_scopes = schema.get_role_scopes("user");
    assert_eq!(user_scopes, vec!["read:User.*", "write:Post.content"]);
}

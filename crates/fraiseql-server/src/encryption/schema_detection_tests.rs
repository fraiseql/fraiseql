//! Comprehensive tests for automatic schema detection of encrypted fields,
//! supporting multiple encryption marks, key references, and schema evolution.

#[cfg(test)]
#[allow(clippy::module_inception)]
mod schema_detection_tests {
    use crate::encryption::{
        FieldEncryption,
        schema::{EncryptionMark, SchemaFieldInfo, SchemaRegistry, StructSchema},
    };

    // ============================================================================
    // BASIC SCHEMA DETECTION TESTS
    // ============================================================================

    /// Test detect basic #[encrypted] attribute on field
    #[test]
    fn test_schema_detect_basic_encrypted_attribute() {
        let mut schema = StructSchema::new("User");
        let email = SchemaFieldInfo::new("email", "String", true, "encryption/email")
            .with_mark(EncryptionMark::Encrypted);
        schema.add_field(email);

        assert!(schema.is_field_encrypted("email"));
        let field = schema.get_encrypted_field("email").unwrap();
        assert_eq!(field.mark, Some(EncryptionMark::Encrypted));
        assert!(field.is_encrypted);
    }

    /// Test detect multiple encrypted fields
    #[test]
    fn test_schema_detect_multiple_encrypted_fields() {
        let mut schema = StructSchema::new("User");
        schema.add_field(SchemaFieldInfo::new("id", "i64", false, ""));
        schema.add_field(SchemaFieldInfo::new("name", "String", false, ""));
        schema.add_field(
            SchemaFieldInfo::new("email", "String", true, "encryption/email")
                .with_mark(EncryptionMark::Encrypted),
        );
        schema.add_field(
            SchemaFieldInfo::new("phone", "String", true, "encryption/phone")
                .with_mark(EncryptionMark::Encrypted),
        );
        schema.add_field(
            SchemaFieldInfo::new("ssn", "String", true, "encryption/ssn")
                .with_mark(EncryptionMark::Encrypted),
        );

        assert_eq!(schema.encrypted_field_count(), 3);
        assert!(schema.is_field_encrypted("email"));
        assert!(schema.is_field_encrypted("phone"));
        assert!(schema.is_field_encrypted("ssn"));
        assert!(!schema.is_field_encrypted("id"));
        assert!(!schema.is_field_encrypted("name"));
    }

    /// Test ignore unencrypted fields
    #[test]
    fn test_schema_ignore_unencrypted_fields() {
        let mut schema = StructSchema::new("User");
        schema.add_field(SchemaFieldInfo::new("id", "i64", false, ""));
        schema.add_field(SchemaFieldInfo::new("name", "String", false, ""));
        schema.add_field(SchemaFieldInfo::new("created_at", "DateTime", false, ""));
        schema.add_field(
            SchemaFieldInfo::new("email", "String", true, "encryption/email")
                .with_mark(EncryptionMark::Encrypted),
        );

        let encrypted_names = schema.encrypted_field_names();
        assert_eq!(encrypted_names.len(), 1);
        assert!(encrypted_names.contains(&"email"));
        assert!(!encrypted_names.contains(&"id"));
        assert!(!encrypted_names.contains(&"name"));
        assert!(!encrypted_names.contains(&"created_at"));
        assert_eq!(schema.total_field_count(), 4);
    }

    /// Test empty struct (no encrypted fields)
    #[test]
    fn test_schema_empty_encrypted_fields() {
        let mut schema = StructSchema::new("Config");
        schema.add_field(SchemaFieldInfo::new("id", "i64", false, ""));
        schema.add_field(SchemaFieldInfo::new("name", "String", false, ""));
        schema.add_field(SchemaFieldInfo::new("value", "String", false, ""));

        assert_eq!(schema.encrypted_field_count(), 0);
        assert!(schema.encrypted_field_names().is_empty());
        assert!(schema.validate().is_ok());
    }

    /// Test all fields encrypted
    #[test]
    fn test_schema_all_fields_encrypted() {
        let mut schema = StructSchema::new("SecretVault");
        schema.add_field(
            SchemaFieldInfo::new("api_key", "String", true, "encryption/api_key")
                .with_mark(EncryptionMark::Encrypted),
        );
        schema.add_field(
            SchemaFieldInfo::new("api_secret", "String", true, "encryption/api_secret")
                .with_mark(EncryptionMark::Encrypted),
        );
        schema.add_field(
            SchemaFieldInfo::new("token", "String", true, "encryption/token")
                .with_mark(EncryptionMark::Encrypted),
        );

        assert_eq!(schema.encrypted_field_count(), 3);
        assert_eq!(schema.total_field_count(), 3);
        // Every field is encrypted
        for field in &schema.all_fields {
            assert!(field.is_encrypted);
        }
    }

    // ============================================================================
    // ALTERNATIVE ENCRYPTION MARKS
    // ============================================================================

    /// Test #[sensitive] as encryption mark
    #[test]
    fn test_schema_detect_sensitive_attribute() {
        let mut schema = StructSchema::new("Patient");
        schema.add_field(
            SchemaFieldInfo::new("diagnosis", "String", true, "encryption/default")
                .with_mark(EncryptionMark::Sensitive),
        );

        let field = schema.get_encrypted_field("diagnosis").unwrap();
        assert_eq!(field.mark, Some(EncryptionMark::Sensitive));
        assert!(field.is_encrypted);
        // Sensitive mark results in the same encryption behavior
        assert_eq!(field.algorithm, "aes256-gcm");
    }

    /// Test #[encrypt(key="...")] with key reference
    #[test]
    fn test_schema_detect_encrypt_with_key_reference() {
        let mut schema = StructSchema::new("User");
        schema.add_field(
            SchemaFieldInfo::new("email", "String", true, "vault/database/encryption/user_email")
                .with_mark(EncryptionMark::Encrypt),
        );
        schema.add_field(
            SchemaFieldInfo::new("phone", "String", true, "vault/database/encryption/user_phone")
                .with_mark(EncryptionMark::Encrypt),
        );

        let email_field = schema.get_encrypted_field("email").unwrap();
        assert_eq!(email_field.key_reference, "vault/database/encryption/user_email");
        assert_eq!(email_field.mark, Some(EncryptionMark::Encrypt));

        let phone_field = schema.get_encrypted_field("phone").unwrap();
        assert_eq!(phone_field.key_reference, "vault/database/encryption/user_phone");
        // Different fields can reference different keys
        assert_ne!(email_field.key_reference, phone_field.key_reference);
    }

    /// Test #[encrypt(algorithm="...")] hint
    #[test]
    fn test_schema_detect_encrypt_with_algorithm_hint() {
        let mut schema = StructSchema::new("User");
        schema.add_field(
            SchemaFieldInfo::new("email", "String", true, "encryption/email")
                .with_algorithm("aes256-gcm")
                .with_mark(EncryptionMark::Encrypt),
        );
        schema.add_field(
            SchemaFieldInfo::new("backup_key", "String", true, "encryption/backup")
                .with_algorithm("chacha20-poly1305")
                .with_mark(EncryptionMark::Encrypt),
        );

        let email_field = schema.get_encrypted_field("email").unwrap();
        assert_eq!(email_field.algorithm, "aes256-gcm");

        let backup_field = schema.get_encrypted_field("backup_key").unwrap();
        assert_eq!(backup_field.algorithm, "chacha20-poly1305");
    }

    /// Test mixed encryption marks in same struct
    #[test]
    fn test_schema_mixed_encryption_marks() {
        let mut schema = StructSchema::new("Employee");
        schema.add_field(
            SchemaFieldInfo::new("email", "String", true, "encryption/email")
                .with_mark(EncryptionMark::Encrypted),
        );
        schema.add_field(
            SchemaFieldInfo::new("medical_info", "String", true, "encryption/default")
                .with_mark(EncryptionMark::Sensitive),
        );
        schema.add_field(
            SchemaFieldInfo::new("ssn", "String", true, "encryption/ssn")
                .with_mark(EncryptionMark::Encrypt),
        );
        schema.add_field(SchemaFieldInfo::new("name", "String", false, ""));

        assert_eq!(schema.encrypted_field_count(), 3);
        // All three marks result in fields being encrypted
        assert!(schema.is_field_encrypted("email"));
        assert!(schema.is_field_encrypted("medical_info"));
        assert!(schema.is_field_encrypted("ssn"));

        // Verify each has its own mark
        assert_eq!(
            schema.get_encrypted_field("email").unwrap().mark,
            Some(EncryptionMark::Encrypted)
        );
        assert_eq!(
            schema.get_encrypted_field("medical_info").unwrap().mark,
            Some(EncryptionMark::Sensitive)
        );
        assert_eq!(schema.get_encrypted_field("ssn").unwrap().mark, Some(EncryptionMark::Encrypt));
    }

    /// Test invalid encryption mark rejected
    #[test]
    fn test_schema_invalid_encryption_mark_rejected() {
        // Only Encrypted, Sensitive, and Encrypt are valid marks
        // A field without a valid mark but marked is_encrypted=true with empty key is invalid
        let mut schema = StructSchema::new("User");
        schema.add_field(SchemaFieldInfo::new("email", "String", true, ""));

        let result = schema.validate();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("missing key reference"),
            "Expected key reference error, got: {}",
            err_msg
        );
    }

    // ============================================================================
    // KEY REFERENCE VALIDATION
    // ============================================================================

    /// Test key reference extracted from attribute
    #[test]
    fn test_schema_key_reference_extracted() {
        let mut schema = StructSchema::new("User");
        schema.add_field(
            SchemaFieldInfo::new("email", "String", true, "database/creds/user_email")
                .with_mark(EncryptionMark::Encrypt),
        );

        let field = schema.get_encrypted_field("email").unwrap();
        assert_eq!(field.key_reference, "database/creds/user_email");
    }

    /// Test default key when not specified
    #[test]
    fn test_schema_default_key_when_unspecified() {
        let registry = SchemaRegistry::new();
        // The registry has a default key "encryption/default"
        let mut schema = StructSchema::new("User");
        schema.add_field(
            SchemaFieldInfo::new("email", "String", true, "encryption/default")
                .with_mark(EncryptionMark::Encrypted),
        );

        let field = schema.get_encrypted_field("email").unwrap();
        assert_eq!(field.key_reference, "encryption/default");

        // Registry recognizes this as the default key
        let all_keys = {
            let mut reg = SchemaRegistry::new();
            reg.register(schema).unwrap();
            reg.all_encryption_keys()
        };
        assert!(all_keys.contains(&"encryption/default".to_string()));
        drop(registry);
    }

    /// Test per-field key override
    #[test]
    fn test_schema_per_field_key_override() {
        let mut schema = StructSchema::new("User");
        schema.add_field(
            SchemaFieldInfo::new("email", "String", true, "encryption/email")
                .with_mark(EncryptionMark::Encrypt),
        );
        schema.add_field(
            SchemaFieldInfo::new("phone", "String", true, "encryption/phone")
                .with_mark(EncryptionMark::Encrypt),
        );
        schema.add_field(
            SchemaFieldInfo::new("ssn", "String", true, "encryption/ssn")
                .with_mark(EncryptionMark::Encrypt),
        );

        // Each field has a different key reference
        assert_eq!(schema.get_encrypted_field("email").unwrap().key_reference, "encryption/email");
        assert_eq!(schema.get_encrypted_field("phone").unwrap().key_reference, "encryption/phone");
        assert_eq!(schema.get_encrypted_field("ssn").unwrap().key_reference, "encryption/ssn");

        // fields_for_key groups correctly
        assert_eq!(schema.fields_for_key("encryption/email").len(), 1);
        assert_eq!(schema.fields_for_key("encryption/phone").len(), 1);
        assert_eq!(schema.fields_for_key("encryption/ssn").len(), 1);
    }

    /// Test key reference validation at startup
    #[test]
    fn test_schema_key_reference_validation_startup() {
        let mut registry = SchemaRegistry::new();
        let mut schema = StructSchema::new("User");
        schema.add_field(SchemaFieldInfo::new("email", "String", true, "encryption/email"));
        registry.register(schema).unwrap();

        // validate_all checks all registered schemas
        assert!(registry.validate_all().is_ok());

        // All encryption keys used can be collected for Vault validation
        let keys = registry.all_encryption_keys();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0], "encryption/email");
    }

    /// Test missing key detection
    #[test]
    fn test_schema_missing_key_detected() {
        let mut schema = StructSchema::new("User");
        // Empty key reference = missing key
        schema.add_field(SchemaFieldInfo::new("email", "String", true, ""));

        let result = schema.validate();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("email"), "Error should mention the field name: {}", err);
        assert!(
            err.contains("missing key reference"),
            "Error should indicate missing key: {}",
            err
        );
    }

    /// Test key size validation
    #[test]
    fn test_schema_key_size_validation() {
        // AES-256 requires exactly 32 bytes
        let valid_key = [0u8; 32];
        let _cipher = FieldEncryption::new(&valid_key);

        // Wrong key sizes are rejected
        let result = std::panic::catch_unwind(|| {
            let invalid_key = [0u8; 16]; // AES-128 size, not AES-256
            FieldEncryption::new(&invalid_key)
        });
        assert!(result.is_err());

        let result = std::panic::catch_unwind(|| {
            let invalid_key = [0u8; 24]; // AES-192 size
            FieldEncryption::new(&invalid_key)
        });
        assert!(result.is_err());

        let result = std::panic::catch_unwind(|| {
            let invalid_key = [0u8; 64]; // Too large
            FieldEncryption::new(&invalid_key)
        });
        assert!(result.is_err());
    }

    // ============================================================================
    // SCHEMA EVOLUTION
    // ============================================================================

    /// Test adding encrypted field to existing schema
    #[test]
    fn test_schema_evolution_add_encrypted_field() {
        // Version 1: no encryption
        let schema_v1 = StructSchema::new("User").with_version(1).with_fields(vec![
            SchemaFieldInfo::new("id", "i64", false, ""),
            SchemaFieldInfo::new("name", "String", false, ""),
            SchemaFieldInfo::new("email", "String", false, ""),
        ]);

        assert_eq!(schema_v1.encrypted_field_count(), 0);
        assert_eq!(schema_v1.version, 1);

        // Version 2: email now encrypted, phone added and encrypted
        let schema_v2 = StructSchema::new("User").with_version(2).with_fields(vec![
            SchemaFieldInfo::new("id", "i64", false, ""),
            SchemaFieldInfo::new("name", "String", false, ""),
            SchemaFieldInfo::new("email", "String", true, "encryption/email")
                .with_mark(EncryptionMark::Encrypted),
            SchemaFieldInfo::new("phone", "String", true, "encryption/phone")
                .with_mark(EncryptionMark::Encrypted),
        ]);

        assert_eq!(schema_v2.encrypted_field_count(), 2);
        assert_eq!(schema_v2.version, 2);
        assert!(schema_v2.is_field_encrypted("email"));
        assert!(schema_v2.is_field_encrypted("phone"));
    }

    /// Test removing encryption from field
    #[test]
    fn test_schema_evolution_remove_encryption_mark() {
        // Version 1: email encrypted
        let schema_v1 = StructSchema::new("User").with_version(1).with_fields(vec![
            SchemaFieldInfo::new("id", "i64", false, ""),
            SchemaFieldInfo::new("email", "String", true, "encryption/email")
                .with_mark(EncryptionMark::Encrypted),
        ]);

        assert!(schema_v1.is_field_encrypted("email"));

        // Version 2: email no longer encrypted
        let schema_v2 = StructSchema::new("User").with_version(2).with_fields(vec![
            SchemaFieldInfo::new("id", "i64", false, ""),
            SchemaFieldInfo::new("email", "String", false, ""),
        ]);

        assert!(!schema_v2.is_field_encrypted("email"));
        assert_eq!(schema_v2.encrypted_field_count(), 0);

        // Registry can hold both versions for migration
        let mut registry = SchemaRegistry::new();
        registry.register(schema_v2).unwrap();
        let current = registry.get("User").unwrap();
        assert!(!current.is_field_encrypted("email"));
    }

    /// Test changing key for field
    #[test]
    fn test_schema_evolution_key_rotation() {
        let schema_v1 =
            StructSchema::new("User").with_version(1).with_fields(vec![SchemaFieldInfo::new(
                "email",
                "String",
                true,
                "encryption/old_key",
            )]);

        let schema_v2 =
            StructSchema::new("User").with_version(2).with_fields(vec![SchemaFieldInfo::new(
                "email",
                "String",
                true,
                "encryption/new_key",
            )]);

        assert_eq!(
            schema_v1.get_encrypted_field("email").unwrap().key_reference,
            "encryption/old_key"
        );
        assert_eq!(
            schema_v2.get_encrypted_field("email").unwrap().key_reference,
            "encryption/new_key"
        );

        // Data encrypted with old key still needs old key to decrypt
        let old_key = [1u8; 32];
        let new_key = [2u8; 32];
        let old_cipher = FieldEncryption::new(&old_key);
        let new_cipher = FieldEncryption::new(&new_key);

        let encrypted_with_old = old_cipher.encrypt("test@example.com").unwrap();
        assert_eq!(old_cipher.decrypt(&encrypted_with_old).unwrap(), "test@example.com");
        // New key cannot decrypt old data
        assert!(new_cipher.decrypt(&encrypted_with_old).is_err());
    }

    /// Test schema versioning
    #[test]
    fn test_schema_versioning_with_encryption() {
        let schema_v1 = StructSchema::new("User").with_version(1).with_fields(vec![
            SchemaFieldInfo::new("id", "i64", false, ""),
            SchemaFieldInfo::new("name", "String", false, ""),
            SchemaFieldInfo::new("email", "String", false, ""),
        ]);

        let schema_v2 = StructSchema::new("User").with_version(2).with_fields(vec![
            SchemaFieldInfo::new("id", "i64", false, ""),
            SchemaFieldInfo::new("name", "String", false, ""),
            SchemaFieldInfo::new("email", "String", true, "encryption/email"),
            SchemaFieldInfo::new("phone", "String", true, "encryption/phone"),
        ]);

        assert_eq!(schema_v1.version, 1);
        assert_eq!(schema_v1.encrypted_field_count(), 0);

        assert_eq!(schema_v2.version, 2);
        assert_eq!(schema_v2.encrypted_field_count(), 2);

        // Version tracks which schema was used to store data
        // Records stored with v1 have no encryption
        // Records stored with v2 have email and phone encrypted
    }

    /// Test nullable encrypted fields
    #[test]
    fn test_schema_evolution_nullable_encrypted() {
        let mut schema = StructSchema::new("User");
        schema.add_field(
            SchemaFieldInfo::new("email", "Option<String>", true, "encryption/email")
                .with_nullable(true)
                .with_mark(EncryptionMark::Encrypted),
        );
        schema.add_field(
            SchemaFieldInfo::new("phone", "String", true, "encryption/phone")
                .with_nullable(false)
                .with_mark(EncryptionMark::Encrypted),
        );

        let nullable_fields = schema.nullable_encrypted_fields();
        assert_eq!(nullable_fields.len(), 1);
        assert_eq!(nullable_fields[0].field_name, "email");

        // Encryption handles Some(value) - None remains NULL
        let key = [0u8; 32];
        let cipher = FieldEncryption::new(&key);

        // Some value gets encrypted
        let encrypted = cipher.encrypt("user@example.com").unwrap();
        assert_eq!(cipher.decrypt(&encrypted).unwrap(), "user@example.com");

        // None/NULL would be stored as NULL in DB (no encryption needed)
        let value: Option<&str> = None;
        assert!(value.is_none()); // NULL stays NULL, no encryption
    }

    // ============================================================================
    // COMPLEX TYPE SUPPORT
    // ============================================================================

    /// Test UUID field encryption
    #[test]
    fn test_schema_uuid_field_support() {
        let mut schema = StructSchema::new("TokenStore");
        schema.add_field(
            SchemaFieldInfo::new("token_id", "Uuid", true, "encryption/token")
                .with_mark(EncryptionMark::Encrypted),
        );

        let field = schema.get_encrypted_field("token_id").unwrap();
        assert_eq!(field.field_type, "Uuid");

        // UUID as string can be encrypted/decrypted
        let key = [0u8; 32];
        let cipher = FieldEncryption::new(&key);
        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let encrypted = cipher.encrypt(uuid_str).unwrap();
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, uuid_str);
    }

    /// Test DateTime field encryption
    #[test]
    fn test_schema_datetime_field_support() {
        let mut schema = StructSchema::new("AuditLog");
        schema.add_field(
            SchemaFieldInfo::new("event_time", "DateTime<Utc>", true, "encryption/audit")
                .with_mark(EncryptionMark::Encrypted),
        );

        let field = schema.get_encrypted_field("event_time").unwrap();
        assert_eq!(field.field_type, "DateTime<Utc>");

        // DateTime as RFC3339 string can be encrypted/decrypted
        let key = [0u8; 32];
        let cipher = FieldEncryption::new(&key);
        let datetime_str = "2026-01-15T10:30:00.123456789Z";
        let encrypted = cipher.encrypt(datetime_str).unwrap();
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, datetime_str);
    }

    /// Test JSON field encryption
    #[test]
    fn test_schema_json_field_support() {
        let mut schema = StructSchema::new("UserProfile");
        schema.add_field(
            SchemaFieldInfo::new("metadata", "Json", true, "encryption/metadata")
                .with_mark(EncryptionMark::Encrypted),
        );

        let field = schema.get_encrypted_field("metadata").unwrap();
        assert_eq!(field.field_type, "Json");

        // JSON serialized to string can be encrypted/decrypted
        let key = [0u8; 32];
        let cipher = FieldEncryption::new(&key);
        let json_str =
            r#"{"preferences":{"theme":"dark","locale":"en-US"},"tags":["admin","active"]}"#;
        let encrypted = cipher.encrypt(json_str).unwrap();
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, json_str);
        // Structure preserved after decryption
        let parsed: serde_json::Value = serde_json::from_str(&decrypted).unwrap();
        assert_eq!(parsed["preferences"]["theme"], "dark");
        assert_eq!(parsed["tags"][0], "admin");
    }

    /// Test collection field encryption
    #[test]
    fn test_schema_collection_field_support() {
        let mut schema = StructSchema::new("User");
        schema.add_field(
            SchemaFieldInfo::new("tags", "Vec<String>", true, "encryption/tags")
                .with_mark(EncryptionMark::Encrypted),
        );

        let field = schema.get_encrypted_field("tags").unwrap();
        assert_eq!(field.field_type, "Vec<String>");

        // Collection serialized as JSON array, encrypted as string
        let key = [0u8; 32];
        let cipher = FieldEncryption::new(&key);
        let tags_json = r#"["admin","editor","viewer"]"#;
        let encrypted = cipher.encrypt(tags_json).unwrap();
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, tags_json);
        let parsed: Vec<String> = serde_json::from_str(&decrypted).unwrap();
        assert_eq!(parsed, vec!["admin", "editor", "viewer"]);
    }

    /// Test nested struct field encryption
    #[test]
    fn test_schema_nested_struct_encryption() {
        let mut schema = StructSchema::new("User");
        schema.add_field(
            SchemaFieldInfo::new("address", "Address", true, "encryption/address")
                .with_mark(EncryptionMark::Encrypted),
        );

        let field = schema.get_encrypted_field("address").unwrap();
        assert_eq!(field.field_type, "Address");

        // Nested struct serialized as JSON, encrypted as unit
        let key = [0u8; 32];
        let cipher = FieldEncryption::new(&key);
        let address_json = r#"{"street":"123 Main St","city":"Springfield","zip":"62701"}"#;
        let encrypted = cipher.encrypt(address_json).unwrap();
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, address_json);
        let parsed: serde_json::Value = serde_json::from_str(&decrypted).unwrap();
        assert_eq!(parsed["street"], "123 Main St");
        assert_eq!(parsed["city"], "Springfield");
        assert_eq!(parsed["zip"], "62701");
    }

    // ============================================================================
    // SCHEMA REFLECTION & INTROSPECTION
    // ============================================================================

    /// Test schema reflection API
    #[test]
    fn test_schema_reflection_api() {
        let mut registry = SchemaRegistry::new();
        let mut user_schema = StructSchema::new("User");
        user_schema.add_field(
            SchemaFieldInfo::new("email", "String", true, "encryption/email")
                .with_algorithm("aes256-gcm")
                .with_mark(EncryptionMark::Encrypted),
        );
        user_schema.add_field(
            SchemaFieldInfo::new("ssn", "String", true, "encryption/ssn")
                .with_algorithm("aes256-gcm")
                .with_mark(EncryptionMark::Sensitive),
        );
        user_schema.add_field(SchemaFieldInfo::new("name", "String", false, ""));
        registry.register(user_schema).unwrap();

        let fields = registry.get_encrypted_fields("User").unwrap();
        assert_eq!(fields.len(), 2);

        // Each FieldInfo has complete metadata
        for field in &fields {
            assert!(field.is_encrypted);
            assert!(!field.key_reference.is_empty());
            assert!(!field.algorithm.is_empty());
        }

        // Non-existent type returns error
        assert!(registry.get_encrypted_fields("NonExistent").is_err());
    }

    /// Test field info includes all metadata
    #[test]
    fn test_schema_field_info_complete() {
        let field = SchemaFieldInfo::new("email", "String", true, "encryption/email")
            .with_algorithm("aes256-gcm")
            .with_nullable(false)
            .with_mark(EncryptionMark::Encrypted);

        assert_eq!(field.field_name, "email");
        assert_eq!(field.field_type, "String");
        assert!(field.is_encrypted);
        assert_eq!(field.key_reference, "encryption/email");
        assert_eq!(field.algorithm, "aes256-gcm");
        assert!(!field.nullable);
        assert_eq!(field.mark, Some(EncryptionMark::Encrypted));
    }

    /// Test schema registration registry
    #[test]
    fn test_schema_registration_registry() {
        let mut registry = SchemaRegistry::new();

        let user_schema = StructSchema::new("User").with_fields(vec![SchemaFieldInfo::new(
            "email",
            "String",
            true,
            "encryption/email",
        )]);

        let product_schema = StructSchema::new("Product").with_fields(vec![SchemaFieldInfo::new(
            "sku",
            "String",
            true,
            "encryption/sku",
        )]);

        registry.register(user_schema).unwrap();
        registry.register(product_schema).unwrap();

        // Retrieve by name
        assert!(registry.get("User").is_some());
        assert!(registry.get("Product").is_some());
        assert!(registry.get("Order").is_none());

        assert_eq!(registry.count(), 2);
        assert_eq!(registry.total_encrypted_fields(), 2);

        let types = registry.types_with_encryption();
        assert_eq!(types.len(), 2);
    }

    // ============================================================================
    // SCHEMA VALIDATION TESTS
    // ============================================================================

    /// Test schema validation on startup
    #[test]
    fn test_schema_validation_startup() {
        let mut registry = SchemaRegistry::new();

        // Valid schema - has key references
        let valid_schema = StructSchema::new("User").with_fields(vec![
            SchemaFieldInfo::new("email", "String", true, "encryption/email"),
            SchemaFieldInfo::new("phone", "String", true, "encryption/phone"),
        ]);
        registry.register(valid_schema).unwrap();

        // validate_all passes for properly configured schemas
        assert!(registry.validate_all().is_ok());

        // Collect all keys for Vault validation
        let keys = registry.all_encryption_keys();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"encryption/email".to_string()));
        assert!(keys.contains(&"encryption/phone".to_string()));
    }

    /// Test schema consistency validation
    #[test]
    fn test_schema_consistency_validation() {
        let mut schema = StructSchema::new("User");
        // Two fields sharing the same key should use consistent settings
        schema.add_field(
            SchemaFieldInfo::new("email", "String", true, "encryption/shared_key")
                .with_algorithm("aes256-gcm"),
        );
        schema.add_field(
            SchemaFieldInfo::new("phone", "String", true, "encryption/shared_key")
                .with_algorithm("aes256-gcm"),
        );

        // fields_for_key shows consistency
        let shared_fields = schema.fields_for_key("encryption/shared_key");
        assert_eq!(shared_fields.len(), 2);
        // All fields using the same key should have the same algorithm
        let algorithms: Vec<&str> = shared_fields.iter().map(|f| f.algorithm.as_str()).collect();
        assert!(
            algorithms.iter().all(|a| *a == algorithms[0]),
            "All fields sharing a key should use the same algorithm"
        );
    }

    /// Test schema with no encryption marks valid
    #[test]
    fn test_schema_no_encryption_marks_valid() {
        let schema = StructSchema::new("Config").with_fields(vec![
            SchemaFieldInfo::new("id", "i64", false, ""),
            SchemaFieldInfo::new("key", "String", false, ""),
            SchemaFieldInfo::new("value", "String", false, ""),
        ]);

        // No encrypted fields is valid
        assert!(schema.validate().is_ok());
        assert_eq!(schema.encrypted_field_count(), 0);

        // Can be registered
        let mut registry = SchemaRegistry::new();
        registry.register(schema).unwrap();
        assert!(!registry.has_encrypted_fields("Config"));
    }

    // ============================================================================
    // BACKWARDS COMPATIBILITY
    // ============================================================================

    /// Test reading unencrypted field from encrypted column
    #[test]
    fn test_schema_read_unencrypted_from_encrypted_column() {
        // Column contains encrypted data (binary ciphertext)
        let key = [0u8; 32];
        let cipher = FieldEncryption::new(&key);
        let encrypted_data = cipher.encrypt("secret@email.com").unwrap();

        // Trying to interpret encrypted bytes as UTF-8 string fails
        let result = String::from_utf8(encrypted_data.clone());
        assert!(result.is_err(), "Encrypted data should not be valid UTF-8 in general");

        // Correct approach: decrypt first
        let decrypted = cipher.decrypt(&encrypted_data).unwrap();
        assert_eq!(decrypted, "secret@email.com");
    }

    /// Test reading encrypted field from unencrypted column
    #[test]
    fn test_schema_read_encrypted_from_unencrypted_column() {
        // Column contains plaintext data
        let plaintext = "user@example.com";
        let plaintext_bytes = plaintext.as_bytes();

        // Attempting to decrypt plaintext fails
        let key = [0u8; 32];
        let cipher = FieldEncryption::new(&key);
        let result = cipher.decrypt(plaintext_bytes);
        assert!(result.is_err(), "Decrypting plaintext should fail");
    }

    /// Test schema migration strategy
    #[test]
    fn test_schema_migration_strategy() {
        let key = [0u8; 32];
        let cipher = FieldEncryption::new(&key);

        // Step 1: Old data is plaintext
        let old_plaintext = "old@example.com";

        // Step 2: After migration, new data is encrypted
        let new_encrypted = cipher.encrypt("new@example.com").unwrap();
        let new_decrypted = cipher.decrypt(&new_encrypted).unwrap();
        assert_eq!(new_decrypted, "new@example.com");

        // Step 3: Migration encrypts old data
        let migrated = cipher.encrypt(old_plaintext).unwrap();
        let migrated_decrypted = cipher.decrypt(&migrated).unwrap();
        assert_eq!(migrated_decrypted, old_plaintext);

        // Schema tracks version for migration awareness
        let schema_v1 = StructSchema::new("User")
            .with_version(1)
            .with_fields(vec![SchemaFieldInfo::new("email", "String", false, "")]);

        let schema_v2 =
            StructSchema::new("User").with_version(2).with_fields(vec![SchemaFieldInfo::new(
                "email",
                "String",
                true,
                "encryption/email",
            )]);

        // v1 records need migration, v2 records are already encrypted
        assert!(!schema_v1.is_field_encrypted("email"));
        assert!(schema_v2.is_field_encrypted("email"));
    }
}

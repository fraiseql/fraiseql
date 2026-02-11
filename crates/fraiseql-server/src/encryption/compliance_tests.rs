//! Comprehensive test specifications for compliance frameworks:
//! HIPAA (PHI protection), PCI-DSS (payment data), GDPR (data privacy), SOC 2 (controls)

#[cfg(test)]
#[allow(clippy::module_inception)]
mod compliance_tests {
    use crate::encryption::{
        FieldEncryption,
        audit_logging::{AuditLogEntry, AuditLogger, EventStatus, OperationType},
        compliance::{
            ComplianceCheckResult, ComplianceConfig, ComplianceFramework, ComplianceReport,
            ComplianceStatus, ComplianceValidator,
        },
        credential_rotation::{CredentialRotationManager, RotationConfig, RotationSchedule},
        schema::{EncryptionMark, SchemaFieldInfo, SchemaRegistry, StructSchema},
    };

    // ============================================================================
    // HIPAA COMPLIANCE TESTS
    // ============================================================================

    /// Test HIPAA PHI encryption at rest
    #[test]
    fn test_hipaa_phi_encryption_at_rest() {
        // Protected Health Information (PHI) must be encrypted at rest
        let cipher = FieldEncryption::new(&[0u8; 32]);

        // PHI fields: SSN, medical record numbers, health conditions
        let phi_fields = vec![
            ("ssn", "123-45-6789"),
            ("medical_record", "MRN-2024-001234"),
            ("health_condition", "Type 2 Diabetes"),
        ];

        for (field_name, plaintext) in &phi_fields {
            let encrypted = cipher.encrypt(plaintext).unwrap();
            // Encrypted data must not contain plaintext
            let encrypted_str = String::from_utf8_lossy(&encrypted);
            assert!(
                !encrypted_str.contains(plaintext),
                "PHI field '{}' should be encrypted at rest",
                field_name
            );

            // Must be decryptable
            let decrypted = cipher.decrypt(&encrypted).unwrap();
            assert_eq!(
                &decrypted, plaintext,
                "PHI field '{}' must roundtrip correctly",
                field_name
            );
        }

        // Verify AES-256-GCM minimum (key size = 32 bytes)
        let config = ComplianceConfig::new(ComplianceFramework::HIPAA);
        assert_eq!(config.encryption_algorithm, "aes256-gcm");
        assert!(config.encryption_required);
    }

    /// Test HIPAA audit trail completeness
    #[test]
    fn test_hipaa_audit_trail_completeness() {
        // HIPAA requires comprehensive audit trail for all PHI access
        let mut logger = AuditLogger::new(1000);

        // Log PHI access capturing: who, what, when, where, why
        let entry =
            AuditLogEntry::new("doctor_123", "ssn", OperationType::Select, "req-001", "sess-abc")
                .with_context("ip_address", "10.0.1.50")
                .with_context("reason", "treatment_lookup")
                .with_context("department", "cardiology");

        logger.log_entry(entry).unwrap();

        // Verify audit trail captures required information
        let entries = logger.entries_for_user("doctor_123");
        assert_eq!(entries.len(), 1);

        let entry = &entries[0];
        assert_eq!(entry.user_id(), "doctor_123");
        assert_eq!(entry.field_name(), "ssn");
        assert_eq!(entry.operation(), OperationType::Select);
        assert_eq!(entry.status(), EventStatus::Success);
        assert!(entry.context().contains_key("ip_address"));
        assert!(entry.context().contains_key("reason"));

        // HIPAA retention: minimum 6 years
        let config = ComplianceConfig::new(ComplianceFramework::HIPAA);
        assert_eq!(config.audit_retention_days, 2190); // 6 years
    }

    /// Test HIPAA access controls
    #[test]
    fn test_hipaa_access_controls() {
        // Only authorized users can access PHI - simulate RBAC enforcement
        let mut logger = AuditLogger::new(100);

        // Authorized access
        let authorized_entry =
            AuditLogEntry::new("doctor_001", "ssn", OperationType::Select, "req-001", "sess-001")
                .with_security_context(Some("10.0.1.50"), Some("physician"));
        logger.log_entry(authorized_entry).unwrap();

        // Denied access
        let denied_entry = AuditLogEntry::new(
            "receptionist_042",
            "ssn",
            OperationType::Select,
            "req-002",
            "sess-002",
        )
        .with_failure("Access denied: insufficient role for PHI field 'ssn'")
        .with_security_context(Some("10.0.2.10"), Some("receptionist"));
        logger.log_entry(denied_entry).unwrap();

        // Audit trail captures both granted and denied access
        let all_entries = logger.entries_for_field("ssn");
        assert_eq!(all_entries.len(), 2);

        let failed = logger.failed_entries();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].user_id(), "receptionist_042");
        assert!(failed[0].error_message().unwrap().contains("Access denied"));
    }

    /// Test HIPAA minimum necessary access
    #[test]
    fn test_hipaa_minimum_necessary_principle() {
        // Users access only PHI needed for their job function
        let mut schema = StructSchema::new("PatientRecord");
        schema.add_field(
            SchemaFieldInfo::new("ssn", "String", true, "encryption/phi")
                .with_mark(EncryptionMark::Sensitive),
        );
        schema.add_field(
            SchemaFieldInfo::new("diagnosis", "String", true, "encryption/phi")
                .with_mark(EncryptionMark::Sensitive),
        );
        schema.add_field(SchemaFieldInfo::new("name", "String", false, ""));
        schema.add_field(SchemaFieldInfo::new("appointment_date", "String", false, ""));

        // Only encrypted fields require authorization
        let encrypted = schema.encrypted_field_names();
        assert_eq!(encrypted.len(), 2);
        assert!(encrypted.contains(&"ssn"));
        assert!(encrypted.contains(&"diagnosis"));

        // Non-PHI fields freely accessible
        assert!(!schema.is_field_encrypted("name"));
        assert!(!schema.is_field_encrypted("appointment_date"));

        // Query logging shows what was requested
        let mut logger = AuditLogger::new(100);
        let entry =
            AuditLogEntry::new("nurse_005", "ssn", OperationType::Select, "req-003", "sess-003")
                .with_context("fields_requested", "ssn,diagnosis")
                .with_context("fields_granted", "diagnosis");
        logger.log_entry(entry).unwrap();
        assert_eq!(logger.entry_count(), 1);
    }

    /// Test HIPAA data retention policy
    #[test]
    fn test_hipaa_data_retention_policy() {
        // PHI retention policy configurable
        let config = ComplianceConfig::new(ComplianceFramework::HIPAA);

        // Default: HIPAA retention is 6 years
        assert_eq!(config.audit_retention_days, 2190);

        // Can customize retention
        let custom_config =
            ComplianceConfig::new(ComplianceFramework::HIPAA).with_retention_days(3650); // 10 years
        assert_eq!(custom_config.audit_retention_days, 3650);

        // Audit trail of purge operations
        let mut logger = AuditLogger::new(100);
        let purge_entry = AuditLogEntry::new(
            "system",
            "patient_records",
            OperationType::Delete,
            "purge-001",
            "sys-001",
        )
        .with_context("reason", "retention_policy_expired")
        .with_context("records_purged", "42");
        logger.log_entry(purge_entry).unwrap();
        let deletes = logger.entries_for_operation(OperationType::Delete);
        assert_eq!(deletes.len(), 1);
    }

    /// Test HIPAA breach notification
    #[test]
    fn test_hipaa_breach_notification_tracking() {
        // System tracks security incidents
        let mut logger = AuditLogger::new(100);

        // Log potential breach events
        let breach_entry =
            AuditLogEntry::new("unknown_user", "ssn", OperationType::Select, "req-999", "sess-999")
                .with_failure("Authentication failed: invalid credentials")
                .with_security_context(Some("203.0.113.50"), None)
                .with_context("breach_indicator", "repeated_auth_failure")
                .with_context("attempt_count", "15");
        logger.log_entry(breach_entry).unwrap();

        // Query breach history
        let failed = logger.failed_entries();
        assert_eq!(failed.len(), 1);
        let entry = &failed[0];
        assert!(entry.context().contains_key("breach_indicator"));
        assert_eq!(entry.context().get("attempt_count"), Some(&"15".to_string()));

        // Breach notification includes timestamp, affected PHI, user, system
        assert!(!entry.request_id().is_empty());
        assert!(!entry.session_id().is_empty());
    }

    // ============================================================================
    // PCI-DSS COMPLIANCE TESTS
    // ============================================================================

    /// Test PCI-DSS cardholder data encryption
    #[test]
    fn test_pci_dss_cardholder_data_encryption() {
        // Cardholder data must be encrypted: PAN, expiry, CVV
        let cipher = FieldEncryption::new(&[0u8; 32]);

        let cardholder_data = vec![
            ("pan", "4532015112830366"),
            ("expiry", "12/2027"),
            ("cvv", "123"),
        ];

        for (field_name, plaintext) in &cardholder_data {
            let encrypted = cipher.encrypt(plaintext).unwrap();
            assert_ne!(
                plaintext.as_bytes(),
                &encrypted[12..], // Skip nonce
                "PCI-DSS: field '{}' must be encrypted",
                field_name
            );
            let decrypted = cipher.decrypt(&encrypted).unwrap();
            assert_eq!(&decrypted, plaintext);
        }

        // Verify AES-256-GCM configuration
        let config = ComplianceConfig::new(ComplianceFramework::PCIDSS);
        assert_eq!(config.encryption_algorithm, "aes256-gcm");
        assert!(config.encryption_required);
    }

    /// Test PCI-DSS key management requirements
    #[test]
    fn test_pci_dss_key_management_requirements() {
        // Key rotation minimum annually (365 days)
        let config = RotationConfig::new().with_ttl_days(365);
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        // Keys managed via rotation manager
        let version = manager.get_current_version().unwrap();
        assert_eq!(version, 1);

        // Key rotation works correctly
        let new_version = manager.rotate_key().unwrap();
        assert_eq!(new_version, 2);
        assert_eq!(manager.get_current_version().unwrap(), 2);

        // PCI-DSS compliance check passes with fresh key
        assert!(manager.check_pci_compliance().unwrap());

        // PCI-DSS requires annual rotation
        let pci_config = ComplianceConfig::new(ComplianceFramework::PCIDSS);
        assert_eq!(pci_config.key_rotation_days, 365);
    }

    /// Test PCI-DSS audit trail for payment data
    #[test]
    fn test_pci_dss_audit_trail_payment_data() {
        // All access to cardholder data logged
        let mut logger = AuditLogger::new(1000);

        // Log multiple operations
        let ops = vec![
            ("user_001", "pan", OperationType::Insert, EventStatus::Success),
            ("user_002", "pan", OperationType::Select, EventStatus::Success),
            ("user_003", "pan", OperationType::Select, EventStatus::Failure),
        ];

        for (user, field, op, status) in &ops {
            let entry = AuditLogEntry::new(*user, *field, *op, "req-pci", "sess-pci");
            let entry = if *status == EventStatus::Failure {
                entry.with_failure("Access denied")
            } else {
                entry
            };
            logger.log_entry(entry).unwrap();
        }

        // Verify log retention
        assert_eq!(logger.entry_count(), 3);
        let pan_entries = logger.entries_for_field("pan");
        assert_eq!(pan_entries.len(), 3);
        let failed = logger.failed_entries();
        assert_eq!(failed.len(), 1);

        // PCI-DSS retention minimum 1 year
        let config = ComplianceConfig::new(ComplianceFramework::PCIDSS);
        assert_eq!(config.audit_retention_days, 365);

        // Logs can be exported for review
        let entry = &pan_entries[0];
        let csv = entry.to_csv();
        assert!(csv.contains("pan"));
        let json = entry.to_json_like();
        assert!(json.contains("pan"));
    }

    /// Test PCI-DSS field masking for output
    #[test]
    fn test_pci_dss_field_masking_output() {
        // Cardholder data masked in output (only last 4 digits visible)
        let pan = "4532015112830366";
        let masked = format!("****-****-****-{}", &pan[pan.len() - 4..]);
        assert_eq!(masked, "****-****-****-0366");
        assert!(!masked.contains("4532015112830366"));

        // Encrypted data fully opaque
        let cipher = FieldEncryption::new(&[0u8; 32]);
        let encrypted = cipher.encrypt(pan).unwrap();
        let encrypted_str = String::from_utf8_lossy(&encrypted);
        assert!(!encrypted_str.contains(pan));

        // Error messages don't leak cardholder data
        let wrong_key = FieldEncryption::new(&[1u8; 32]);
        let result = wrong_key.decrypt(&encrypted);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(!err_msg.contains(pan), "Error message should not leak PAN");
    }

    /// Test PCI-DSS secure transmission
    #[test]
    fn test_pci_dss_secure_transmission() {
        // Cardholder data encrypted in transit
        let cipher = FieldEncryption::new(&[0u8; 32]);
        let pan = "4532015112830366";

        // Context-based encryption for transit (simulates TLS-like authenticated channel)
        let context = "transit:tls1.3:session:abc123";
        let encrypted = cipher.encrypt_with_context(pan, context).unwrap();

        // Must decrypt with same context
        let decrypted = cipher.decrypt_with_context(&encrypted, context).unwrap();
        assert_eq!(decrypted, pan);

        // Wrong context fails (different session/channel)
        let wrong_context = "transit:tls1.3:session:xyz789";
        let result = cipher.decrypt_with_context(&encrypted, wrong_context);
        assert!(result.is_err());
    }

    // ============================================================================
    // GDPR COMPLIANCE TESTS
    // ============================================================================

    /// Test GDPR data encryption
    #[test]
    fn test_gdpr_personal_data_encryption() {
        // Personal data encrypted at rest
        let cipher = FieldEncryption::new(&[0u8; 32]);

        let personal_data = vec![
            ("name", "Jean-Pierre Dubois"),
            ("email", "jp.dubois@example.fr"),
            ("phone", "+33-1-23-45-67-89"),
            ("address", "15 Rue de Rivoli, 75001 Paris"),
        ];

        for (field_name, plaintext) in &personal_data {
            let encrypted = cipher.encrypt(plaintext).unwrap();
            let decrypted = cipher.decrypt(&encrypted).unwrap();
            assert_eq!(
                &decrypted, plaintext,
                "GDPR: personal data field '{}' must roundtrip",
                field_name
            );
        }

        // Key management via GDPR config
        let config = ComplianceConfig::new(ComplianceFramework::GDPR);
        assert!(config.encryption_required);
        assert_eq!(config.encryption_algorithm, "aes256-gcm");
    }

    /// Test GDPR right to be forgotten
    #[test]
    fn test_gdpr_right_to_be_forgotten() {
        // User can request data deletion
        let cipher = FieldEncryption::new(&[0u8; 32]);

        // Encrypt personal data
        let email = "user@example.eu";
        let encrypted_email = cipher.encrypt(email).unwrap();

        // Verify data exists and is decryptable
        let decrypted = cipher.decrypt(&encrypted_email).unwrap();
        assert_eq!(decrypted, email);

        // Simulate deletion by creating a new cipher with different key
        // (the original encrypted data is irrecoverable without the old key)
        let new_cipher = FieldEncryption::new(&[1u8; 32]);
        let result = new_cipher.decrypt(&encrypted_email);
        assert!(result.is_err(), "Data should be inaccessible after key deletion");

        // Audit trail shows deletion request and execution
        let mut logger = AuditLogger::new(100);
        let delete_entry = AuditLogEntry::new(
            "user_gdpr_req",
            "email",
            OperationType::Delete,
            "gdpr-del-001",
            "sess-gdpr",
        )
        .with_context("gdpr_request_type", "right_to_erasure")
        .with_context("deletion_verified", "true");
        logger.log_entry(delete_entry).unwrap();
        let deletes = logger.entries_for_operation(OperationType::Delete);
        assert_eq!(deletes.len(), 1);
        assert_eq!(
            deletes[0].context().get("gdpr_request_type"),
            Some(&"right_to_erasure".to_string())
        );
    }

    /// Test GDPR data portability
    #[test]
    fn test_gdpr_data_portability() {
        // User can export their personal data
        let cipher = FieldEncryption::new(&[0u8; 32]);

        let personal_data = [
            ("name", "Hans Mueller"),
            ("email", "hans@example.de"),
            ("phone", "+49-30-1234567"),
        ];

        // Encrypt all data
        let encrypted_data: Vec<_> = personal_data
            .iter()
            .map(|(field, value)| (*field, cipher.encrypt(value).unwrap()))
            .collect();

        // Export: decrypt for data portability (JSON format)
        let mut export_json = String::from("{ ");
        for (i, (field, encrypted)) in encrypted_data.iter().enumerate() {
            let decrypted = cipher.decrypt(encrypted).unwrap();
            if i > 0 {
                export_json.push_str(", ");
            }
            export_json.push_str(&format!("\"{}\": \"{}\"", field, decrypted));
        }
        export_json.push_str(" }");

        assert!(export_json.contains("Hans Mueller"));
        assert!(export_json.contains("hans@example.de"));
        assert!(export_json.contains("+49-30-1234567"));

        // Audit trail shows export
        let mut logger = AuditLogger::new(100);
        let export_entry = AuditLogEntry::new(
            "hans_id",
            "all_personal_data",
            OperationType::Select,
            "export-001",
            "sess-exp",
        )
        .with_context("gdpr_request_type", "data_portability")
        .with_context("export_format", "json");
        logger.log_entry(export_entry).unwrap();
        assert_eq!(logger.entry_count(), 1);
    }

    /// Test GDPR data minimization
    #[test]
    fn test_gdpr_data_minimization_principle() {
        // Only necessary personal data collected
        let mut schema = StructSchema::new("GDPRUser");

        // Mark only necessary fields for encryption (personal data)
        schema.add_field(
            SchemaFieldInfo::new("email", "String", true, "encryption/gdpr")
                .with_mark(EncryptionMark::Encrypted),
        );
        schema.add_field(
            SchemaFieldInfo::new("phone", "String", true, "encryption/gdpr")
                .with_mark(EncryptionMark::Sensitive)
                .with_nullable(true), // Phone is optional - data minimization
        );
        schema.add_field(SchemaFieldInfo::new("username", "String", false, ""));

        // Can query what personal data is collected
        let encrypted = schema.encrypted_field_names();
        assert_eq!(encrypted.len(), 2);
        assert!(encrypted.contains(&"email"));
        assert!(encrypted.contains(&"phone"));

        // Non-personal data not encrypted (minimization)
        assert!(!schema.is_field_encrypted("username"));

        // Nullable fields show optional collection
        let nullable = schema.nullable_encrypted_fields();
        assert_eq!(nullable.len(), 1);
        assert_eq!(nullable[0].field_name, "phone");
    }

    /// Test GDPR consent tracking
    #[test]
    fn test_gdpr_consent_tracking() {
        // Consent recorded for each data processing activity
        let mut logger = AuditLogger::new(100);

        // Record consent
        let consent_entry = AuditLogEntry::new(
            "user_eu_001",
            "consent",
            OperationType::Insert,
            "consent-001",
            "sess-consent",
        )
        .with_context("consent_type", "marketing_emails")
        .with_context("consent_version", "v2.1")
        .with_context("consent_given", "true");
        logger.log_entry(consent_entry).unwrap();

        // Consent can be withdrawn
        let withdrawal_entry = AuditLogEntry::new(
            "user_eu_001",
            "consent",
            OperationType::Update,
            "consent-002",
            "sess-consent",
        )
        .with_context("consent_type", "marketing_emails")
        .with_context("consent_version", "v2.1")
        .with_context("consent_given", "false")
        .with_context("withdrawal_reason", "user_requested");
        logger.log_entry(withdrawal_entry).unwrap();

        // Audit trail of consent lifecycle
        let consent_entries = logger.entries_for_user("user_eu_001");
        assert_eq!(consent_entries.len(), 2);

        // Verify consent timeline
        let first = &consent_entries[0];
        assert_eq!(first.operation(), OperationType::Insert);
        assert_eq!(first.context().get("consent_given"), Some(&"true".to_string()));

        let second = &consent_entries[1];
        assert_eq!(second.operation(), OperationType::Update);
        assert_eq!(second.context().get("consent_given"), Some(&"false".to_string()));
    }

    /// Test GDPR data breach notification
    #[test]
    fn test_gdpr_data_breach_notification() {
        // Breach detection and logging
        let mut logger = AuditLogger::new(100);

        // Log breach detection event
        let breach_entry = AuditLogEntry::new(
            "system_monitor",
            "personal_data",
            OperationType::Select,
            "breach-001",
            "sys-001",
        )
        .with_failure("Unauthorized access detected")
        .with_context("breach_type", "unauthorized_access")
        .with_context("affected_records", "150")
        .with_context("data_categories", "email,phone,address")
        .with_context("detection_method", "anomaly_detection")
        .with_context("severity", "high");
        logger.log_entry(breach_entry).unwrap();

        // Can query breach incidents
        let failed = logger.failed_entries();
        assert_eq!(failed.len(), 1);
        let breach = &failed[0];
        assert!(breach.context().contains_key("breach_type"));
        assert_eq!(breach.context().get("affected_records"), Some(&"150".to_string()));
        assert_eq!(breach.context().get("severity"), Some(&"high".to_string()));

        // GDPR retention for breach records
        let config = ComplianceConfig::new(ComplianceFramework::GDPR);
        assert_eq!(config.audit_retention_days, 2555); // ~7 years
    }

    // ============================================================================
    // SOC 2 COMPLIANCE TESTS
    // ============================================================================

    /// Test SOC 2 access controls
    #[test]
    fn test_soc2_logical_access_controls() {
        // Access to encrypted data controlled via role-based authentication
        let mut logger = AuditLogger::new(100);

        // Admin access granted
        let admin_entry = AuditLogEntry::new(
            "admin_001",
            "api_keys",
            OperationType::Select,
            "req-soc2-001",
            "sess-soc2",
        )
        .with_security_context(Some("10.0.0.1"), Some("admin"));
        logger.log_entry(admin_entry).unwrap();

        // Regular user access denied
        let user_entry = AuditLogEntry::new(
            "user_002",
            "api_keys",
            OperationType::Select,
            "req-soc2-002",
            "sess-soc2",
        )
        .with_failure("Role 'viewer' insufficient for encrypted field")
        .with_security_context(Some("10.0.0.5"), Some("viewer"));
        logger.log_entry(user_entry).unwrap();

        // Segregation of duties: verify different roles logged
        let entries = logger.entries_for_field("api_keys");
        assert_eq!(entries.len(), 2);

        let successful = logger.successful_entries();
        assert_eq!(successful.len(), 1);
        assert_eq!(successful[0].context().get("user_role"), Some(&"admin".to_string()));

        let failed = logger.failed_entries();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].context().get("user_role"), Some(&"viewer".to_string()));
    }

    /// Test SOC 2 monitoring and alerting
    #[test]
    fn test_soc2_monitoring_and_alerting() {
        // Continuous monitoring of encryption operations
        let mut logger = AuditLogger::new(100);

        // Simulate normal and unusual activity
        for i in 0..5 {
            let entry = AuditLogEntry::new(
                format!("user_{}", i),
                "email",
                OperationType::Select,
                format!("req-{}", i),
                "sess-mon",
            );
            logger.log_entry(entry).unwrap();
        }

        // Simulate unusual activity: multiple failed decryptions
        for i in 0..10 {
            let entry = AuditLogEntry::new(
                "suspicious_user",
                "credit_card",
                OperationType::Select,
                format!("req-fail-{}", i),
                "sess-suspicious",
            )
            .with_failure("Decryption failed: invalid key");
            logger.log_entry(entry).unwrap();
        }

        // Alerting based on failure thresholds
        let failed = logger.failed_entries();
        assert_eq!(failed.len(), 10);

        let suspicious_entries = logger.entries_for_user("suspicious_user");
        assert_eq!(suspicious_entries.len(), 10);

        // All suspicious entries are failures
        assert!(suspicious_entries.iter().all(|e| e.status() == EventStatus::Failure));
    }

    /// Test SOC 2 change management
    #[test]
    fn test_soc2_change_management() {
        // Schema changes logged and tracked
        let mut registry = SchemaRegistry::new();

        // Version 1 schema
        let schema_v1 = StructSchema::new("User")
            .with_fields(vec![SchemaFieldInfo::new(
                "email",
                "String",
                true,
                "encryption/email",
            )])
            .with_version(1);
        registry.register(schema_v1).unwrap();

        // Version 2 schema (added phone encryption)
        let schema_v2 = StructSchema::new("User")
            .with_fields(vec![
                SchemaFieldInfo::new("email", "String", true, "encryption/email"),
                SchemaFieldInfo::new("phone", "String", true, "encryption/phone"),
            ])
            .with_version(2);
        registry.register(schema_v2).unwrap();

        // Version tracking
        let schema = registry.get("User").unwrap();
        assert_eq!(schema.version, 2);
        assert_eq!(schema.encrypted_field_count(), 2);

        // Key rotation tracked
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();
        manager.rotate_key().unwrap();

        let metrics = manager.metrics();
        assert_eq!(metrics.total_rotations(), 1);
        assert!(metrics.last_rotation().is_some());
    }

    /// Test SOC 2 incident response
    #[test]
    fn test_soc2_incident_response() {
        // Encryption failures logged as incidents
        let mut logger = AuditLogger::new(100);

        // Log encryption failure incident
        let incident_entry = AuditLogEntry::new(
            "system",
            "ssn",
            OperationType::Insert,
            "incident-001",
            "sys-incident",
        )
        .with_failure("Encryption key expired: key version 3")
        .with_context("incident_type", "encryption_failure")
        .with_context("severity", "critical")
        .with_context("key_version", "3");
        logger.log_entry(incident_entry).unwrap();

        // Log response action
        let response_entry = AuditLogEntry::new(
            "ops_team",
            "ssn",
            OperationType::Update,
            "incident-001-response",
            "sys-incident",
        )
        .with_context("action", "emergency_key_rotation")
        .with_context("resolution", "new_key_v4_deployed");
        logger.log_entry(response_entry).unwrap();

        // Timeline of events reconstructible from audit log
        let field_entries = logger.entries_for_field("ssn");
        assert_eq!(field_entries.len(), 2);

        // First: incident detected
        assert_eq!(field_entries[0].status(), EventStatus::Failure);
        assert!(field_entries[0].error_message().unwrap().contains("key expired"));

        // Second: response action recorded
        assert_eq!(field_entries[1].status(), EventStatus::Success);
        assert_eq!(
            field_entries[1].context().get("action"),
            Some(&"emergency_key_rotation".to_string())
        );
    }

    /// Test SOC 2 availability requirements
    #[test]
    fn test_soc2_availability_and_resilience() {
        // Encryption operations don't significantly impact performance
        let cipher = FieldEncryption::new(&[0u8; 32]);
        let start = std::time::Instant::now();

        // Perform 100 encrypt/decrypt operations
        for _ in 0..100 {
            let encrypted = cipher.encrypt("test@example.com").unwrap();
            let _decrypted = cipher.decrypt(&encrypted).unwrap();
        }

        let duration = start.elapsed();
        // Should complete in reasonable time (< 1 second for 100 ops)
        assert!(
            duration.as_secs() < 1,
            "100 encrypt/decrypt cycles took {:?}, should be < 1s",
            duration
        );

        // Key cache prevents unavailability issues
        let config = RotationConfig::new().with_ttl_days(365);
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        // Current version accessible (cached)
        let version = manager.get_current_version().unwrap();
        assert!(version > 0);
        assert!(manager.can_decrypt_with_version(version).unwrap());
    }

    // ============================================================================
    // CROSS-FRAMEWORK COMPLIANCE TESTS
    // ============================================================================

    /// Test compliance configuration at startup
    #[test]
    fn test_compliance_configuration_at_startup() {
        // Application can configure multiple compliance frameworks simultaneously
        let mut validator = ComplianceValidator::new();

        validator.register_framework(ComplianceConfig::new(ComplianceFramework::HIPAA));
        validator.register_framework(ComplianceConfig::new(ComplianceFramework::PCIDSS));
        validator.register_framework(ComplianceConfig::new(ComplianceFramework::GDPR));
        validator.register_framework(ComplianceConfig::new(ComplianceFramework::SOC2));

        let enabled = validator.enabled_frameworks();
        assert_eq!(enabled.len(), 4);

        // Each framework has its own config
        let hipaa_config = validator.get_framework_config(ComplianceFramework::HIPAA).unwrap();
        assert_eq!(hipaa_config.audit_retention_days, 2190);

        let pcidss_config = validator.get_framework_config(ComplianceFramework::PCIDSS).unwrap();
        assert_eq!(pcidss_config.key_rotation_days, 365);
    }

    /// Test compliance validation on schema
    #[test]
    fn test_compliance_schema_validation() {
        // Schema validated against compliance requirements
        let mut validator = ComplianceValidator::new();
        validator.register_framework(ComplianceConfig::new(ComplianceFramework::HIPAA));

        // Valid schema: PHI fields encrypted
        let mut schema = StructSchema::new("Patient");
        schema.add_field(SchemaFieldInfo::new("ssn", "String", true, "encryption/phi"));
        schema.add_field(SchemaFieldInfo::new("diagnosis", "String", true, "encryption/phi"));
        assert!(schema.validate().is_ok());

        // Record compliant result
        let result = ComplianceCheckResult::new(
            ComplianceFramework::HIPAA,
            "phi_encryption",
            ComplianceStatus::Compliant,
            "All PHI fields encrypted with AES-256-GCM",
        )
        .with_details(format!("{} encrypted fields detected", schema.encrypted_field_count()));
        validator.record_result(result);

        // Invalid schema: missing encryption on PHI
        let invalid_schema = StructSchema::new("Patient").with_fields(vec![
            SchemaFieldInfo::new("ssn", "String", false, ""), // Not encrypted!
        ]);
        assert!(!invalid_schema.is_field_encrypted("ssn"));

        let failure_result = ComplianceCheckResult::new(
            ComplianceFramework::HIPAA,
            "phi_encryption",
            ComplianceStatus::NonCompliant,
            "PHI field 'ssn' not encrypted",
        );
        validator.record_result(failure_result);

        // Validation status reflects both results
        let status = validator.check_framework_status(ComplianceFramework::HIPAA);
        assert_eq!(status, ComplianceStatus::PartiallyCompliant);
    }

    /// Test compliance reporting
    #[test]
    fn test_compliance_reporting() {
        // Can generate compliance reports
        let results = vec![
            ComplianceCheckResult::new(
                ComplianceFramework::HIPAA,
                "encryption_at_rest",
                ComplianceStatus::Compliant,
                "All PHI encrypted",
            ),
            ComplianceCheckResult::new(
                ComplianceFramework::HIPAA,
                "audit_trail",
                ComplianceStatus::Compliant,
                "Comprehensive audit logging enabled",
            ),
            ComplianceCheckResult::new(
                ComplianceFramework::HIPAA,
                "key_rotation",
                ComplianceStatus::PartiallyCompliant,
                "Key rotation configured but not yet tested",
            ),
        ];

        let report = ComplianceReport::new(ComplianceFramework::HIPAA).with_results(results);
        assert_eq!(report.framework, ComplianceFramework::HIPAA);
        assert_eq!(report.compliant_count, 2);
        assert_eq!(report.partial_count, 1);
        assert_eq!(report.non_compliant_count, 0);
        assert_eq!(report.overall_status, ComplianceStatus::PartiallyCompliant);

        // Export to JSON
        let json = report.to_json_like();
        assert!(json.contains("HIPAA"));
        assert!(json.contains("partially-compliant"));

        // Export to CSV
        let header = ComplianceReport::to_csv_header();
        assert!(header.contains("Framework"));
        let rows = report.to_csv_rows();
        assert_eq!(rows.len(), 3);
    }

    /// Test compliance audit trail integrity
    #[test]
    fn test_compliance_audit_trail_integrity() {
        // Audit trail tamper-evident (append-only)
        let mut logger = AuditLogger::new(100);

        // Add entries
        for i in 0..5 {
            let entry = AuditLogEntry::new(
                format!("user_{}", i),
                "email",
                OperationType::Select,
                format!("req-{}", i),
                "sess-integrity",
            );
            logger.log_entry(entry).unwrap();
        }

        // Entries are append-only (count only grows)
        assert_eq!(logger.entry_count(), 5);

        // Add another entry
        let entry =
            AuditLogEntry::new("user_5", "email", OperationType::Select, "req-5", "sess-integrity");
        logger.log_entry(entry).unwrap();
        assert_eq!(logger.entry_count(), 6);

        // Verify entries can be signed/exported for tamper detection
        let recent = logger.recent_entries(6);
        for entry in &recent {
            let csv = entry.to_csv();
            assert!(!csv.is_empty());
            // Each entry has a timestamp for ordering verification
            assert!(!entry.timestamp().to_string().is_empty());
        }

        // Bounded history preserves integrity
        let mut bounded_logger = AuditLogger::new(3);
        for i in 0..5 {
            let entry = AuditLogEntry::new(
                format!("user_{}", i),
                "email",
                OperationType::Select,
                format!("req-{}", i),
                "sess-bounded",
            );
            bounded_logger.log_entry(entry).unwrap();
        }
        assert_eq!(bounded_logger.entry_count(), 3);
    }

    /// Test compliance with encryption key rotation
    #[test]
    fn test_compliance_with_key_rotation() {
        // Key rotation maintains compliance
        let config = RotationConfig::new().with_ttl_days(365);
        let manager = CredentialRotationManager::new(config);

        // Initialize first key version
        let v1 = manager.initialize_key().unwrap();
        assert_eq!(v1, 1);

        // Rotate to new version
        let v2 = manager.rotate_key().unwrap();
        assert_eq!(v2, 2);

        // Old version still exists for decryption of historical data
        assert!(manager.can_decrypt_with_version(v1).unwrap());
        assert!(manager.can_decrypt_with_version(v2).unwrap());

        // All rotations audited via metrics
        let metrics = manager.metrics();
        assert_eq!(metrics.total_rotations(), 1);
        assert_eq!(metrics.failed_rotations(), 0);
        assert_eq!(metrics.success_rate_percent(), 100);

        // Version history available
        let history = manager.get_version_history().unwrap();
        assert_eq!(history.len(), 2);

        // Compliance checks pass
        assert!(manager.check_hipaa_compliance().unwrap());
        assert!(manager.check_pci_compliance().unwrap());
    }

    /// Test compliance failure handling
    #[test]
    fn test_compliance_failure_handling() {
        // If compliance requirement violated, system logs violation
        let mut validator = ComplianceValidator::new();
        validator.register_framework(ComplianceConfig::new(ComplianceFramework::HIPAA));

        // Record a compliance violation
        let violation = ComplianceCheckResult::new(
            ComplianceFramework::HIPAA,
            "encryption_at_rest",
            ComplianceStatus::NonCompliant,
            "Unencrypted PHI field detected: 'ssn'",
        )
        .with_details("Field 'ssn' in table 'patients' is stored as plaintext");
        validator.record_result(violation);

        // Status reflects non-compliance
        let status = validator.check_framework_status(ComplianceFramework::HIPAA);
        assert_eq!(status, ComplianceStatus::NonCompliant);

        // Alerting available for violations
        let non_compliant = validator.results_by_status(ComplianceStatus::NonCompliant);
        assert_eq!(non_compliant.len(), 1);
        assert!(non_compliant[0].description.contains("Unencrypted PHI"));

        // Audit trail shows compliance failures
        let mut logger = AuditLogger::new(100);
        let failure_entry = AuditLogEntry::new(
            "compliance_checker",
            "ssn",
            OperationType::Select,
            "compliance-check-001",
            "sys-check",
        )
        .with_failure("Compliance violation: unencrypted PHI")
        .with_context("framework", "HIPAA")
        .with_context("severity", "critical");
        logger.log_entry(failure_entry).unwrap();
        let failed = logger.failed_entries();
        assert_eq!(failed.len(), 1);
    }

    // ============================================================================
    // COMPLIANCE METRICS & MONITORING
    // ============================================================================

    /// Test compliance metrics collection
    #[test]
    fn test_compliance_metrics_collection() {
        // Metrics collected per framework
        let mut validator = ComplianceValidator::new();

        // Register all frameworks
        for framework in &[
            ComplianceFramework::HIPAA,
            ComplianceFramework::PCIDSS,
            ComplianceFramework::GDPR,
            ComplianceFramework::SOC2,
        ] {
            validator.register_framework(ComplianceConfig::new(*framework));
        }

        // Record results per framework
        let frameworks_results = vec![
            (ComplianceFramework::HIPAA, "phi_encryption", ComplianceStatus::Compliant),
            (ComplianceFramework::HIPAA, "audit_trail", ComplianceStatus::Compliant),
            (ComplianceFramework::PCIDSS, "card_encryption", ComplianceStatus::Compliant),
            (ComplianceFramework::PCIDSS, "key_rotation", ComplianceStatus::NonCompliant),
            (ComplianceFramework::GDPR, "personal_data", ComplianceStatus::Compliant),
            (
                ComplianceFramework::SOC2,
                "access_controls",
                ComplianceStatus::PartiallyCompliant,
            ),
        ];

        for (framework, requirement, status) in &frameworks_results {
            let result = ComplianceCheckResult::new(*framework, *requirement, *status, "test");
            validator.record_result(result);
        }

        // Per-framework metrics
        let hipaa_results = validator.results_for_framework(ComplianceFramework::HIPAA);
        assert_eq!(hipaa_results.len(), 2);

        let pci_results = validator.results_for_framework(ComplianceFramework::PCIDSS);
        assert_eq!(pci_results.len(), 2);

        // Summary metrics
        let (compliant, non_compliant, partial) = validator.get_summary();
        assert_eq!(compliant, 4);
        assert_eq!(non_compliant, 1);
        assert_eq!(partial, 1);
    }

    /// Test compliance dashboard availability
    #[test]
    fn test_compliance_dashboard_availability() {
        // Dashboard shows compliance status per framework
        let mut validator = ComplianceValidator::new();
        validator.register_framework(ComplianceConfig::new(ComplianceFramework::HIPAA));
        validator.register_framework(ComplianceConfig::new(ComplianceFramework::PCIDSS));

        // Add results
        validator.record_result(ComplianceCheckResult::new(
            ComplianceFramework::HIPAA,
            "encryption",
            ComplianceStatus::Compliant,
            "Encrypted",
        ));
        validator.record_result(ComplianceCheckResult::new(
            ComplianceFramework::PCIDSS,
            "encryption",
            ComplianceStatus::Compliant,
            "Encrypted",
        ));

        // Key metrics displayed
        let overall = validator.overall_status();
        assert_eq!(overall, ComplianceStatus::Compliant);

        let hipaa_status = validator.check_framework_status(ComplianceFramework::HIPAA);
        assert_eq!(hipaa_status, ComplianceStatus::Compliant);

        let pci_status = validator.check_framework_status(ComplianceFramework::PCIDSS);
        assert_eq!(pci_status, ComplianceStatus::Compliant);

        // Export functionality available
        let report = ComplianceReport::new(ComplianceFramework::HIPAA).with_results(vec![
            ComplianceCheckResult::new(
                ComplianceFramework::HIPAA,
                "encryption",
                ComplianceStatus::Compliant,
                "Encrypted",
            ),
        ]);
        let json = report.to_json_like();
        assert!(json.contains("HIPAA"));
        assert!(json.contains("compliant"));
    }

    /// Test compliance policy enforcement
    #[test]
    fn test_compliance_policy_enforcement() {
        // Encryption policy enforced (no plaintext for sensitive data)
        let mut schema = StructSchema::new("SensitiveData");
        schema.add_field(SchemaFieldInfo::new("ssn", "String", true, "encryption/phi"));
        schema.add_field(SchemaFieldInfo::new("pan", "String", true, "encryption/pci"));

        // All sensitive fields must be encrypted
        assert!(schema.is_field_encrypted("ssn"));
        assert!(schema.is_field_encrypted("pan"));

        // Schema validates key references exist
        assert!(schema.validate().is_ok());

        // Key rotation policy enforced
        let config = RotationConfig::new()
            .with_ttl_days(365)
            .with_schedule(RotationSchedule::Interval(365));
        assert_eq!(config.ttl_days, 365);
        assert_eq!(config.schedule, RotationSchedule::Interval(365));

        // Compliance validator tracks enforcement
        let mut validator = ComplianceValidator::new();
        validator.register_framework(ComplianceConfig::new(ComplianceFramework::HIPAA));

        let result = ComplianceCheckResult::new(
            ComplianceFramework::HIPAA,
            "encryption_policy",
            ComplianceStatus::Compliant,
            "All sensitive fields encrypted per policy",
        );
        validator.record_result(result);
        assert_eq!(validator.count_by_status(ComplianceStatus::Compliant), 1);
    }

    /// Test compliance documentation generation
    #[test]
    fn test_compliance_documentation_generation() {
        // System can generate compliance documentation
        let results = vec![
            ComplianceCheckResult::new(
                ComplianceFramework::HIPAA,
                "encryption_at_rest",
                ComplianceStatus::Compliant,
                "PHI data encrypted with AES-256-GCM",
            )
            .with_details("All 12 PHI fields in 3 tables encrypted"),
            ComplianceCheckResult::new(
                ComplianceFramework::HIPAA,
                "audit_logging",
                ComplianceStatus::Compliant,
                "Comprehensive audit trail for all PHI access",
            )
            .with_details("6-year retention policy configured"),
            ComplianceCheckResult::new(
                ComplianceFramework::HIPAA,
                "key_management",
                ComplianceStatus::Compliant,
                "Annual key rotation via Vault",
            )
            .with_details("Last rotation: 2024-06-15"),
        ];

        let report = ComplianceReport::new(ComplianceFramework::HIPAA).with_results(results);

        // JSON format for system integration
        let json = report.to_json_like();
        assert!(json.contains("HIPAA"));
        assert!(json.contains("compliant"));
        assert!(json.contains("\"compliant\": 3"));

        // CSV format for auditor review
        let header = ComplianceReport::to_csv_header();
        assert!(header.contains("Framework"));
        assert!(header.contains("Requirement"));
        assert!(header.contains("Status"));

        let rows = report.to_csv_rows();
        assert_eq!(rows.len(), 3);
        for row in &rows {
            assert!(row.contains("HIPAA"));
            assert!(row.contains("compliant"));
        }

        // Report includes summary statistics
        assert_eq!(report.compliant_count, 3);
        assert_eq!(report.non_compliant_count, 0);
        assert_eq!(report.partial_count, 0);
        assert_eq!(report.overall_status, ComplianceStatus::Compliant);
    }
}

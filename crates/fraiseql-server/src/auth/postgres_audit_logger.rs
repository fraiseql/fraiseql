// PostgreSQL-backed audit logger for authentication events
//
// Implements the AuditLogger trait with persistent storage to the audit_log table.
// Uses a background task with a bounded channel to bridge the sync AuditLogger trait
// with async database operations.

use sqlx::postgres::PgPool;
use tokio::sync::mpsc;
use tracing::{error, warn};

use crate::auth::audit_logger::{AuditEntry, AuditLogger};

/// PostgreSQL-backed audit logger for compliance-grade authentication event persistence.
///
/// Writes audit entries to the `audit_log` table (migration 0010) via a background
/// tokio task. The sync `log_entry` method sends entries through a bounded channel,
/// ensuring the caller is never blocked on database I/O.
///
/// # HMAC Integrity
///
/// Each entry is signed with HMAC-SHA256 using a server-side key stored in the
/// `metadata` JSONB column. This allows tamper detection during compliance audits.
pub struct PostgresAuditLogger {
    sender: mpsc::Sender<AuditEntry>,
}

impl PostgresAuditLogger {
    /// Create a new PostgreSQL audit logger.
    ///
    /// Spawns a background task that drains the channel and writes entries to the database.
    ///
    /// # Arguments
    /// * `pool` - PostgreSQL connection pool
    /// * `hmac_key` - HMAC key for entry integrity signatures
    /// * `buffer_size` - Channel buffer size (default: 1024)
    pub fn new(pool: PgPool, hmac_key: Vec<u8>, buffer_size: usize) -> Self {
        let (sender, receiver) = mpsc::channel(buffer_size);
        let writer = AuditWriter { pool, hmac_key };
        tokio::spawn(writer.run(receiver));
        Self { sender }
    }

    /// Create with default buffer size of 1024.
    pub fn with_defaults(pool: PgPool, hmac_key: Vec<u8>) -> Self {
        Self::new(pool, hmac_key, 1024)
    }
}

impl AuditLogger for PostgresAuditLogger {
    fn log_entry(&self, entry: AuditEntry) {
        if let Err(e) = self.sender.try_send(entry) {
            match e {
                mpsc::error::TrySendError::Full(_) => {
                    warn!(
                        "Audit log channel full, dropping entry — consider increasing buffer size"
                    );
                },
                mpsc::error::TrySendError::Closed(_) => {
                    error!("Audit log background writer has stopped — entries will be lost");
                },
            }
        }
    }
}

/// Background writer that drains the channel and persists entries to PostgreSQL.
struct AuditWriter {
    pool:     PgPool,
    hmac_key: Vec<u8>,
}

impl AuditWriter {
    async fn run(self, mut receiver: mpsc::Receiver<AuditEntry>) {
        while let Some(entry) = receiver.recv().await {
            if let Err(e) = self.write_entry(&entry).await {
                error!(
                    event_type = entry.event_type.as_str(),
                    error = %e,
                    "Failed to persist audit entry to database"
                );
            }
        }
    }

    async fn write_entry(&self, entry: &AuditEntry) -> std::result::Result<(), sqlx::Error> {
        let event_type = entry.event_type.as_str();
        let secret_type = entry.secret_type.as_str();
        let status = if entry.success { "success" } else { "failure" };
        let hmac_signature = compute_hmac(&self.hmac_key, entry);

        let metadata = serde_json::json!({
            "secret_type": secret_type,
            "context": entry.context,
            "hmac_signature": hmac_signature,
        });

        sqlx::query(
            r"
            INSERT INTO audit_log (
                event_type, user_id, action, status,
                error_message, resource_type, metadata
            ) VALUES ($1, $2, $3, $4, $5, $6, $7)
            ",
        )
        .bind(event_type)
        .bind(entry.subject.as_deref())
        .bind(&entry.operation)
        .bind(status)
        .bind(entry.error_message.as_deref())
        .bind("auth")
        .bind(metadata)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

/// Compute HMAC-SHA256 signature for an audit entry.
///
/// Signs canonical fields in deterministic order for tamper detection.
fn compute_hmac(key: &[u8], entry: &AuditEntry) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;

    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");

    // Sign the canonical fields in a deterministic order
    mac.update(entry.event_type.as_str().as_bytes());
    mac.update(entry.secret_type.as_str().as_bytes());
    if let Some(ref subject) = entry.subject {
        mac.update(subject.as_bytes());
    }
    mac.update(entry.operation.as_bytes());
    mac.update(if entry.success { b"1" } else { b"0" });
    if let Some(ref err) = entry.error_message {
        mac.update(err.as_bytes());
    }

    let result = mac.finalize();
    hex::encode(result.into_bytes())
}

/// Query interface for reading audit entries from PostgreSQL.
///
/// Separated from the logger to keep the write path simple and the read path
/// flexible for compliance dashboards and investigations.
pub struct AuditLogQuery {
    pool: PgPool,
}

/// A persisted audit log row returned from queries.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuditLogRow {
    pub id:            uuid::Uuid,
    pub timestamp:     chrono::DateTime<chrono::Utc>,
    pub event_type:    String,
    pub user_id:       Option<String>,
    pub action:        String,
    pub status:        String,
    pub error_message: Option<String>,
    pub resource_type: Option<String>,
    pub metadata:      serde_json::Value,
}

impl AuditLogQuery {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Query recent audit entries, most recent first.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub async fn recent(&self, limit: i64) -> std::result::Result<Vec<AuditLogRow>, sqlx::Error> {
        let rows = sqlx::query_as::<_, AuditLogRow>(
            r"
            SELECT id, timestamp, event_type, user_id, action,
                   status, error_message, resource_type, metadata
            FROM audit_log
            ORDER BY timestamp DESC
            LIMIT $1
            ",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    /// Query audit entries by event type.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub async fn by_event_type(
        &self,
        event_type: &str,
        limit: i64,
    ) -> std::result::Result<Vec<AuditLogRow>, sqlx::Error> {
        let rows = sqlx::query_as::<_, AuditLogRow>(
            r"
            SELECT id, timestamp, event_type, user_id, action,
                   status, error_message, resource_type, metadata
            FROM audit_log
            WHERE event_type = $1
            ORDER BY timestamp DESC
            LIMIT $2
            ",
        )
        .bind(event_type)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    /// Query audit entries by user ID.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub async fn by_user(
        &self,
        user_id: &str,
        limit: i64,
    ) -> std::result::Result<Vec<AuditLogRow>, sqlx::Error> {
        let rows = sqlx::query_as::<_, AuditLogRow>(
            r"
            SELECT id, timestamp, event_type, user_id, action,
                   status, error_message, resource_type, metadata
            FROM audit_log
            WHERE user_id = $1
            ORDER BY timestamp DESC
            LIMIT $2
            ",
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    /// Count entries matching a status (e.g., "failure" for security reviews).
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub async fn count_by_status(&self, status: &str) -> std::result::Result<i64, sqlx::Error> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_log WHERE status = $1")
            .bind(status)
            .fetch_one(&self.pool)
            .await?;

        Ok(row.0)
    }
}

/// Verify HMAC integrity of an audit log row.
///
/// Re-computes the HMAC from the stored fields and compares against the stored
/// signature using constant-time comparison.
pub fn verify_row_hmac(row: &AuditLogRow, hmac_key: &[u8]) -> bool {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;

    let stored_sig = row.metadata.get("hmac_signature").and_then(|v| v.as_str());

    let Some(stored_sig) = stored_sig else {
        return false;
    };

    let mut mac = HmacSha256::new_from_slice(hmac_key).expect("HMAC accepts any key length");

    // Must match the signing order in compute_hmac
    mac.update(row.event_type.as_bytes());

    // secret_type is stored in metadata
    if let Some(secret_type) = row.metadata.get("secret_type").and_then(|v| v.as_str()) {
        mac.update(secret_type.as_bytes());
    }

    if let Some(ref user_id) = row.user_id {
        mac.update(user_id.as_bytes());
    }
    mac.update(row.action.as_bytes());
    mac.update(if row.status == "success" { b"1" } else { b"0" });
    if let Some(ref err) = row.error_message {
        mac.update(err.as_bytes());
    }

    let expected = hex::encode(mac.finalize().into_bytes());
    subtle::ConstantTimeEq::ct_eq(expected.as_bytes(), stored_sig.as_bytes()).into()
}

// Implement sqlx::FromRow manually since we need specific column mappings
impl<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> for AuditLogRow {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> std::result::Result<Self, sqlx::Error> {
        use sqlx::Row;
        Ok(Self {
            id:            row.try_get("id")?,
            timestamp:     row.try_get("timestamp")?,
            event_type:    row.try_get("event_type")?,
            user_id:       row.try_get("user_id")?,
            action:        row.try_get("action")?,
            status:        row.try_get("status")?,
            error_message: row.try_get("error_message")?,
            resource_type: row.try_get("resource_type")?,
            metadata:      row.try_get("metadata")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::audit_logger::{AuditEventType, SecretType};

    fn make_entry(
        event_type: AuditEventType,
        secret_type: SecretType,
        subject: Option<&str>,
        operation: &str,
        success: bool,
        error_message: Option<&str>,
    ) -> AuditEntry {
        AuditEntry {
            event_type,
            secret_type,
            subject: subject.map(String::from),
            operation: operation.to_string(),
            success,
            error_message: error_message.map(String::from),
            context: None,
        }
    }

    fn make_row_from_entry(entry: &AuditEntry, hmac_key: &[u8]) -> AuditLogRow {
        let sig = compute_hmac(hmac_key, entry);
        AuditLogRow {
            id:            uuid::Uuid::new_v4(),
            timestamp:     chrono::Utc::now(),
            event_type:    entry.event_type.as_str().to_string(),
            user_id:       entry.subject.clone(),
            action:        entry.operation.clone(),
            status:        if entry.success {
                "success".to_string()
            } else {
                "failure".to_string()
            },
            error_message: entry.error_message.clone(),
            resource_type: Some("auth".to_string()),
            metadata:      serde_json::json!({
                "secret_type": entry.secret_type.as_str(),
                "context": entry.context,
                "hmac_signature": sig,
            }),
        }
    }

    #[test]
    fn test_hmac_deterministic() {
        let key = b"test-secret-key";
        let entry = make_entry(
            AuditEventType::JwtValidation,
            SecretType::JwtToken,
            Some("user123"),
            "validate",
            true,
            None,
        );

        let sig1 = compute_hmac(key, &entry);
        let sig2 = compute_hmac(key, &entry);
        assert_eq!(sig1, sig2, "HMAC should be deterministic");
    }

    #[test]
    fn test_hmac_different_for_different_entries() {
        let key = b"test-secret-key";

        let entry1 = make_entry(
            AuditEventType::JwtValidation,
            SecretType::JwtToken,
            Some("user123"),
            "validate",
            true,
            None,
        );

        let entry2 = make_entry(
            AuditEventType::AuthFailure,
            SecretType::JwtToken,
            Some("user456"),
            "validate",
            false,
            Some("Token expired"),
        );

        let sig1 = compute_hmac(key, &entry1);
        let sig2 = compute_hmac(key, &entry2);
        assert_ne!(sig1, sig2, "Different entries should produce different HMACs");
    }

    #[test]
    fn test_hmac_different_keys() {
        let entry = make_entry(
            AuditEventType::JwtValidation,
            SecretType::JwtToken,
            Some("user123"),
            "validate",
            true,
            None,
        );

        let sig1 = compute_hmac(b"key-one", &entry);
        let sig2 = compute_hmac(b"key-two", &entry);
        assert_ne!(sig1, sig2, "Different keys should produce different HMACs");
    }

    #[test]
    fn test_hmac_format() {
        let entry = AuditEntry {
            event_type:    AuditEventType::OauthCallback,
            secret_type:   SecretType::AuthorizationCode,
            subject:       Some("user@example.com".to_string()),
            operation:     "exchange".to_string(),
            success:       true,
            error_message: None,
            context:       Some("provider=google".to_string()),
        };

        let sig = compute_hmac(b"test-secret-key", &entry);
        assert_eq!(sig.len(), 64, "SHA256 HMAC hex should be 64 characters");
        assert!(sig.chars().all(|c| c.is_ascii_hexdigit()), "HMAC should be valid hex");
    }

    #[test]
    fn test_hmac_verification_roundtrip() {
        let hmac_key = b"compliance-key-2024";
        let entry = make_entry(
            AuditEventType::SessionTokenCreated,
            SecretType::SessionToken,
            Some("alice"),
            "create",
            true,
            None,
        );

        let row = make_row_from_entry(&entry, hmac_key);
        assert!(verify_row_hmac(&row, hmac_key), "HMAC should verify correctly");
    }

    #[test]
    fn test_hmac_verification_detects_tampering() {
        let hmac_key = b"compliance-key-2024";
        let entry = make_entry(
            AuditEventType::AuthFailure,
            SecretType::JwtToken,
            Some("attacker"),
            "validate",
            false,
            Some("Invalid signature"),
        );

        let mut row = make_row_from_entry(&entry, hmac_key);
        // Tamper: change status from "failure" to "success"
        row.status = "success".to_string();

        assert!(!verify_row_hmac(&row, hmac_key), "HMAC should reject tampered data");
    }

    #[test]
    fn test_hmac_verification_fails_wrong_key() {
        let entry = make_entry(
            AuditEventType::JwtValidation,
            SecretType::JwtToken,
            Some("user123"),
            "validate",
            true,
            None,
        );

        let row = make_row_from_entry(&entry, b"correct-key");
        assert!(!verify_row_hmac(&row, b"wrong-key"), "HMAC should reject wrong key");
    }

    #[test]
    fn test_hmac_verification_fails_missing_signature() {
        let row = AuditLogRow {
            id:            uuid::Uuid::new_v4(),
            timestamp:     chrono::Utc::now(),
            event_type:    "jwt_validation".to_string(),
            user_id:       Some("user123".to_string()),
            action:        "validate".to_string(),
            status:        "success".to_string(),
            error_message: None,
            resource_type: Some("auth".to_string()),
            metadata:      serde_json::json!({}), // No HMAC signature
        };

        assert!(!verify_row_hmac(&row, b"any-key"), "Should fail when signature is missing");
    }

    #[tokio::test]
    async fn test_logger_handles_closed_channel() {
        let (sender, receiver) = mpsc::channel(1);
        drop(receiver);

        let logger = PostgresAuditLogger { sender };

        // Should not panic, just log a warning
        logger.log_entry(make_entry(
            AuditEventType::JwtValidation,
            SecretType::JwtToken,
            Some("user123"),
            "validate",
            true,
            None,
        ));
    }

    #[test]
    fn test_hmac_with_all_fields() {
        let key = b"full-test-key";
        let entry = AuditEntry {
            event_type:    AuditEventType::OidcTokenExchange,
            secret_type:   SecretType::AuthorizationCode,
            subject:       Some("service-account@project.iam".to_string()),
            operation:     "exchange".to_string(),
            success:       false,
            error_message: Some("invalid_grant: code already used".to_string()),
            context:       Some("provider=google,attempt=2".to_string()),
        };

        let sig = compute_hmac(key, &entry);
        assert_eq!(sig.len(), 64);

        let row = make_row_from_entry(&entry, key);
        assert!(verify_row_hmac(&row, key));
    }

    #[test]
    fn test_hmac_with_no_optional_fields() {
        let key = b"minimal-key";
        let entry = make_entry(
            AuditEventType::CsrfStateGenerated,
            SecretType::CsrfToken,
            None,
            "generate",
            true,
            None,
        );

        let sig = compute_hmac(key, &entry);
        assert_eq!(sig.len(), 64);

        let row = make_row_from_entry(&entry, key);
        assert!(verify_row_hmac(&row, key));
    }

    #[test]
    fn test_hmac_detects_user_id_tampering() {
        let key = b"tamper-detect-key";
        let entry = make_entry(
            AuditEventType::AuthSuccess,
            SecretType::SessionToken,
            Some("legitimate-user"),
            "login",
            true,
            None,
        );

        let mut row = make_row_from_entry(&entry, key);
        row.user_id = Some("admin".to_string()); // Tamper user ID

        assert!(!verify_row_hmac(&row, key), "HMAC should detect user_id tampering");
    }

    #[test]
    fn test_hmac_detects_event_type_tampering() {
        let key = b"tamper-detect-key";
        let entry = make_entry(
            AuditEventType::AuthFailure,
            SecretType::JwtToken,
            Some("user123"),
            "validate",
            false,
            Some("bad token"),
        );

        let mut row = make_row_from_entry(&entry, key);
        row.event_type = "auth_success".to_string(); // Tamper event type

        assert!(!verify_row_hmac(&row, key), "HMAC should detect event_type tampering");
    }

    #[test]
    fn test_hmac_detects_error_message_tampering() {
        let key = b"tamper-detect-key";
        let entry = make_entry(
            AuditEventType::AuthFailure,
            SecretType::JwtToken,
            Some("user123"),
            "validate",
            false,
            Some("Unauthorized access to admin panel"),
        );

        let mut row = make_row_from_entry(&entry, key);
        row.error_message = None; // Remove incriminating error

        assert!(!verify_row_hmac(&row, key), "HMAC should detect error_message tampering");
    }

    #[tokio::test]
    async fn test_logger_handles_full_channel() {
        let (sender, _receiver) = mpsc::channel(1);

        let logger = PostgresAuditLogger { sender };

        // Fill the channel
        logger.log_entry(make_entry(
            AuditEventType::JwtValidation,
            SecretType::JwtToken,
            Some("user1"),
            "validate",
            true,
            None,
        ));

        // This should not panic — it drops the entry with a warning
        logger.log_entry(make_entry(
            AuditEventType::JwtValidation,
            SecretType::JwtToken,
            Some("user2"),
            "validate",
            true,
            None,
        ));
    }
}

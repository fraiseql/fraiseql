//! Webhook audit exporter.
//!
//! Streams [`AuditEntry`] records as JSON over HTTPS to a configurable URL.
//! Entries are accumulated in a batch buffer and flushed periodically or when
//! `batch_size` is reached. Feature-gated behind `audit-webhook`.
//!
//! # SSRF protection
//!
//! Uses [`crate::http::client::build_ssrf_safe_client`] to prevent redirect-
//! chain attacks and enforce HTTPS-only connections.
//!
//! # Failure handling
//!
//! Export failures are logged but never block the primary PostgreSQL write path.
//! Failed batches are retried with exponential backoff (max 3 attempts).

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use parking_lot::Mutex;
use reqwest::Client;
use tracing::{error, warn};

use super::audit::{AuditEntry, AuditError, AuditExporter, WebhookExportConfig};
use crate::http::client::build_ssrf_safe_client;

/// Maximum number of retry attempts for failed webhook deliveries.
const MAX_RETRIES: u32 = 3;

/// Base delay for exponential backoff (doubled on each retry).
const BASE_RETRY_DELAY: Duration = Duration::from_millis(500);

/// Webhook timeout for each HTTP request.
const WEBHOOK_TIMEOUT: Duration = Duration::from_secs(10);

/// Webhook audit exporter that POSTs JSON batches to a configurable URL.
pub struct WebhookAuditExporter {
    client: Client,
    config: WebhookExportConfig,
    pub(crate) buffer: Arc<Mutex<Vec<AuditEntry>>>,
}

impl WebhookAuditExporter {
    /// Create a new webhook audit exporter from configuration.
    ///
    /// # Errors
    ///
    /// Returns [`AuditError::Export`] if the SSRF-safe HTTP client cannot be built.
    pub fn new(config: &WebhookExportConfig) -> Result<Self, AuditError> {
        let client = build_ssrf_safe_client(WEBHOOK_TIMEOUT)
            .map_err(|e| AuditError::Export(format!("webhook client init: {e}")))?;

        Ok(Self {
            client,
            config: config.clone(),
            buffer: Arc::new(Mutex::new(Vec::with_capacity(config.batch_size))),
        })
    }

    /// Send a batch of entries to the webhook URL with retry logic.
    async fn send_batch(&self, entries: &[AuditEntry]) -> Result<(), AuditError> {
        if entries.is_empty() {
            return Ok(());
        }

        let mut request = self.client.post(&self.config.url).json(entries);

        for (key, value) in &self.config.headers {
            request = request.header(key.as_str(), value.as_str());
        }

        let mut last_err = None;
        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                let delay = BASE_RETRY_DELAY * 2u32.saturating_pow(attempt - 1);
                tokio::time::sleep(delay).await;
                warn!(attempt, "Retrying webhook audit export");
            }

            match request.try_clone() {
                Some(req) => match req.send().await {
                    Ok(resp) if resp.status().is_success() => return Ok(()),
                    Ok(resp) => {
                        let status = resp.status();
                        last_err = Some(format!("HTTP {status}"));
                    },
                    Err(e) => {
                        last_err = Some(e.to_string());
                    },
                },
                None => {
                    return Err(AuditError::Export(
                        "webhook request could not be cloned for retry".to_string(),
                    ));
                },
            }
        }

        Err(AuditError::Export(format!(
            "webhook export failed after {MAX_RETRIES} attempts: {}",
            last_err.unwrap_or_default()
        )))
    }
}

#[async_trait]
impl AuditExporter for WebhookAuditExporter {
    async fn export(&self, entry: &AuditEntry) -> Result<(), AuditError> {
        let should_flush = {
            let mut buf = self.buffer.lock();
            buf.push(entry.clone());
            buf.len() >= self.config.batch_size
        };

        if should_flush {
            self.flush().await?;
        }

        Ok(())
    }

    async fn flush(&self) -> Result<(), AuditError> {
        let entries: Vec<AuditEntry> = {
            let mut buf = self.buffer.lock();
            std::mem::take(&mut *buf)
        };

        if entries.is_empty() {
            return Ok(());
        }

        if let Err(e) = self.send_batch(&entries).await {
            error!(error = %e, count = entries.len(), "Failed to flush webhook audit batch");
            return Err(e);
        }

        Ok(())
    }
}

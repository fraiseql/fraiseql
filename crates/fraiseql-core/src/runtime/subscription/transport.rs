use async_trait::async_trait;

use super::{SubscriptionError, types::SubscriptionEvent};

// =============================================================================
// Transport Adapters
// =============================================================================

/// Transport adapter trait for delivering subscription events.
///
/// Transport adapters are responsible for delivering events to external systems.
/// Each adapter implements a specific delivery mechanism (HTTP, Kafka, etc.).
///
/// # Implementors
///
/// - [`super::WebhookAdapter`] - HTTP POST delivery with retry logic
/// - [`super::KafkaAdapter`] - Apache Kafka event streaming
// Reason: used as dyn Trait (Box<dyn TransportAdapter>); async_trait ensures Send bounds and
// dyn-compatibility async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
pub trait TransportAdapter: Send + Sync {
    /// Deliver an event to the transport.
    ///
    /// # Arguments
    ///
    /// * `event` - The subscription event to deliver
    /// * `subscription_name` - Name of the subscription
    ///
    /// # Returns
    ///
    /// `Ok(())` on successful delivery, `Err` on failure.
    async fn deliver(
        &self,
        event: &SubscriptionEvent,
        subscription_name: &str,
    ) -> Result<(), SubscriptionError>;

    /// Get the adapter name for logging/metrics.
    fn name(&self) -> &'static str;

    /// Check if the adapter is healthy/connected.
    async fn health_check(&self) -> bool;
}

/// Type alias for boxed dynamic transport adapter.
///
/// Used to store transport adapters without generic type parameters.
pub type BoxDynTransportAdapter = Box<dyn TransportAdapter>;

/// Multi-transport delivery manager.
///
/// Manages multiple transport adapters and delivers events to all configured
/// destinations in parallel.
///
/// # Example
///
/// ```no_run
/// // Requires: live transport destination (webhook/NATS/etc).
/// use fraiseql_core::runtime::subscription::{
///     TransportManager, WebhookAdapter, WebhookTransportConfig, SubscriptionEvent,
/// };
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// # let event: SubscriptionEvent = unimplemented!();
/// let mut manager = TransportManager::new();
///
/// // Add webhook adapter
/// let webhook = WebhookAdapter::new(WebhookTransportConfig::new("https://api.example.com/events"))?;
/// manager.add_adapter(Box::new(webhook));
///
/// // Deliver to all transports
/// manager.deliver_all(&event, "orderCreated").await?;
/// # Ok(())
/// # }
/// ```
pub struct TransportManager {
    adapters: Vec<BoxDynTransportAdapter>,
}

impl TransportManager {
    /// Create a new transport manager.
    #[must_use]
    pub fn new() -> Self {
        Self {
            adapters: Vec::new(),
        }
    }

    /// Add a transport adapter.
    pub fn add_adapter(&mut self, adapter: BoxDynTransportAdapter) {
        tracing::info!(adapter = adapter.name(), "Added transport adapter");
        self.adapters.push(adapter);
    }

    /// Get the number of configured adapters.
    #[must_use]
    pub fn adapter_count(&self) -> usize {
        self.adapters.len()
    }

    /// Check if there are no adapters configured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.adapters.is_empty()
    }

    /// Deliver an event to all configured transports.
    ///
    /// Delivers in parallel and collects results. Delivery failures are accumulated
    /// in [`DeliveryResult::errors`] rather than propagated as `Err`.
    ///
    /// # Errors
    ///
    /// Currently infallible — always returns `Ok`. Individual adapter failures are
    /// captured in [`DeliveryResult::errors`] and do not short-circuit the method.
    pub async fn deliver_all(
        &self,
        event: &SubscriptionEvent,
        subscription_name: &str,
    ) -> Result<DeliveryResult, SubscriptionError> {
        if self.adapters.is_empty() {
            return Ok(DeliveryResult {
                successful: 0,
                failed: 0,
                errors: Vec::new(),
            });
        }

        let futures: Vec<_> = self
            .adapters
            .iter()
            .map(|adapter| {
                let name = adapter.name().to_string();
                async move {
                    let result = adapter.deliver(event, subscription_name).await;
                    (name, result)
                }
            })
            .collect();

        let results = futures::future::join_all(futures).await;

        let mut successful = 0;
        let mut failed = 0;
        let mut errors = Vec::new();

        for (name, result) in results {
            match result {
                Ok(()) => successful += 1,
                Err(e) => {
                    failed += 1;
                    errors.push((name, e.to_string()));
                },
            }
        }

        Ok(DeliveryResult {
            successful,
            failed,
            errors,
        })
    }

    /// Check health of all adapters.
    pub async fn health_check_all(&self) -> Vec<(String, bool)> {
        let futures: Vec<_> = self
            .adapters
            .iter()
            .map(|adapter| {
                let name = adapter.name().to_string();
                async move {
                    let healthy = adapter.health_check().await;
                    (name, healthy)
                }
            })
            .collect();

        futures::future::join_all(futures).await
    }
}

impl Default for TransportManager {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for TransportManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransportManager")
            .field("adapter_count", &self.adapters.len())
            .finish()
    }
}

/// Result of delivering an event to multiple transports.
#[derive(Debug, Clone)]
pub struct DeliveryResult {
    /// Number of successful deliveries.
    pub successful: usize,
    /// Number of failed deliveries.
    pub failed: usize,
    /// Errors from failed deliveries (adapter name, error message).
    pub errors: Vec<(String, String)>,
}

impl DeliveryResult {
    /// Check if all deliveries succeeded.
    #[must_use]
    pub const fn all_succeeded(&self) -> bool {
        self.failed == 0
    }

    /// Check if at least one delivery succeeded.
    #[must_use]
    pub const fn any_succeeded(&self) -> bool {
        self.successful > 0
    }
}

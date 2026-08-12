//! Outbound CDC drain runtime (#382).
//!
//! Builds one [`fraiseql_cdc_sinks::DrainWorker`] per configured
//! sink and ticks it on the server's `JoinSet`. The engine itself —
//! anti-join enqueue bounded by a commit-lag window, claim-then-publish under
//! a lease, head-of-line ordering guard, dead-lettering — lives in
//! `fraiseql-cdc-sinks` and is used unchanged; this module is only the missing
//! wiring between `[cdc_outbound]` TOML and that engine.
//!
//! Boot is fail-loud throughout: a configured section with no database pool, a
//! sink whose broker will not connect, or delivery-state DDL that will not
//! apply all refuse to start. The alternative — booting with a drain that
//! never runs — is silent data loss for every downstream consumer.

#[cfg(test)]
mod tests;

use std::time::Duration;

use fraiseql_cdc_sinks::{
    CdcSink, CdcSinkConfig, ChangeEvent, DrainWorker, NatsJetStreamSink, PublishOutcome, SinkKind,
};
use sqlx::PgPool;
use tracing::{error, info, warn};

use crate::server_config::cdc_outbound::{CdcOutboundConfig, CdcSinkSectionConfig};

/// Every sink kind this binary can drain to, as one concrete type.
///
/// `CdcSink::publish` is an RPITIT (`-> impl Future + Send`), which makes the
/// trait **not** dyn-safe — `Box<dyn CdcSink>` will not compile. So the drain
/// worker is made generic over this enum and each method delegates, rather than
/// over a trait object. Variants are feature-gated, so a build without
/// `cdc-kafka` carries no rdkafka and no dead arm.
// Reason: exactly one value per configured sink, built at boot and held by its
// DrainWorker for the process lifetime — it is never moved in a hot path or
// stored in a collection, so boxing would buy an allocation and a deref per
// publish and save nothing.
#[allow(clippy::large_enum_variant)]
pub enum ConfiguredSink {
    /// NATS `JetStream` (always available with `cdc-outbound`).
    NatsJetStream(NatsJetStreamSink),
    /// Apache Kafka (feature `cdc-kafka`).
    #[cfg(feature = "cdc-kafka")]
    Kafka(fraiseql_cdc_sinks::KafkaSink),
    /// AWS Kinesis Data Streams (feature `cdc-kinesis`).
    #[cfg(feature = "cdc-kinesis")]
    Kinesis(fraiseql_cdc_sinks::KinesisSink),
}

impl CdcSink for ConfiguredSink {
    fn name(&self) -> &str {
        match self {
            Self::NatsJetStream(sink) => sink.name(),
            #[cfg(feature = "cdc-kafka")]
            Self::Kafka(sink) => sink.name(),
            #[cfg(feature = "cdc-kinesis")]
            Self::Kinesis(sink) => sink.name(),
        }
    }

    fn kind(&self) -> SinkKind {
        match self {
            Self::NatsJetStream(sink) => sink.kind(),
            #[cfg(feature = "cdc-kafka")]
            Self::Kafka(sink) => sink.kind(),
            #[cfg(feature = "cdc-kinesis")]
            Self::Kinesis(sink) => sink.kind(),
        }
    }

    fn matches(&self, ev: &ChangeEvent) -> bool {
        match self {
            Self::NatsJetStream(sink) => sink.matches(ev),
            #[cfg(feature = "cdc-kafka")]
            Self::Kafka(sink) => sink.matches(ev),
            #[cfg(feature = "cdc-kinesis")]
            Self::Kinesis(sink) => sink.matches(ev),
        }
    }

    async fn publish(&self, ev: &ChangeEvent) -> PublishOutcome {
        match self {
            Self::NatsJetStream(sink) => sink.publish(ev).await,
            #[cfg(feature = "cdc-kafka")]
            Self::Kafka(sink) => sink.publish(ev).await,
            #[cfg(feature = "cdc-kinesis")]
            Self::Kinesis(sink) => sink.publish(ev).await,
        }
    }
}

/// One configured sink's drain worker, ready to be ticked.
pub struct SinkDrain {
    name:   String,
    worker: DrainWorker<ConfiguredSink>,
    tick:   Duration,
}

impl SinkDrain {
    /// Drain forever: tick, log the outcome, sleep, repeat.
    ///
    /// A tick that fails is logged and retried on the next tick rather than
    /// ending the loop — the delivery state is durable, so a transient
    /// database or broker fault costs latency, never events. Shutdown is by
    /// task abort (the server's `JoinSet` owns this future).
    pub async fn run_forever(self) {
        let mut ticker = tokio::time::interval(self.tick);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        info!(sink = %self.name, tick_secs = self.tick.as_secs(), "cdc outbound drain started");
        loop {
            ticker.tick().await;
            match self.worker.tick().await {
                Ok(stats) => {
                    if stats.dead > 0 {
                        warn!(
                            sink = %self.name,
                            dead = stats.dead,
                            published = stats.published,
                            "cdc drain dead-lettered rows"
                        );
                    }
                    if stats.late_recovered > 0 {
                        warn!(
                            sink = %self.name,
                            late_recovered = stats.late_recovered,
                            "cdc drain recovered rows that committed after the lag window"
                        );
                    }
                },
                Err(error) => {
                    // Never silent: a drain that cannot run is the failure
                    // mode downstream consumers cannot see for themselves.
                    error!(
                        sink = %self.name,
                        %error,
                        "cdc drain tick failed — retrying on the next tick"
                    );
                },
            }
        }
    }
}

/// Build one drain per configured sink, or `Ok(None)` when `[cdc_outbound]` is
/// absent.
///
/// # Errors
///
/// Returns a message when the section is invalid, no database pool is
/// available, the delivery-state DDL cannot be applied, or a sink cannot
/// connect to its broker — each a boot refusal rather than a server that
/// silently replicates nothing.
pub async fn build_drains(
    config: Option<&CdcOutboundConfig>,
    db_pool: Option<&PgPool>,
) -> Result<Option<Vec<SinkDrain>>, String> {
    let Some(cfg) = config else {
        return Ok(None);
    };
    cfg.validate()?;

    let pool = db_pool.cloned().ok_or_else(|| {
        "[cdc_outbound] requires a database pool — the change-log outbox and the per-sink \
         delivery state are database-resident. The binary provides one when database_url is \
         set; library embedders must pass a PgPool to Server::new."
            .to_string()
    })?;

    sqlx::raw_sql(fraiseql_cdc_sinks::outbox_sink_state_migration_sql())
        .execute(&pool)
        .await
        .map_err(|e| {
            format!(
                "[cdc_outbound] could not create the delivery-state table \
                 (core.tb_cdc_sink_state): {e}. Refusing to boot rather than draining without \
                 durable delivery state, which would re-publish every event on restart."
            )
        })?;

    let tick = Duration::from_secs(cfg.tick_interval_secs);
    let mut drains = Vec::with_capacity(cfg.sinks.len());
    for section in &cfg.sinks {
        drains.push(build_one(section, &pool, tick, cfg.batch_size).await?);
    }
    info!(sinks = drains.len(), "cdc outbound configured");
    Ok(Some(drains))
}

/// Connect one sink and wrap it in a drain worker.
async fn build_one(
    section: &CdcSinkSectionConfig,
    pool: &PgPool,
    tick: Duration,
    batch_size: i64,
) -> Result<SinkDrain, String> {
    let mut sink_config = CdcSinkConfig::new(&section.name, &section.subject_template);
    sink_config.tables.clone_from(&section.tables);
    sink_config.tenants.clone_from(&section.tenants);
    if let Some(max_attempts) = section.max_attempts {
        sink_config.max_attempts = max_attempts;
    }

    // Only kinds this build compiled in reach here: `validate()` refused every
    // other one by name before any connection was attempted.
    let connect_failed = |e: fraiseql_cdc_sinks::CdcError| {
        format!(
            "[cdc_outbound] sink {:?} could not connect to {}: {e}. Refusing to boot \
             rather than starting a drain that publishes nowhere.",
            section.name, section.endpoint
        )
    };

    let sink = match section.kind.to_ascii_lowercase().as_str() {
        #[cfg(feature = "cdc-kafka")]
        "kafka" => ConfiguredSink::Kafka(
            fraiseql_cdc_sinks::KafkaSink::connect(&section.endpoint, sink_config.clone())
                .map_err(connect_failed)?,
        ),
        #[cfg(feature = "cdc-kinesis")]
        "kinesis" => ConfiguredSink::Kinesis(
            fraiseql_cdc_sinks::KinesisSink::connect(&section.endpoint, sink_config.clone())
                .await
                .map_err(connect_failed)?,
        ),
        _ => {
            let sink = NatsJetStreamSink::connect(&section.endpoint, sink_config.clone())
                .await
                .map_err(connect_failed)?;
            if let Some(ref stream) = section.ensure_stream {
                sink.ensure_stream(stream, vec![subject_wildcard(&section.subject_template)])
                    .await
                    .map_err(|e| {
                        format!(
                            "[cdc_outbound] sink {:?}: could not ensure JetStream stream \
                             {stream:?}: {e}",
                            section.name
                        )
                    })?;
            }
            ConfiguredSink::NatsJetStream(sink)
        },
    };

    Ok(SinkDrain {
        name: section.name.clone(),
        worker: DrainWorker::new(pool.clone(), sink, sink_config).with_batch_size(batch_size),
        tick,
    })
}

/// The stream subject filter for a template: everything up to the first
/// placeholder, then `>`.
///
/// `fraiseql.{tenant_id}.{table}` becomes `fraiseql.>`, which is the whole
/// space this sink can render into — a stream narrower than that would
/// silently drop the events it excludes.
fn subject_wildcard(template: &str) -> String {
    let literal_prefix = template.split('{').next().unwrap_or("");
    let trimmed = literal_prefix.trim_end_matches('.');
    if trimmed.is_empty() {
        ">".to_string()
    } else {
        format!("{trimmed}.>")
    }
}

/// Spawn every drain onto a `JoinSet`.
pub fn spawn_all(drains: Vec<SinkDrain>, tasks: &mut tokio::task::JoinSet<()>) {
    for drain in drains {
        tasks.spawn(async move { drain.run_forever().await });
    }
}

# fraiseql-cdc-sinks

Outbound change-data-capture for FraiseQL. This crate drains the framework-owned
change-log outbox (`core.tb_entity_change_log`) — the rows the mutation executor and the
external-write capture trigger write in-transaction — to external message brokers, turning
FraiseQL's change spine into a durable event stream for downstream systems.

## Features

- **Durable outbox drain worker** (`DrainWorker`) that enqueues each new, matching change-log
  row into a per-sink delivery-state table keyed by a restart-safe `MAX(seq)` cursor, then
  publishes due rows in `seq` order under `FOR UPDATE SKIP LOCKED`.
- **NATS JetStream sink** (enable the `cdc-nats-jetstream` feature). The broker client
  (`async-nats`, pure Rust) is pulled in only when this feature is on; the default build is
  broker-free and the drain worker plus all encoding/sanitisation logic compile
  unconditionally.
- **At-least-once delivery** — a broker outage accumulates backlog and retries with capped
  exponential backoff rather than losing events; a permanent failure (e.g. an un-renderable
  subject) is dead-lettered. Consumers dedup on `(object_type, seq)`, carried as the NATS
  `Nats-Msg-Id` header (which also engages JetStream's server-side dedup window).
- **Per-tenant / per-table subject templating** (`fraiseql.{tenant_id}.{table}`) that
  sanitises every interpolated segment against the NATS subject charset, failing closed on
  any unsafe value (no subject injection).

## Scope

This is the first CDC sink slice. Additional brokers (Kafka / Kinesis / Pulsar), alternate
encodings (Avro / Protobuf), and server auto-mount from TOML configuration are tracked on the
CDC umbrella.

## License

Dual-licensed under MIT or Apache-2.0, at your option — the same terms as the FraiseQL
workspace.

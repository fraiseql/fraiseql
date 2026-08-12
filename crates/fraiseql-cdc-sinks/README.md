# fraiseql-cdc-sinks

Outbound change-data-capture for FraiseQL. This crate drains the framework-owned
change-log outbox (`core.tb_entity_change_log`) — the rows the mutation executor and the
external-write capture trigger write in-transaction — to external message brokers, turning
FraiseQL's change spine into a durable event stream for downstream systems.

## Features

- **Durable outbox drain worker** (`DrainWorker`) that enqueues each new, matching change-log
  row into a per-sink delivery-state table via an anti-join (so a row whose transaction
  commits out of sequence order is still picked up — never silently skipped), then claims a
  head-contiguous batch under a lease and publishes it in `seq` order with **no database
  transaction held across broker calls**.
- **Ordered from the head** — a transiently failing row blocks its successors
  (head-of-line blocking) instead of being overtaken; a dead-lettered row releases them.
- **Three broker sinks**, each behind its own feature — `cdc-nats-jetstream` (NATS
  `JetStream`, via pure-Rust `async-nats`), `cdc-kafka` (Apache Kafka, via `rdkafka`, which
  builds librdkafka from source and links system OpenSSL), and `cdc-kinesis` (AWS Kinesis
  Data Streams, via the AWS SDK). A broker client is pulled in only when its feature is on;
  the default build is broker-free, and the drain worker plus **all** endpoint guards,
  encoding and sanitisation logic compile unconditionally.
- **Transport guards that refuse rather than default.** Every sink screens its endpoint
  before a client is constructed, and each guard is *pure* — no broker type appears in its
  signature — so the refusing half is exercised by the default, broker-free test build
  rather than only where that broker's feature is on. Plaintext is refused unless the
  broker's own opt-in is set **and** `FRAISEQL_ENV` declares development, and on that path
  the host is screened so the escape hatch cannot reach an instance-metadata address.
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

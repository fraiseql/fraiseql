# Outbound change-data-capture

Fan database changes out to an external broker — a data warehouse loader, a
search indexer, an analytics pipeline — without running Debezium or a sidecar,
and without bypassing FraiseQL's tenancy context.

Requires a server built with the `cdc-outbound` feature.

## How it works

FraiseQL already records every change in `core.tb_entity_change_log`: mutations
write it through the executor, and external writes are captured by the
`@subscribable` capture trigger. That table is the outbox. A **drain worker**
per configured sink reads it, publishes to the broker, and records what it
published in `core.tb_cdc_sink_state` — which the server creates at boot.

```
mutation / external write → core.tb_entity_change_log → drain → broker
                                                          ↕
                                              core.tb_cdc_sink_state
                                              (per-sink delivery state)
```

Because delivery state is durable and per-sink, a restart resumes where it left
off rather than re-publishing history, and two sinks never mark each other's
rows as delivered.

## Configuration

```toml
[cdc_outbound]
tick_interval_secs = 5   # drain cadence (default 5)
batch_size = 256         # rows published per tick per sink (default 256)

[[cdc_outbound.sinks]]
name = "warehouse"                              # durable: it partitions delivery state
kind = "nats-jetstream"
endpoint = "tls://nats.internal:4222"
subject_template = "fraiseql.{tenant_id}.{table}"
tables = ["tb_order", "tb_customer"]            # optional allow-list
tenants = ["8f14e45f-ceea-467a-9e6a-6b1c1cf6b1e4"]  # optional allow-list
max_attempts = 8                                # before dead-lettering
ensure_stream = "FRAISEQL"                      # optional; usually provisioned out of band
```

Placeholders in `subject_template`: `{tenant_id}`, `{table}`, `{op}`. An
interpolated value containing broker-illegal characters dead-letters that event
rather than being re-routed onto an unintended subject.

Omit the `[cdc_outbound]` section to disable outbound CDC. A section that
declares no sinks is a startup error, not a way to turn it off — a drain
configured to go nowhere is a mistake worth surfacing.

## Delivery guarantees

- **At-least-once.** Consumers deduplicate on `(object_type, seq)`, carried in the record (for NATS,
  as the `Nats-Msg-Id` header, which JetStream also uses for its own dedup window; for Kafka, as the
  `fraiseql-msg-id` header).
- **Ordered per sink.** The drain claims a contiguous prefix and publishes in `seq` order; a
  transient failure blocks its successors rather than letting them overtake it.
- **No silent loss.** A transient failure retries with exponential backoff; exhausting
  `max_attempts`, or a permanent failure such as an unrenderable subject, dead-letters the row
  (`status = 'dead'` in `core.tb_cdc_sink_state`) and logs it. Rows whose transaction commits after
  the drain's lag window are recovered by a periodic sweep, which logs each recovery.

## Failure behaviour

The server **refuses to boot** when `[cdc_outbound]` is configured and any of:
the database pool is missing, the delivery-state table cannot be created, a
sink's broker is unreachable, a sink name is duplicated, or a `kind` is unknown
or unimplemented. A server that boots without its drain looks healthy while
every downstream consumer silently starves — the refusal is the point.

Once running, a failed drain tick is logged at `error` and retried on the next
tick; the delivery state is durable, so a transient database or broker outage
costs latency, never events.

## Broker support

| `kind` | Status |
|---|---|
| `nats-jetstream` | Supported |
| `kafka` | Supported — requires the `cdc-kafka` build feature |
| `kinesis` | Recognised; refuses to boot (tracked in #382) |
| `pulsar` | Recognised; refuses to boot (tracked in #382) |

Payloads are JSON. Avro/Protobuf with a schema registry is planned.

A binary built without `cdc-kafka` refuses `kind = "kafka"` **by name**, telling
you which feature to rebuild with — it never accepts the sink and then drops its
events.

Plaintext endpoints are refused unless the broker's opt-in
(`FRAISEQL_NATS_ALLOW_PLAINTEXT=true` or `FRAISEQL_KAFKA_ALLOW_PLAINTEXT=true`)
is set **and** `FRAISEQL_ENV=development` — change events carry business data and
must not cross the wire in the clear. On the plaintext path every host is
screened, so the development escape hatch reaches loopback brokers only and
cannot be used to reach an instance-metadata address.

### Kafka endpoints

`bootstrap.servers` is a comma-separated `host:port` list with no URL scheme, so
transport security is not expressible in it. FraiseQL requires a scheme of its
own and maps it to an explicit `security.protocol`:

| Endpoint | `security.protocol` |
|---|---|
| `kafka+ssl://b1:9092,b2:9092` | `SSL` |
| `kafka+sasl-ssl://b1:9096` | `SASL_SSL` |
| `kafka://localhost:9092` | `PLAINTEXT` (development opt-in only) |

A **scheme-less endpoint is refused, not defaulted** — librdkafka would read it
as `PLAINTEXT`. Every broker in the list is validated, so one bad entry refuses
the whole endpoint rather than being silently skipped.

`kafka+sasl-ssl://` reads its credentials from the environment, not the TOML:

| Variable | Meaning |
|---|---|
| `FRAISEQL_KAFKA_SASL_MECHANISM` | `PLAIN`, `SCRAM-SHA-256` or `SCRAM-SHA-512` — required |
| `FRAISEQL_KAFKA_SASL_USERNAME` | SASL username |
| `FRAISEQL_KAFKA_SASL_PASSWORD` | SASL password |

The mechanism is required rather than defaulted: librdkafka's own default is
`GSSAPI`, which these builds cannot perform, and no single default fits every
broker (Confluent Cloud uses `PLAIN`, MSK `SCRAM-SHA-512`, Redpanda
`SCRAM-SHA-256`). Kerberos/GSSAPI is deliberately not supported — it is the only
mechanism that would require linking Cyrus libsasl2, and every managed Kafka
offers one of the three above.

Kafka topic names are narrower than NATS subjects (`[a-zA-Z0-9._-]`, at most 249
characters, and never `.` or `..`). A `subject_template` that renders outside
that charset dead-letters the event rather than being re-routed to another topic.

Records are keyed by entity identity (`{object_type}:{object_id}`), which pins
all of one entity's changes to a single partition — Kafka only orders within a
partition. The `(object_type, seq)` consumer dedup key travels in the payload and
in the `fraiseql-msg-id` header. The producer runs with `enable.idempotence`, so
producer-side retries neither duplicate nor reorder.

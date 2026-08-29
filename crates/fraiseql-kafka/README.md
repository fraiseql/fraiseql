# fraiseql-kafka

The single Kafka egress used by every FraiseQL path that produces to a broker.

Two callers need one: `fraiseql-cdc-sinks`' outbox drain, whose durability
(claim → publish → record, with retry and dead-lettering) sits *above* this crate,
and `fraiseql-core`'s subscription transport, which is at-most-once and has no
outbox at all. What they share is the part that must not differ — the endpoint
guard, `security.protocol`, SASL resolution, and the produce call.

Endpoint parsing and the plaintext refusal live one crate lower, in
[`fraiseql-guard`](../fraiseql-guard), because they name no rdkafka type and so
belong where every crate can reach them.

See issue #1102 for why this exists rather than a producer per caller.

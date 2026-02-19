# ADR-0006: Custom Wire Protocol for Streaming JSON

## Status: Accepted

## Context

Standard PostgreSQL drivers (tokio-postgres, pgx) buffer entire result sets in memory before returning. For large datasets (10M+ rows), this creates multi-GB memory spikes and 500ms+ time-to-first-byte latency. FraiseQL's use case demands streaming results as JSON without buffering. Standard COPY protocol and server-side cursors add protocol overhead.

## Decision

Implement `fraiseql-wire`: custom PostgreSQL wire protocol extension that:

- Streams rows as JSON objects immediately after query execution starts
- Maintains constant memory footprint (buffer one row at a time)
- Omits result set metadata, reducing protocol overhead
- Returns sub-millisecond time-to-first-byte
- Supports cancellation mid-stream

Extends PostgreSQL's binary protocol, requiring custom decoder/encoder in fraiseql-wire crate.

## Consequences

**Positive:**
- Sub-millisecond first-byte latency
- Constant memory usage for arbitrarily large result sets
- Better user experience for real-time data
- Competitive advantage over REST APIs

**Negative:**
- Requires maintaining wire protocol implementation
- PostgreSQL-specific; other databases need different streaming strategies
- Potential compatibility issues with PostgreSQL version updates

## Alternatives Considered

1. **tokio-postgres with streaming**: Library not designed for row-by-row streaming; requires workarounds
2. **COPY protocol**: Requires client to parse PostgreSQL format; less efficient than JSON
3. **Server-side cursors**: Adds protocol round-trips; slower than streaming

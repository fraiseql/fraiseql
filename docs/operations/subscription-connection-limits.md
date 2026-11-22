# Subscription connection limits

A `WebSocket` subscription connection stays open for as long as the client keeps it.
The tenant registry's `max_concurrent` permit is held only while a request runs, and
an upgrade returns as soon as it answers `101`, so it bounds nothing about connections
already open. The server therefore counts open `/ws` connections itself and refuses an
upgrade that would go over a bound.

## The bounds

| Key (`[subscriptions]`) | Default | Counts |
|---|---|---|
| `max_connections_per_principal` | 32 | One principal's connections, within its tenant |
| `max_connections_per_tenant` | 1000 | The connections of one tenant's authenticated principals |
| `max_anonymous_connections` | 1000 | Connections with no principal, across the server |
| `max_anonymous_connections_per_ip` | 32 | Connections with no principal, from one client address |

Each key takes a positive number or the string `"unlimited"`. An omitted key keeps its
default. `0` is refused when the schema is compiled and when it is loaded: it would
refuse every connection.

```toml
# fraiseql.toml
[subscriptions]
max_connections_per_principal = 16
max_connections_per_tenant = 5000
max_anonymous_connections = "unlimited"
max_anonymous_connections_per_ip = 8
```

A tenant registered through the tenant admin API can set its own bound with
`max_subscription_connections`, which takes the place of `max_connections_per_tenant`
for that tenant. It takes the same values, and `0` is refused there too.

## Who counts against what

A principal is its user id within its tenant: the same user id in two tenants is two
principals. The tenant a principal's connection counts against is the one its token
binds. A request that names any other tenant is refused before it is counted.

A connection with no principal names its tenant only through a request hint, such as
`X-Tenant-ID` or `Host`, which any client can write. It is never counted against that
tenant, so clients with no credentials cannot use up the connections a tenant's
principals hold. It counts against `max_anonymous_connections` and against its client
address. The address is read the way the rate limiter reads it: `X-Real-IP` and
`X-Forwarded-For` count only when `[security.rate_limiting] trust_proxy_headers = true`
and the peer is in `trusted_proxy_cidrs`. Otherwise the address is the TCP peer, and an
IPv6 address is keyed by its /64. Behind a proxy without that trust, every
unauthenticated client shares the proxy's address and its 32 connections.

## The refusal

An upgrade over a bound is answered before the `101`, with
`429 Too Many Requests` and the error code `SUBSCRIPTION_CONNECTIONS_EXHAUSTED`. The
message names which bound was reached. There is no `Retry-After`: a slot is free as
soon as one of the connections it counts closes.

A connection gives its slot back when it ends, however it ends: the client closes it,
the server closes it (an init timeout, a protocol violation, a token that expires or
is withdrawn mid-stream, a drain), or the socket drops. An upgrade that is refused, or
that never completes, holds no slot.

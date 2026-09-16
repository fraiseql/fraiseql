# fraiseql-jwks

The single JWKS client used by every FraiseQL path that verifies a JWT against a
publisher's key set.

Three callers need one: `fraiseql-core`'s `[auth]` OIDC validator,
`fraiseql-auth`'s OAuth ID-token check, and the inbound webhook schemes whose
sender is an identity provider. The first two each had their own, and the two had
drifted — one accepted RSA keys only and did not pin DNS, the other accepted RSA
and EC and pinned against rebinding. An operator whose IdP rotated onto an EC key
therefore had one path work and the other refuse every token.

## What this crate adds that neither copy had

Neither bounded a refetch. A `kid` the cache did not hold fetched the key set
again, every time, with no negative cache, no cooldown and no single-flight — so
any anonymous client could make the server issue one outbound HTTPS request per
inbound request just by naming a `kid` nobody publishes.

The consequence is not the amplification. IdPs rate-limit their JWKS endpoints;
once the IdP throttles the server, a *genuine* key rotation can no longer be
fetched and every user holding a new-`kid` token is refused. The amplification
converts into an authentication outage, caused by traffic that never
authenticated.

So: a miss is remembered for `REFETCH_COOLDOWN`, concurrent misses single-flight
onto one request, a failed fetch cools down like a successful one, and an expired
key set is never served — because a key the publisher withdrew has to stop
validating at the TTL, and the cooldown must not extend that window.

The algorithm allow-list is the **caller's** to check, and it must run *before*
asking for a key: a token the server would refuse on its header alone must not
cost an outbound request.

Address validation and DNS pinning use
[`fraiseql-guard`](../fraiseql-guard), the one outbound guard in the workspace,
so a `jwks_uri` pointing inside the network is refused on what it resolves to
rather than on how it is spelled.

See issue #1335 for the measurements, and #1322 for the third caller.

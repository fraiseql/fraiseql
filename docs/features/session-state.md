# Session state: durable per-thread conversation memory

Agents and multi-turn applications need short-term working memory — intermediate
reasoning, accumulated context, thread summaries — with deterministic retention.
The `[session_state]` subsystem is that store: durable key/value entries scoped
to `(session, thread)`, per-entry TTL, a background eviction sweep, and an
optional summarisation hook that collapses long threads.

## Configuration

```toml
[session_state]
backend = "postgres"        # "memory" (volatile, dev) | "postgres" (durable)
default_ttl_secs = 3600     # per-entry TTL
evict_interval_secs = 300   # background sweep cadence
```

Presence of the section enables the subsystem; absence leaves it off. The
section is strict: an unrecognised key is a boot error.

**The backend never downgrades silently.** `backend = "postgres"` requires a
database pool and initialises `_system.session_state` at boot (idempotently,
like `_system.sessions`); if the pool is missing or the table cannot be
created, the server refuses to boot. `backend = "memory"` is volatile — every
thread is lost on restart — and logs a warning at boot; use it for local
development only.

## Semantics

- **Isolation** is application-layer, exactly like `_system.sessions`: the
  `session_id` comes from the authenticated context (an MCP session, a user
  session), never from a client-named field. One session cannot read another's
  threads.
- **TTL**: every write stamps `expires_at = now + default_ttl_secs`. Expired
  entries are invisible to reads immediately; the background sweep reclaims
  their storage every `evict_interval_secs`.
- **Size cap**: a value serializing beyond 64 KiB is refused loudly — this is
  working state, not blob storage.
- **Summarisation** (library API): install a `Summarizer` with
  `SessionState::with_summarizer(summarizer, threshold)`. Once a thread holds
  more than `threshold` ordinary entries, the whole thread is atomically
  replaced by a single entry under the reserved `_summary` key (which ordinary
  writes may not use). A failing summarizer leaves the thread intact — state
  loss is worse than an oversized thread. No summarizer installed means threads
  simply grow and expire by TTL.

## Embedding

Library embedders reach the store through `Server::session_state()`
(`fraiseql_auth::session_state::SessionState`): `get` / `set` / `delete` /
`list_thread` / `expire_thread`, plus `evict_expired` for custom sweeps. The
subsystem requires the `auth` feature (on by default).

See `.github` issue #389 for the roadmap items built on top of this store
(MCP session continuity, GraphQL-surfaced `Thread`/`Message` types).

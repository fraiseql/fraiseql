# FraiseQL on FreeBSD

FraiseQL is a single static-ish Rust binary with no runtime language
dependencies, which makes it a natural fit for the FreeBSD operational
model: Jails for isolation, ZFS for cheap multi-tenancy, and Caddy for
TLS and routing.

Compile-time FreeBSD support is enforced on every PR by the
`FreeBSD x86_64 cross-check` CI job (issue
[#148](https://github.com/fraiseql/fraiseql/issues/148)), which
cross-compiles the workspace and the full `fraiseql-server` feature
surface for `x86_64-unknown-freebsd`. Runtime testing on a real
FreeBSD host is not yet automated — see
[Known limitations](#known-limitations-on-freebsd).

## Why FreeBSD

The fit is structural:

- **Jails** isolate FraiseQL and PostgreSQL into separate process
  groups. The database Jail can run with no network interface at all;
  the API Jail reaches it only through a nullfs-mounted Unix socket, so
  the database has no network attack surface.
- **ZFS** clones make multi-tenancy a `zfs clone` away: a new tenant is
  a clone of the API Jail dataset plus an edited `fraiseql.toml` and a
  few lines of `Caddyfile`.
- **Caddy** terminates TLS and routes per-Jail (per-tenant).

## Install the binary

Two supported paths:

### Build on the FreeBSD host (recommended)

Rust and the FraiseQL workspace build natively on FreeBSD with no extra
configuration:

```sh
pkg install rust
cargo install fraiseql-cli   # or build from a checkout: cargo build --release
```

This is the most reliable path and the only one that supports the two
features excluded from cross-compilation (see
[Known limitations](#known-limitations-on-freebsd)).

### Cross-compile from Linux

The CI cross-check produces `x86_64-unknown-freebsd` artifacts from a
Linux host using a FreeBSD sysroot and `clang`. To reproduce locally:

```sh
rustup target add x86_64-unknown-freebsd

# Unpack a FreeBSD base set as a sysroot (libc + libs + headers):
mkdir -p /tmp/freebsd-sysroot
curl -sSL -o /tmp/base.txz \
  https://download.freebsd.org/releases/amd64/amd64/14.3-RELEASE/base.txz
tar -xf /tmp/base.txz -C /tmp/freebsd-sysroot ./lib ./usr/lib ./usr/include

export CC_x86_64_unknown_freebsd=clang
export AR_x86_64_unknown_freebsd=llvm-ar
export CFLAGS_x86_64_unknown_freebsd="--target=x86_64-unknown-freebsd14 --sysroot=/tmp/freebsd-sysroot"

cargo build --release -p fraiseql-server --target x86_64-unknown-freebsd
```

The sysroot and `clang` are needed because a few dependencies compile C
(BoringSSL via `rustls`/`aws-lc-sys`, `zstd-sys`); pure-Rust crates need
none of it.

Once you have a binary, drop it in place on the FreeBSD host:

```sh
fetch https://example.com/fraiseql-server-x86_64-unknown-freebsd.tar.gz
sha256 -c CHECKSUMS.txt fraiseql-server-x86_64-unknown-freebsd.tar.gz
tar -xzf fraiseql-server-x86_64-unknown-freebsd.tar.gz
mv fraiseql-server /usr/local/bin/
```

## Jails layout

Run two Jails per tenant: `api-{tenant}` for FraiseQL and `db-{tenant}`
for PostgreSQL. The DB Jail has no network interface; the API Jail sees
the Postgres Unix socket via a nullfs mount.

```
# /usr/local/etc/jail.conf

api-acme {
    path = "/zroot/jails/api-acme";
    host.hostname = "api.acme.local";
    mount.fstab = "/usr/local/etc/jail.acme.fstab";
    exec.start = "/usr/local/bin/fraiseql-server";
    # ... standard hardening (allow.* = 0, exec.clean, etc.)
}

db-acme {
    path = "/zroot/jails/db-acme";
    host.hostname = "db.acme.local";
    ip4 = disable;            # no network at all
    exec.start = "/usr/local/etc/rc.d/postgresql onestart";
}
```

The nullfs mount makes the DB Jail's `/sockets/postgres` directory
visible at the same path inside the API Jail:

```
# /usr/local/etc/jail.acme.fstab
/zroot/jails/db-acme/sockets/postgres /zroot/jails/api-acme/sockets/postgres nullfs ro 0 0
```

FraiseQL then connects over the Unix socket — no TCP, no exposed port:

```
postgresql:///acme?host=/sockets/postgres
```

## ZFS clones for multi-tenancy

```sh
zfs snapshot zroot/jails/api-acme@template
zfs clone   zroot/jails/api-acme@template zroot/jails/api-newtenant

# Edit /zroot/jails/api-newtenant/fraiseql.toml — point at the new DB.
# Add a Caddyfile vhost for newtenant.example.com.
service jail start api-newtenant
```

Cloning is copy-on-write, so a new tenant costs only its diff on disk.

## Caddyfile pattern

Front each API Jail with a Caddy vhost over its Unix socket:

```
acme.example.com {
    reverse_proxy unix//zroot/jails/api-acme/run/fraiseql.sock
}

newtenant.example.com {
    reverse_proxy unix//zroot/jails/api-newtenant/run/fraiseql.sock
}
```

## Known limitations on FreeBSD

The CI cross-check covers the default-feature workspace build and the
full `fraiseql-server` feature surface (storage backends, gRPC, Arrow,
observers + NATS, Redis, MCP, metrics, OpenTelemetry, webhooks, Vault
secrets, federation). Those all cross-compile and are expected to work.

Two optional features are **not** exercised by the cross-check because
they have no Linux→FreeBSD cross path. They build on a FreeBSD host but
are unverified there:

| Feature | Status on FreeBSD | Notes |
|---|---|---|
| Core GraphQL + PostgreSQL | ✅ | covered by cross-check |
| Object storage (`aws-s3`, `azure-blob`, `gcs`) | ✅ | rustls, covered |
| `observers` / `observers-nats` | ✅ | pure-Rust transports, covered |
| `secrets` (Vault) | ✅ | HTTP over rustls, covered |
| `mcp`, `metrics`, `tracing-opentelemetry`, `webhooks` | ✅ | covered |
| Deno edge functions (`fraiseql-functions/runtime-deno`) | ⚠️ build natively | pulls `deno_core` → `v8`, which has no Linux→FreeBSD cross path. Build on a FreeBSD host; not exercised in CI. |
| SQL Server backend (`sqlserver` / `mssql`) | ⚠️ build natively | uses `tiberius` (`native-tls` → OpenSSL). Cross-builds only against a target OpenSSL; builds natively on FreeBSD where OpenSSL ships in base. PostgreSQL is the primary backend. |

If you depend on one of the ⚠️ features on FreeBSD, build on the host
and please file an issue with your results.

## DTrace tracing (future)

The DTrace probes mentioned in
[#148](https://github.com/fraiseql/fraiseql/issues/148) are downstream
tooling and are not yet implemented. This guide covers making the
binary *run* on FreeBSD; DTrace integration is tracked separately.

## Related

- [#148: FreeBSD support](https://github.com/fraiseql/fraiseql/issues/148) — tracking issue.
- [Deployment runbook](../runbooks/01-deployment.md) — version rollout, health checks, rollback.
- [Production security checklist](production-security-checklist.md) — hardening before exposing the endpoint.

# fraiseql-storage

Object storage backends and HTTP handlers for FraiseQL. This crate provides an enum-dispatched storage abstraction over local filesystem, AWS S3, Google Cloud Storage, Azure Blob Storage, and S3-compatible European providers (Hetzner, Scaleway, OVH, Exoscale, Backblaze, Cloudflare R2).

Storage objects are tracked in a SQL metadata repository (Postgres-only today) with row-level security enforcement for per-tenant access control. HTTP handlers expose `PUT`, `GET`, `DELETE`, and `LIST` operations against the chosen backend. Bucket configuration enforces size limits and MIME-type allowlists at the boundary.

## Features

- `local` (default) — local filesystem backend
- `aws-s3` — AWS S3 and S3-compatible providers
- `gcs` — Google Cloud Storage (JWT-signed requests)
- `azure-blob` — Azure Blob Storage (shared key auth)
- `transforms` — image transforms via the `image` crate (resize, format conversion, EXIF strip)
- Postgres metadata repository with row-level security
- HTTP handlers for object upload, download, deletion, and listing

## Usage

```toml
[dependencies]
fraiseql-storage = { version = "2.3.0", features = ["aws-s3"] }
```

```rust
use fraiseql_storage::backend::StorageBackend;

// StorageBackend is an enum dispatched over the enabled backend features.
// Construct it from your bucket configuration and pass it into the
// fraiseql-server storage route layer.
```

## Documentation

- [API Documentation](https://docs.rs/fraiseql-storage)
- [FraiseQL Documentation](https://docs.fraiseql.dev)
- [Repository](https://github.com/fraiseql/fraiseql)

## License

MIT OR Apache-2.0

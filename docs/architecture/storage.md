# Storage Architecture

The `fraiseql-storage` crate provides a unified object storage abstraction for
FraiseQL, supporting file uploads, downloads, and transforms across multiple
cloud backends.

## Overview

```
HTTP Upload → Storage Router → Backend Adapter → S3 / GCS / Azure / Local
                    ↓
              RLS Enforcement (per-tenant isolation)
                    ↓
              Optional Transforms (resize, EXIF strip)
```

## Backends

| Backend | Feature Flag | Status |
|---------|-------------|--------|
| Amazon S3 / MinIO | `aws-s3` | Stable |
| Google Cloud Storage | `gcs` | Stable |
| Azure Blob Storage | `azure-blob` | Stable |
| Local filesystem | (default) | Development only |

All backends implement the same `StorageBackend` trait, enabling transparent
switching between providers via configuration.

## Security

- **RLS enforcement**: Storage operations respect Row-Level Security policies.
  Each upload/download is scoped to the authenticated user's tenant.
- **Path validation**: Upload paths are validated against traversal attacks.
  The `validate_socket_dir` pattern rejects `..` components.
- **Size limits**: Configurable per-file and per-request size limits.

## Transforms (Optional)

When the `transforms` feature is enabled:

- Image resizing (width, height, fit mode)
- EXIF metadata stripping (privacy)
- Format conversion (JPEG, PNG, WebP)

Transforms are applied on upload before storage, reducing storage costs and
ensuring consistent output formats.

## Configuration

```toml
[storage]
backend = "s3"

[storage.s3]
bucket = "my-app-uploads"
region = "us-east-1"
endpoint = "${AWS_ENDPOINT_URL}"  # For MinIO compatibility
```

## Local development with emulators

All three cloud backends honour the `endpoint` field, so the standard
emulators can be used for local development and CI:

| Backend | Emulator | Example `endpoint` |
|---------|----------|--------------------|
| S3 | [MinIO](https://min.io/) | `http://localhost:9000` |
| Azure Blob | [Azurite](https://github.com/Azure/Azurite) | `http://127.0.0.1:10000/devstoreaccount1` |
| GCS | [fake-gcs-server](https://github.com/fsouza/fake-gcs-server) | `http://localhost:4443` |

For Azure Blob the endpoint is the account-level base (Azurite serves the
account as a path segment, e.g. `devstoreaccount1`). Example:

```toml
[storage]
backend = "azure"
account_name = "devstoreaccount1"
bucket = "uploads"           # container name
endpoint = "http://127.0.0.1:10000/devstoreaccount1"
```

When `endpoint` is omitted the backends target the production hostnames
(`*.blob.core.windows.net`, `storage.googleapis.com`), so real-cloud
deployments need no change.

## API

Storage endpoints are mounted under `/storage/v1/`:

- `POST /storage/v1/upload` -- Upload a file
- `GET /storage/v1/download/:path` -- Download a file
- `DELETE /storage/v1/delete/:path` -- Delete a file

## Crate Dependencies

```
fraiseql-storage
├── fraiseql-error
├── image (optional, transforms feature)
└── kamadak-exif (optional, transforms feature)
```

## Testing

S3 backend tests require MinIO and are gated behind `#[ignore]`:

```bash
# Start MinIO
docker run -p 9000:9000 -e MINIO_ROOT_USER=minioadmin -e MINIO_ROOT_PASSWORD=minioadmin minio/minio server /data

# Run storage tests
cargo test -p fraiseql-storage -- --ignored
```

## See Also

- [Functions Architecture](functions.md) -- Serverless functions runtime
- [Architecture Overview](overview.md) -- System-wide architecture

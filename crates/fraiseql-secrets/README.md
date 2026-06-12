# fraiseql-secrets

Secrets management for FraiseQL. This crate provides pluggable backends for storing and retrieving secrets, along with automatic credential rotation.

> **Field-level at-rest encryption is not supported in this release.** The write path does
> not encrypt field values, so a field marked for encryption would be stored in plaintext.
> The server refuses to start when any schema field declares `encryption`. The
> encryption types and read path are retained for the in-progress implementation but must
> not be relied on for at-rest confidentiality.

## Features

- HashiCorp Vault integration
- Environment variable backend
- File-based secrets
- Credential rotation with monitoring
- Audit logging for secret access

## Usage

```toml
[dependencies]
fraiseql-secrets = "2.3"
```

## Documentation

- [API Documentation](https://docs.rs/fraiseql-secrets)
- [FraiseQL Documentation](https://docs.fraiseql.dev)
- [Repository](https://github.com/fraiseql/fraiseql)

## License

MIT OR Apache-2.0

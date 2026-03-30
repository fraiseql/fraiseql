# fraiseql-secrets

Secrets management and field-level encryption for FraiseQL. This crate provides pluggable backends for storing and retrieving secrets, along with transparent encryption of sensitive database columns at rest and automatic credential rotation.

## Features

- HashiCorp Vault integration
- Environment variable backend
- File-based secrets
- Field-level column encryption at rest
- Credential rotation with monitoring
- Audit logging for secret access

## Usage

```toml
[dependencies]
fraiseql-secrets = "2.1.0"
```

## Documentation

- [API Documentation](https://docs.rs/fraiseql-secrets)
- [FraiseQL Documentation](https://docs.fraiseql.dev)
- [Repository](https://github.com/fraiseql/fraiseql)

## License

MIT OR Apache-2.0

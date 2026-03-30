# fraiseql-auth

Authentication, authorization, and session management for FraiseQL. This crate provides the security layer that validates tokens, enforces access control policies, and manages user sessions across the FraiseQL server runtime.

## Features

- OIDC/JWKS token validation
- PKCE OAuth flow
- JWT verification
- Role-based access control
- Session management
- Constant-time token comparison

## Usage

```toml
[dependencies]
fraiseql-auth = "2.1.0"
```

## Documentation

- [API Documentation](https://docs.rs/fraiseql-auth)
- [FraiseQL Documentation](https://docs.fraiseql.dev)
- [Repository](https://github.com/fraiseql/fraiseql)

## License

MIT OR Apache-2.0

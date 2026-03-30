# fraiseql-webhooks

Webhook signature verification and event processing for FraiseQL. This crate handles inbound webhook validation, ensuring payload integrity and authenticity before events are dispatched to the FraiseQL server for processing.

## Features

- HMAC-SHA256 signature verification
- Webhook event parsing
- Replay attack prevention via timestamp validation
- Multiple signature scheme support

## Usage

```toml
[dependencies]
fraiseql-webhooks = "2.1.0"
```

## Documentation

- [API Documentation](https://docs.rs/fraiseql-webhooks)
- [FraiseQL Documentation](https://docs.fraiseql.dev)
- [Repository](https://github.com/fraiseql/fraiseql)

## License

MIT OR Apache-2.0

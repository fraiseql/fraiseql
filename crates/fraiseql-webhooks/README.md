# fraiseql-webhooks

Webhook signature verification and event processing for FraiseQL. This crate handles inbound webhook validation, ensuring payload integrity and authenticity before events are dispatched to the FraiseQL server for processing.

## Features

- HMAC-SHA256 (and Ed25519 / ECDSA) signature verification for 15+ providers
- Replay attack prevention via timestamp validation
- Multiple signature scheme support
- An inbound receiver pipeline (`WebhookPipeline`) with atomic idempotency
  (duplicate deliveries are silently discarded) and transactional handler
  execution (the idempotency claim and the handler commit or roll back together)

## Usage

```toml
[dependencies]
fraiseql-webhooks = "2.3"
```

## Documentation

- [API Documentation](https://docs.rs/fraiseql-webhooks)
- [FraiseQL Documentation](https://docs.fraiseql.dev)
- [Repository](https://github.com/fraiseql/fraiseql)

## License

MIT OR Apache-2.0

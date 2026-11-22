# FraiseQL Dart/Flutter SDK

Dart/Flutter client SDK for authoring FraiseQL GraphQL schemas.

## Installation

```yaml
# pubspec.yaml
dependencies:
  fraiseql: ^2.1.6
```

```bash
dart pub get
```

## Quick Start

```dart
import 'package:fraiseql/fraiseql.dart';

void main() {
  final schema = FraiseQLSchema();

  schema.type('User', sqlSource: 'users', fields: {
    'id': FieldType.int_(),
    'name': FieldType.string(),
    'email': FieldType.string(),
  });

  schema.type('Post', sqlSource: 'posts', fields: {
    'id': FieldType.int_(),
    'title': FieldType.string(),
    'body': FieldType.string(),
    'fk_user': FieldType.int_(),
  });

  schema.exportJson('schema.json');
}
```

## Features

- Type definitions with SQL source mapping
- Enum support
- Query and mutation registration
- Field-level metadata (description, deprecation, access control)
- Fact table and analytics support (measures, dimensions)
- Observer and webhook configuration
- Custom scalar types
- CRUD auto-generation

> **Dynamic SQL-source selection is not supported.** The `SqlSourceDispatch` annotation was removed in #926:
> no part of the compiler ever read `sql_source_dispatch`, so a query using it dispatched
> nothing. Declare **one query per source** — `ordersDaily` on `tf_orders_day`,
> `ordersWeekly` on `tf_orders_week` — which also gives each source its own compile-time
> SQL identifier validation.

## Field Metadata

```dart
schema.type('User', sqlSource: 'users', fields: {
  'id': FieldType.int_(),
  'email': FieldType.string(
    requiresScope: 'admin:read',
    description: 'User email address',
  ),
  'ssn': FieldType.string(
    requiresScope: 'pii:read',
    onDeny: OnDeny.nullMask,
    deprecated: 'Use encrypted_ssn instead',
  ),
});
```

A String stored as a locale map (`[locale]` in `fraiseql.toml`) is
`FieldType.string(localized: true)`, as a field, an input field or a mutation argument.
Only a String can be localized; declaring any other localized type is refused.

## Observers

An observer runs actions when a row of an entity is inserted, updated or deleted:

```dart
schema.observer(
  'orderShipped',
  entity: 'Order',
  event: 'UPDATE',
  condition: "status == 'shipped'",
  actions: [
    ObserverAction.webhook(urlEnv: 'ORDER_HOOK_URL', signingSecretEnv: 'ORDER_HOOK_KEY'),
    ObserverAction.slack('#orders', 'Order {{ id }} shipped'),
    ObserverAction.email('ops@example.com', 'Order shipped', 'Order {{ id }} shipped'),
  ],
  retry: const RetryConfig(
    maxAttempts: 5,
    backoffStrategy: 'exponential',
    initialDelayMs: 200,
    maxDelayMs: 5000,
  ),
);
```

A webhook takes exactly one of `url` and `urlEnv`. A Slack action posts to the URL in
`SLACK_WEBHOOK_URL` unless `webhookUrl` or `webhookUrlEnv` names another. The email
sender is the server's SMTP configuration. An observer with no `retry` runs on the
server's own policy.

## Compile and Serve

```bash
dart run schema.dart            # Generate schema.json
fraiseql-cli compile schema.json  # Compile to schema.compiled.json
fraiseql-server --schema schema.compiled.json
```

## Requirements

- Dart SDK >= 3.0.0
- FraiseQL CLI for schema compilation

## License

MIT or Apache 2.0

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
- Subscription definitions
- Field-level metadata (description, deprecation, access control)
- Fact table and analytics support (measures, dimensions)
- Observer and webhook configuration
- Custom scalar types
- CRUD auto-generation

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

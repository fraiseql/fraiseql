# fraiseql-dart

> **Status: Not yet implemented.**

The Dart authoring SDK for FraiseQL is planned but not yet built.

## What it will provide

A Dart-native way to define FraiseQL schemas that compile to `schema.json`:

```dart
// Planned API (subject to change)
@FraiseQLType()
class User {
  @FraiseQLField()
  final int id;

  @FraiseQLField()
  final String name;

  @FraiseQLField(nullable: true)
  final String? email;
}

void main() {
  FraiseQL.query(
    name: 'users',
    returnType: User,
    returnsList: true,
    sqlSource: 'v_users',
    args: {'limit': FraiseQLArg(type: 'Int', defaultValue: 10)},
  );

  FraiseQL.exportSchema('schema.json');
}
```

## Alternatives

The following SDKs are production-ready today:

- [fraiseql-python](../fraiseql-python) — reference implementation
- [fraiseql-typescript](../fraiseql-typescript)
- [fraiseql-java](../fraiseql-java)
- [fraiseql-php](../fraiseql-php)
- [fraiseql-go](../fraiseql-go)

## Contributing

Contributions welcome. See the Python SDK for the reference authoring API and
the expected `schema.json` output format.

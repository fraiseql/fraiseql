# FraiseQL PHP SDK

PHP 8.2+ schema authoring library for [FraiseQL](https://fraiseql.dev) — the compiled GraphQL execution engine that transforms schema definitions into optimized SQL at build time.

## Requirements

- PHP 8.2+
- `ext-json`

## Installation

```bash
composer require fraiseql/fraiseql
```

## How it works

FraiseQL uses a **compile-time** approach:

1. **Author** your schema in PHP using attributes
2. **Export** to `schema.json` via the CLI binary
3. **Compile** with `fraiseql compile` to produce optimized SQL
4. **Serve** with the FraiseQL Rust runtime — zero PHP at query time

```
PHP classes   →   schema.json   →   schema.compiled.json   →   GraphQL server
(authoring)       (export)          (fraiseql compile)          (Rust runtime)
```

## Quick start

### 1. Define types

```php
<?php
// schema/schema.php

use FraiseQL\Attributes\GraphQLType;
use FraiseQL\Attributes\GraphQLField;
use FraiseQL\StaticAPI;

#[GraphQLType(name: 'Author', sqlSource: 'v_author')]
final class Author
{
    #[GraphQLField(type: 'ID', nullable: false)]
    public int $id;

    #[GraphQLField(type: 'String', nullable: false)]
    public string $name;

    #[GraphQLField(type: 'String', nullable: true)]
    public ?string $bio;
}

// Register types
StaticAPI::register(Author::class);

// Register queries
StaticAPI::query('authors')
    ->returnType('Author')
    ->returnsList(true)
    ->sqlSource('v_author')
    ->register();

StaticAPI::query('author')
    ->returnType('Author')
    ->sqlSource('v_author')
    ->argument('id', 'ID', nullable: false)
    ->register();

// Register mutations
StaticAPI::mutation('createAuthor')
    ->returnType('Author')
    ->sqlSource('fn_create_author')
    ->operation('insert')
    ->argument('name', 'String', nullable: false)
    ->register();
```

### 2. Export schema

```bash
vendor/bin/fraiseql export schema/schema.php
# Schema exported to schema.json
#   Version:   2.0.0
#   Types:     1
#   Queries:   2
#   Mutations: 1
```

### 3. Compile

```bash
fraiseql compile schema.json
# Produces: schema.compiled.json
```

### 4. Run

Start the FraiseQL Rust server pointing at `schema.compiled.json`. The PHP SDK is no longer involved at runtime.

## Attributes

### `#[GraphQLType]`

Marks a class as a GraphQL type.

| Parameter | Type | Description |
|-----------|------|-------------|
| `name` | `?string` | GraphQL type name (default: class name) |
| `sqlSource` | `?string` | SQL view backing this type (e.g. `v_user`) |
| `description` | `?string` | Schema documentation |
| `isInput` | `bool` | Whether this is a GraphQL input type |
| `relay` | `bool` | Whether this type implements the Relay Node interface |
| `isError` | `bool` | Whether this is a mutation error type |

### `#[GraphQLField]`

Marks a property as a GraphQL field.

| Parameter | Type | Description |
|-----------|------|-------------|
| `type` | `?string` | GraphQL type (auto-detected from PHP type if omitted) |
| `description` | `?string` | Field documentation |
| `nullable` | `bool` | Whether the field is nullable (default: `false`) |
| `resolver` | `?string` | Custom resolver method name |
| `scope` | `?string` | JWT scope required to access this field |
| `scopes` | `?array` | Multiple JWT scopes required |

## Static API

For programmatic schema construction without attributes:

```php
use FraiseQL\StaticAPI;

// Queries
StaticAPI::query('posts')
    ->returnType('Post')
    ->returnsList(true)
    ->sqlSource('v_post')
    ->cacheTtlSeconds(300)
    ->register();

// Mutations
StaticAPI::mutation('deletePost')
    ->returnType('Post')
    ->sqlSource('fn_delete_post')
    ->operation('delete')
    ->argument('id', 'ID', nullable: false)
    ->register();
```

## Schema export

The `SchemaExporter` class produces the canonical `IntermediateSchema` JSON format:

```php
use FraiseQL\SchemaExporter;

// Export to file
SchemaExporter::exportToFile('schema.json');

// Export to string
$json = SchemaExporter::export();

// Inspect as array
$schema = SchemaExporter::toArray();
// ['version' => '2.0.0', 'types' => [...], 'queries' => [...], 'mutations' => [...]]
```

## CLI reference

```
vendor/bin/fraiseql export [options] <bootstrap-file>

Options:
  --output=<path>   Output file path (default: schema.json)
  --compact         Compact JSON output
  --help            Show help
```

## Type mapping

PHP types are automatically mapped to GraphQL scalar types:

| PHP type | GraphQL type |
|----------|-------------|
| `int` | `Int` |
| `float` | `Float` |
| `bool` | `Boolean` |
| `string` | `String` |
| `?T` | nullable `T` |

Use `#[GraphQLField(type: 'ID')]` to override the inferred type.

## License

MIT — see [LICENSE](LICENSE).

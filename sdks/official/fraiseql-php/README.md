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

## Working with an exported schema

`SchemaExporter` gives you the document. `JsonSchema`, `Validator`, `CacheKey` and
`SchemaCache` are author-side utilities for working with one — inspecting it, pre-flighting
it before you shell out to the compiler, and caching it across a build. They are optional:
nothing in the export path uses them, and `fraiseql compile` remains the authority on
whether a document is valid.

Start from the exporter. `JsonSchema` has no public constructor, because every document it
should hold comes from one:

```php
use FraiseQL\JsonSchema;
use FraiseQL\SchemaExporter;

$schema = JsonSchema::fromJson(SchemaExporter::export());   // from the JSON string
$schema = JsonSchema::fromArray(SchemaExporter::toArray()); // or without the string hop
$schema = JsonSchema::loadFromFile('schema.json');          // or from a file on disk
```

Then inspect it:

```php
$schema->version;              // '2.0.0'
$schema->getTypeNames();       // ['User', 'Post']
$schema->getQueryNames();      // ['users']
$schema->getMutationNames();   // ['createUser']
$schema->getType('User');      // ['name' => 'User', 'fields' => [...], 'sql_source' => 'v_user']
$schema->hasType('Invoice');   // false
```

**The document is carried verbatim.** `toArray()`, `toJson()` and `saveToFile()` return
what came in — same keys, same order, same values, including keys `JsonSchema` itself knows
nothing about. That is deliberate: the compiler's schema struct denies unknown fields, so a
value type that dropped a key it does not model, or added one it invented, would write a
document `fraiseql compile` refuses. Reading and re-writing a schema is therefore safe:

```php
JsonSchema::loadFromFile('schema.json')->saveToFile('build/schema.json');
// byte-for-byte the same schema; still compiles
```

### Validating before you compile

`Validator` is a cheap pre-flight over the document's shape — type and field names, and
that each type carries a `fields` list. It catches an authoring typo without a round trip
through the CLI. It is not a second compiler:

```php
use FraiseQL\Validator;

$validator = new Validator();

if (!$validator->validateJsonSchema($schema)) {
    fwrite(STDERR, $validator->getReport());
    exit(1);
}
```

`validateRegistry()` and `validateBuilder()` do the same one step earlier, against the
registry or a `TypeBuilder`, before anything is exported.

### Caching across a build

`SchemaCache` keeps a parsed schema, its JSON, and validation results in memory, keyed by
`CacheKey`. It is worth reaching for when a long-running process — a dev server, a build
script that exports repeatedly — would otherwise re-derive the same document:

```php
use FraiseQL\SchemaCache;

$cache = new SchemaCache(ttl: 300);
$registry = SchemaRegistry::getInstance();

$schema = $cache->getFormattedSchema($registry);

if ($schema === null) {
    $schema = JsonSchema::fromJson(SchemaExporter::export());
    $cache->cacheFormattedSchema($registry, $schema);
}

$cache->getHits();   // for build diagnostics
$cache->getMisses();
```

`CacheKey` keys on content: two schemas that differ anywhere key differently, and two
documents differing only in JSON object-key order key the same.

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

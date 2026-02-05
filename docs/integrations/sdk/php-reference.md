# FraiseQL PHP SDK Reference

**Status**: Production-Ready | **PHP Version**: 8.2+ | **SDK Version**: 2.0.0+
**Last Updated**: 2026-02-05 | **Maintained By**: FraiseQL Community

Complete API reference for the FraiseQL PHP SDK. This guide covers the complete PHP authoring interface for building type-safe GraphQL APIs with PHP 8 attributes, readonly classes, and web framework integration (Laravel, Symfony, etc.).

## Installation & Setup

### Composer Installation

```bash
# Add to existing project
composer require fraiseql/sdk "^2.0"

# Or create new project
composer create-project fraiseql/php-starter my-api
```

**Requirements**:
- PHP 8.2 or later
- Composer for dependency management
- Type declarations enabled (strict types recommended)
- Optional: Laravel 10+, Symfony 6.0+, or PSR-12 compliant framework

### PHP Configuration

Create `composer.json` with PSR-4 autoloading:

```json
{
  "name": "my-company/my-api",
  "description": "GraphQL API with FraiseQL",
  "require": {
    "php": "^8.2",
    "fraiseql/sdk": "^2.0"
  },
  "autoload": {
    "psr-4": {
      "MyApp\\": "src/",
      "MyApp\\GraphQL\\Types\\": "src/GraphQL/Types/",
      "MyApp\\GraphQL\\Queries\\": "src/GraphQL/Queries/"
    }
  }
}
```

### First Schema (60 seconds)

```php
<?php
declare(strict_types=1);

namespace MyApp\GraphQL\Types;

use Fraiseql\Attributes\Type;
use Fraiseql\Attributes\Field;

#[Type(name: 'User', description: 'A user account')]
readonly class User
{
    public function __construct(
        #[Field(type: 'ID')]
        public int $id,

        #[Field(type: 'String')]
        public string $name,

        #[Field(type: 'String')]
        public string $email,
    ) {}
}
```

Export and compile:

```bash
vendor/bin/fraiseql export src/GraphQL schema.json
fraiseql-cli compile schema.json fraiseql.toml
fraiseql-server --schema schema.compiled.json
```

---

## Quick Reference Table

| Feature | Attribute | Purpose | Returns |
|---------|-----------|---------|---------|
| **Types** | `#[Type]` | GraphQL object types | JSON schema |
| **Fields** | `#[Field]` | Typed object properties | Field definition |
| **Queries** | `#[Query]` | Read operations (SELECT) | Type or array |
| **Mutations** | `#[Mutation]` | Write operations (INSERT/UPDATE) | Type result |
| **Input Types** | `#[InputType]` | Structured parameters | Input schema |
| **Enums** | `#[Enum]` | Enumeration values | Enum definition |
| **Fact Tables** | `#[FactTable]` | Analytics tables (OLAP) | Aggregation schema |
| **RBAC** | `#[RequireScope]` | Role-based access control | Auth metadata |
| **Validators** | `#[Validate]` | Field validation rules | Validation result |
| **Metadata** | `#[Deprecated], #[FieldMeta]` | Field-level metadata | Schema annotation |

---

## Type System

### 1. The `#[Type]` Attribute

Define GraphQL object types using readonly PHP classes with attributes.

**Signature:**

```php
#[Type(
    name: string = 'ClassName',
    description: string = '',
    table: string = null,  // Database table for mapping
)]
readonly class MyType {}
```

**Key Features**:

- **Readonly Classes**: Immutable value objects (PHP 8.2+)
- **Constructor Property Promotion**: Concise field definitions
- **Type Declarations**: All properties require PHP type hints
- **Nullability**: Use `?Type` or `Type | null` for optional fields
- **Nested Types**: Reference other `#[Type]` classes
- **Field Attributes**: Each property decorated with `#[Field]`

**Examples**:

```php
// ✅ Basic type with readonly properties
#[Type(
    name: 'User',
    description: 'A user account',
    table: 'users'
)]
readonly class User
{
    public function __construct(
        #[Field(type: 'ID', description: 'User ID')]
        public int $id,

        #[Field(type: 'String')]
        public string $username,

        #[Field(type: 'Email')]
        public string $email,

        #[Field(type: 'Boolean')]
        public bool $isActive,

        #[Field(type: 'DateTime')]
        public \DateTime $createdAt,
    ) {}
}

// ✅ Optional fields and nested types
#[Type(name: 'Profile')]
readonly class Profile
{
    public function __construct(
        public int $userId,
        #[Field(type: 'String', nullable: true)]
        public ?string $bio,

        #[Field(type: 'User')]
        public User $user,  // Nested object

        #[Field(type: '[String]')]
        public array $tags,  // Array type
    ) {}
}

// ✅ With docstrings for descriptions
#[Type]
readonly class Product
{
    /**
     * @param int $id The product identifier
     * @param string $name Product display name
     * @param float $price Price in USD
     */
    public function __construct(
        #[Field(type: 'ID')]
        public int $id,

        #[Field(type: 'String')]
        public string $name,

        #[Field(type: 'Float')]
        public float $price,
    ) {}
}
```

### 2. Field Definitions with `#[Field]`

The `#[Field]` attribute defines properties with type and metadata:

```php
#[Field(
    type: string,           // GraphQL type: 'String', 'Int', 'ID', etc.
    description: string = '',
    nullable: bool = false,
    deprecated: string = null,  // "Use X instead" message
)]
```

**Common Field Types**:

```php
readonly class FieldExamples
{
    public function __construct(
        #[Field(type: 'ID')]
        public string $id,  // GraphQL ID type

        #[Field(type: 'String')]
        public string $name,

        #[Field(type: 'Int')]
        public int $count,

        #[Field(type: 'Float')]
        public float $rating,

        #[Field(type: 'Boolean')]
        public bool $isPublished,

        #[Field(type: 'DateTime')]
        public \DateTime $publishedAt,

        #[Field(type: '[String]')]
        public array $tags,  // Array of strings

        #[Field(type: 'String', nullable: true)]
        public ?string $description,  // Optional

        #[Field(
            type: 'String',
            deprecated: 'Use newFieldName instead'
        )]
        public string $legacyField,
    ) {}
}
```

---

## Operations

### Query Operations

Define read-only GraphQL queries that select from database tables:

```php
<?php
declare(strict_types=1);

namespace MyApp\GraphQL\Queries;

use Fraiseql\Attributes\Query;
use MyApp\GraphQL\Types\User;

class UserQueries
{
    #[Query(
        name: 'getUser',
        sql_source: 'users',
        description: 'Get a single user by ID'
    )]
    public static function getUser(
        #[Field(type: 'ID')] int $id,
    ): ?User {
        // Implementation auto-generated by compiler
    }

    #[Query(
        sql_source: 'users',
        limit: 100,
    )]
    public static function listUsers(
        #[Field(type: 'Int', nullable: true)]
        ?int $limit = 10,

        #[Field(type: 'Int', nullable: true)]
        ?int $offset = 0,
    ): array {  // Returns User[]
        // Compiler generates paginated SELECT
    }

    #[Query(sql_source: 'users')]
    public static function searchUsers(
        #[Field(type: 'String')]
        string $query,
    ): array {
        // Full-text search implementation
    }
}
```

### Mutation Operations

Define write operations (INSERT, UPDATE, DELETE):

```php
<?php
namespace MyApp\GraphQL\Mutations;

use Fraiseql\Attributes\Mutation;
use Fraiseql\Attributes\InputType;
use MyApp\GraphQL\Types\User;

#[InputType(name: 'CreateUserInput')]
readonly class CreateUserInput
{
    public function __construct(
        #[Field(type: 'String')] public string $name,
        #[Field(type: 'String')] public string $email,
    ) {}
}

class UserMutations
{
    #[Mutation(
        operation: 'INSERT',
        table: 'users',
    )]
    public static function createUser(
        #[Field(type: 'CreateUserInput')]
        CreateUserInput $input,
    ): User {
        // Compiler generates INSERT statement
    }

    #[Mutation(
        operation: 'UPDATE',
        table: 'users',
    )]
    public static function updateUser(
        #[Field(type: 'ID')] int $id,
        #[Field(type: 'String', nullable: true)] ?string $name,
        #[Field(type: 'String', nullable: true)] ?string $email,
    ): User {
        // Compiler generates UPDATE statement
    }

    #[Mutation(
        operation: 'DELETE',
        table: 'users',
    )]
    public static function deleteUser(
        #[Field(type: 'ID')] int $id,
    ): bool {
        // Compiler generates DELETE statement
    }
}
```

---

## Advanced Features

### Fact Tables for Analytics

Define OLAP fact tables with measures and dimensions:

```php
<?php
namespace MyApp\GraphQL\Analytics;

use Fraiseql\Attributes\FactTable;
use Fraiseql\Attributes\Dimension;
use Fraiseql\Attributes\Measure;

#[FactTable(
    name: 'SalesAnalytics',
    table: 'sales_facts',
    description: 'Sales metrics and analysis'
)]
class SalesAnalytics
{
    #[Dimension(
        name: 'date',
        type: 'Date',
        column: 'sale_date'
    )]
    public \DateTime $date;

    #[Dimension(
        name: 'region',
        type: 'String',
        column: 'region_name'
    )]
    public string $region;

    #[Measure(
        name: 'totalSales',
        type: 'Float',
        aggregation: 'SUM',
        column: 'sale_amount'
    )]
    public float $totalSales;

    #[Measure(
        name: 'averageOrderValue',
        type: 'Float',
        aggregation: 'AVG',
        column: 'order_value'
    )]
    public float $averageOrderValue;

    #[Measure(
        name: 'orderCount',
        type: 'Int',
        aggregation: 'COUNT',
        column: 'order_id'
    )]
    public int $orderCount;
}
```

### Role-Based Access Control (RBAC)

Restrict field access by authentication scopes:

```php
#[Type]
readonly class SensitiveData
{
    public function __construct(
        #[Field(type: 'String')]
        public string $publicField,

        #[Field(
            type: 'String',
            requiresScope: 'admin'
        )]
        public string $adminOnlyField,

        #[Field(
            type: 'String',
            requiresScope: ['admin', 'moderator']
        )]
        public string $staffField,
    ) {}
}
```

### Field Metadata and Validation

Add custom metadata and validation rules:

```php
#[Type(name: 'BlogPost')]
readonly class BlogPost
{
    public function __construct(
        #[Field(type: 'ID')]
        public string $id,

        #[Field(
            type: 'String',
            validate: [
                'minLength' => 1,
                'maxLength' => 200,
            ]
        )]
        public string $title,

        #[Field(
            type: 'String',
            validate: [
                'minLength' => 50,
                'maxLength' => 5000,
            ]
        )]
        public string $content,

        #[Field(
            type: 'String',
            validate: ['pattern' => '^[a-z0-9-]+$']  // URL slug
        )]
        public string $slug,

        #[Field(
            type: 'Int',
            validate: ['min' => 1, 'max' => 5]
        )]
        public int $rating,
    ) {}
}
```

---

## Type Mappings

### PHP ↔ GraphQL Type System

| PHP Type | GraphQL Type | Notes |
|----------|--------------|-------|
| `int` | `Int` | 32-bit signed integer |
| `float` | `Float` | IEEE 754 double precision |
| `string` | `String` | UTF-8 text |
| `bool` | `Boolean` | True/False |
| `\DateTime` | `DateTime` | ISO 8601 format |
| `\DateTimeImmutable` | `DateTime` | Immutable timestamp |
| `\UUID` | `ID` | UUID identifier |
| `array<Type>` | `[Type]` | List/array type |
| `Type \| null` | `Type` (nullable) | Optional field |
| `Enum` | `ENUM_NAME` | Enumeration |

### Scalar Type Definitions

```php
// Built-in scalars
#[Field(type: 'String')]     // UTF-8 string
#[Field(type: 'Int')]        // 32-bit integer
#[Field(type: 'Float')]      // IEEE 754 float
#[Field(type: 'Boolean')]    // true/false
#[Field(type: 'ID')]         // Unique identifier
#[Field(type: 'DateTime')]   // ISO 8601 datetime
#[Field(type: 'Date')]       // Date only (YYYY-MM-DD)
#[Field(type: 'Time')]       // Time only (HH:MM:SS)
#[Field(type: 'JSON')]       // Arbitrary JSON
#[Field(type: 'UUID')]       // RFC 4122 UUID
#[Field(type: 'Email')]      // RFC 5322 email
#[Field(type: 'URL')]        // RFC 3986 URI
```

---

## Schema Export & PSR-4 Integration

### Automatic Schema Discovery

The `#[Type]`, `#[Query]`, `#[Mutation]` attributes are automatically discovered via PSR-4 autoloading:

```bash
# Export schema from all PSR-4 namespaces
vendor/bin/fraiseql export src/GraphQL schema.json

# Or specific directory
vendor/bin/fraiseql export src/GraphQL/Types schema.json
```

### Integration with Laravel

Configure in `config/fraiseql.php`:

```php
return [
    'namespaces' => [
        'types' => 'App\\GraphQL\\Types',
        'queries' => 'App\\GraphQL\\Queries',
        'mutations' => 'App\\GraphQL\\Mutations',
    ],
    'schema_path' => 'database/fraiseql/schema.json',
    'compiled_path' => 'database/fraiseql/schema.compiled.json',
];
```

Use in routes (`routes/api.php`):

```php
use Fraiseql\Laravel\FraiseqlController;

Route::post('/graphql', FraiseqlController::class);
```

### Integration with Symfony

Configure in `config/packages/fraiseql.yaml`:

```yaml
fraiseql:
  namespaces:
    types: 'App\GraphQL\Types'
    queries: 'App\GraphQL\Queries'
    mutations: 'App\GraphQL\Mutations'
  schema_path: '%kernel.project_dir%/var/fraiseql/schema.json'
```

Use in controller:

```php
<?php
namespace App\Controller;

use Fraiseql\Symfony\FraiseqlService;
use Symfony\Bundle\FrameworkBundle\Controller\AbstractController;

class GraphQLController extends AbstractController
{
    public function __construct(
        private FraiseqlService $fraiseql,
    ) {}

    public function query(): Response
    {
        return $this->fraiseql->handle($request);
    }
}
```

---

## Common Patterns

### CRUD Operations

```php
<?php
namespace MyApp\GraphQL;

use Fraiseql\Attributes\Query;
use Fraiseql\Attributes\Mutation;
use MyApp\GraphQL\Types\Post;

class PostOperations
{
    // CREATE
    #[Mutation(operation: 'INSERT', table: 'posts')]
    public static function createPost(
        #[Field(type: 'String')] string $title,
        #[Field(type: 'String')] string $content,
    ): Post {}

    // READ
    #[Query(sql_source: 'posts')]
    public static function getPost(
        #[Field(type: 'ID')] int $id,
    ): ?Post {}

    // UPDATE
    #[Mutation(operation: 'UPDATE', table: 'posts')]
    public static function updatePost(
        #[Field(type: 'ID')] int $id,
        #[Field(type: 'String', nullable: true)] ?string $title,
        #[Field(type: 'String', nullable: true)] ?string $content,
    ): Post {}

    // DELETE
    #[Mutation(operation: 'DELETE', table: 'posts')]
    public static function deletePost(
        #[Field(type: 'ID')] int $id,
    ): bool {}

    // LIST with pagination
    #[Query(sql_source: 'posts')]
    public static function listPosts(
        #[Field(type: 'Int')] int $limit = 10,
        #[Field(type: 'Int')] int $offset = 0,
    ): array {}
}
```

### Filtering and Pagination

```php
#[InputType(name: 'PostFilter')]
readonly class PostFilter
{
    public function __construct(
        #[Field(type: 'String', nullable: true)]
        public ?string $searchTerm,

        #[Field(type: 'String', nullable: true)]
        public ?string $status,  // 'draft', 'published', 'archived'
    ) {}
}

class PostQueries
{
    #[Query(sql_source: 'posts')]
    public static function searchPosts(
        #[Field(type: 'PostFilter', nullable: true)]
        ?PostFilter $filter,

        #[Field(type: 'Int')] int $limit = 10,
        #[Field(type: 'Int')] int $offset = 0,
    ): array {}
}
```

---

## Error Handling

### Exception Hierarchy

FraiseQL provides typed exceptions following PSR standards:

```php
use Fraiseql\Exception\FraiseQLException;
use Fraiseql\Exception\ValidationException;
use Fraiseql\Exception\NotFoundException;
use Fraiseql\Exception\UnauthorizedException;

try {
    $result = $query->execute();
} catch (ValidationException $e) {
    // Field validation failed
    log_error('Validation failed: ' . $e->getMessage());
} catch (NotFoundException $e) {
    // Resource not found
    http_response_code(404);
} catch (UnauthorizedException $e) {
    // RBAC scope check failed
    http_response_code(403);
} catch (FraiseQLException $e) {
    // General FraiseQL error
    http_response_code(500);
}
```

---

## Testing with PHPUnit

### Basic Test Pattern

```php
<?php
namespace Tests\GraphQL;

use PHPUnit\Framework\TestCase;
use Fraiseql\Testing\SchemaTestCase;
use MyApp\GraphQL\Types\User;

class UserQueriesTest extends SchemaTestCase
{
    public function testGetUserReturnsValidType(): void
    {
        $result = $this->query('getUser', ['id' => 1]);

        $this->assertInstanceOf(User::class, $result);
        $this->assertEquals(1, $result->id);
    }

    public function testCreateUserRequiresValidEmail(): void
    {
        $this->expectException(ValidationException::class);

        $this->mutation('createUser', [
            'name' => 'John',
            'email' => 'invalid-email',
        ]);
    }
}
```

---

## See Also

- [Type System Guide](../../../docs/TYPE_SYSTEM.md)
- [Schema Design Patterns](../../../docs/DESIGN_PATTERNS.md)
- [RBAC Implementation](../../../docs/SECURITY_PATTERNS.md)
- [Laravel Integration](./laravel-integration.md)
- [Symfony Integration](./symfony-integration.md)
- [Testing Guide](../../../docs/TESTING_GUIDE.md)
- [API Reference](https://github.com/fraiseql/fraiseql/tree/dev/docs/api)

---

## Troubleshooting

### Common Setup Issues

#### Composer Issues

**Issue**: `Could not find package fraiseql/fraiseql`

**Solution**:
```bash
composer require fraiseql/fraiseql "^2.0"
composer update
composer dump-autoload
```

#### Autoloader Issues

**Issue**: `Class 'FraiseQL\Server' not found`

**Solution - Require autoloader**:
```php
<?php
require 'vendor/autoload.php';

use FraiseQL\Server;
$server = Server::fromCompiled('schema.compiled.json');
```

#### PHP Version Issues

**Issue**: `Parse error: syntax error, unexpected ...`

**Check PHP version** (7.4+ required):
```bash
php --version
```

**Update PHP**:
```bash
php -v
# Or use version manager
phpenv versions
phpenv global 8.2
```

#### Extension Issues

**Issue**: `Warning: Module compiled for PHP 8.1.0 API ... but loaded into ...`

**Solution - Rebuild extensions**:
```bash
pecl install fraiseql
php -m | grep fraiseql
```

---

### Type System Issues

#### Type Declaration Errors

**Issue**: `Uncaught TypeError: Return value must be of type array`

**Solution - Use proper types**:
```php
// ✅ Declare return types
public function getUsers(): array {
    return $this->server->execute($query);
}

// ✅ Or nullable
public function getUser(int $id): ?User {
    return $this->server->execute($query);
}
```

#### Attribute Issues

**Issue**: `Error: Call to undefined method attribute()`

**Solution - Use correct syntax**:
```php
// ✅ PHP 8 attributes
#[FraiseQL\Type]
class User {
    public int $id;
    public string $email;
}

// Or docblock for PHP 7.4
/**
 * @FraiseQL\Type
 */
class User {
}
```

#### Class Type Issues

**Issue**: `Undefined class FraiseQL\Type`

**Solution - Import classes**:
```php
<?php
use FraiseQL\Type;
use FraiseQL\Query;

#[Type]
class User { }

#[Query(sqlSource: 'v_users')]
function getUsers(): array { }
```

---

### Runtime Errors

#### Connection Issues

**Issue**: `PDOException: SQLSTATE[HY000]: General error`

**Check environment**:
```bash
echo $DATABASE_URL
```

**Solution - Configure database**:
```php
$dotenv = \Dotenv\Dotenv::createImmutable(__DIR__);
$dotenv->load();

$server = \FraiseQL\Server::fromCompiled(
    'schema.compiled.json',
    [
        'database_url' => $_ENV['DATABASE_URL']
    ]
);
```

#### Session Issues

**Issue**: `Cannot modify header information`

**Solution - Set headers early**:
```php
<?php
// Must be first
header('Content-Type: application/json');
header('Access-Control-Allow-Origin: *');

// Then your code
$result = $server->execute($query);
echo json_encode($result);
```

#### Variable Type Mismatch

**Issue**: `Unexpected type for argument`

**Solution - Cast properly**:
```php
// ✅ Correct types
$result = $server->execute(
    query: $query,
    variables: ['id' => (int)$id]  // Must be int, not string
);
```

---

### Performance Issues

#### Slow Execution

**Issue**: Queries take >5 seconds

**Enable caching**:
```php
$server = \FraiseQL\Server::fromCompiled(
    'schema.compiled.json',
    ['cache_ttl' => 300]  // 5 minutes
);
```

#### Memory Issues

**Issue**: `Fatal error: Allowed memory size of X bytes exhausted`

**Increase memory limit**:
```php
ini_set('memory_limit', '512M');

// Or in php.ini
memory_limit = 512M
```

**Or check for leaks**:
```php
echo memory_get_usage() . " bytes\n";
$result = $server->execute($query);
echo memory_get_usage() . " bytes\n";
```

---

### Debugging Techniques

#### Error Reporting

```php
<?php
error_reporting(E_ALL);
ini_set('display_errors', 1);

// Or log errors
ini_set('log_errors', 1);
ini_set('error_log', '/var/log/php-error.log');
```

#### var_dump Debugging

```php
<?php
$result = $server->execute($query);
var_dump($result);

// Or pretty print
echo json_encode($result, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES);
```

#### Xdebug Profiling

```php
<?php
xdebug_start_code_coverage();

$result = $server->execute($query);

$coverage = xdebug_get_code_coverage();
var_dump($coverage);
```

---

### Getting Help

Provide: 1. PHP version: `php -v`
2. Composer version: `composer --version`
3. FraiseQL version: `composer show fraiseql/fraiseql`
4. Error message
5. Minimal code example

---

**Last Reviewed**: February 2026 | **Version**: 2.0.0+ | **License**: Apache 2.0

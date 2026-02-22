<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use PHPUnit\Framework\TestCase;
use FraiseQL\SchemaRegistry;
use FraiseQL\TypeBuilder;
use FraiseQL\SchemaFormatter;
use FraiseQL\JsonSchema;
use FraiseQL\Attributes\GraphQLType;
use FraiseQL\Attributes\GraphQLField;
use FraiseQL\StaticAPI;

/**
 * Integration tests for real-world usage patterns.
 */
final class IntegrationTest extends TestCase
{
    protected function tearDown(): void
    {
        SchemaRegistry::getInstance()->clear();
        parent::tearDown();
    }

    public function testSimpleBlogSchema(): void
    {
        // Define Blog schema
        SchemaRegistry::getInstance()->register(BlogUser::class);
        SchemaRegistry::getInstance()->register(BlogPost::class);

        $formatter = new SchemaFormatter();
        $schema = $formatter->formatRegistry(SchemaRegistry::getInstance());

        // Verify schema structure
        $this->assertTrue($schema->hasType('BlogUser'));
        $this->assertTrue($schema->hasType('BlogPost'));
        $this->assertCount(2, $schema->getTypeNames());

        // Verify User type
        $userType = $schema->getType('BlogUser');
        $this->assertIsArray($userType);
        $this->assertArrayHasKey('fields', $userType);
        $this->assertCount(3, $userType['fields']);

        // Verify Post type
        $postType = $schema->getType('BlogPost');
        $this->assertIsArray($postType);
        $this->assertArrayHasKey('fields', $postType);
        $this->assertCount(5, $postType['fields']);
    }

    public function testComplexSchemaWithBuilder(): void
    {
        // Create complex schema with builder
        $userType = TypeBuilder::type('User')
            ->scalarField('id', 'Int', 'User ID')
            ->scalarField('name', 'String', 'User name')
            ->optionalField('email', 'String', 'Email address')
            ->listField('roles', 'String', 'User roles');

        $queryType = TypeBuilder::type('Query')
            ->field('user', 'User', nullable: true, description: 'Get user by ID')
            ->field('users', 'User', isList: true, description: 'List all users')
            ->scalarField('status', 'String', 'API status');

        $formatter = new SchemaFormatter();
        $schema = $formatter->formatBuilders($userType, $queryType);

        // Verify schema
        $this->assertCount(2, $schema->getTypeNames());
        $this->assertTrue($schema->hasType('User'));
        $this->assertTrue($schema->hasType('Query'));

        // Verify User type structure
        $user = $schema->getType('User');
        $this->assertSame('Int!', $user['fields']['id']['type']);
        $this->assertSame('String!', $user['fields']['name']['type']);
        $this->assertSame('String', $user['fields']['email']['type']);
        $this->assertSame('[String!]', $user['fields']['roles']['type']);
    }

    public function testSchemaWithMetadata(): void
    {
        $builder = TypeBuilder::type('Product')
            ->scalarField('id', 'Int')
            ->scalarField('name', 'String')
            ->scalarField('price', 'Float');

        $formatter = new SchemaFormatter();
        $schema = $formatter->formatBuilder($builder);

        // Save with metadata
        $metadata = [
            'author' => 'Test User',
            'version' => '1.0.0',
            'created' => date('Y-m-d H:i:s'),
        ];

        $schemaWithMeta = new JsonSchema(
            version: $schema->version,
            types: $schema->types,
            scalars: $schema->scalars,
            description: 'Product catalog',
            metadata: $metadata,
        );

        $array = $schemaWithMeta->toArray();
        $this->assertArrayHasKey('metadata', $array);
        $this->assertSame('Test User', $array['metadata']['author']);
        $this->assertSame('1.0.0', $array['metadata']['version']);
    }

    public function testSchemaExportAndReimport(): void
    {
        SchemaRegistry::getInstance()->register(IntegrationUser::class);
        SchemaRegistry::getInstance()->register(IntegrationPost::class);

        $formatter = new SchemaFormatter();
        $schema = $formatter->formatRegistry(
            SchemaRegistry::getInstance(),
            description: 'Test schema'
        );

        // Export to JSON
        $json = $schema->toJson();
        $this->assertIsString($json);
        $this->assertStringContainsString('IntegrationUser', $json);
        $this->assertStringContainsString('IntegrationPost', $json);

        // Re-import from JSON
        $reimported = JsonSchema::fromJson($json);
        $this->assertSame($schema->version, $reimported->version);
        $this->assertSame($schema->getTypeCount(), $reimported->getTypeCount());
        $this->assertSame($schema->description, $reimported->description);

        // Verify data integrity
        foreach ($schema->getTypeNames() as $typeName) {
            $this->assertTrue($reimported->hasType($typeName));
            $original = $schema->getType($typeName);
            $imported = $reimported->getType($typeName);
            $this->assertSame($original['name'], $imported['name']);
            $this->assertCount(count($original['fields']), $imported['fields']);
        }
    }

    public function testMultipleSchemaVersions(): void
    {
        $v1Builder = TypeBuilder::type('API')
            ->scalarField('version', 'String', 'API version')
            ->scalarField('status', 'String', 'API status');

        $formatter = new SchemaFormatter();
        $v1Schema = $formatter->formatBuilder($v1Builder);

        // Create v2 with more fields
        $v2Builder = TypeBuilder::type('API')
            ->scalarField('version', 'String', 'API version')
            ->scalarField('status', 'String', 'API status')
            ->scalarField('uptime', 'Int', 'Uptime in seconds')
            ->optionalField('lastUpdated', 'String', 'Last update time');

        $v2Schema = $formatter->formatBuilder($v2Builder);

        // Compare versions
        $v1Type = $v1Schema->getType('API');
        $v2Type = $v2Schema->getType('API');

        $this->assertCount(2, $v1Type['fields']);
        $this->assertCount(4, $v2Type['fields']);

        // v2 has superset of v1 fields
        foreach (array_keys($v1Type['fields']) as $fieldName) {
            $this->assertArrayHasKey($fieldName, $v2Type['fields']);
        }
    }

    public function testStaticAPIWorkflow(): void
    {
        // Use StaticAPI for convenient access
        StaticAPI::register(StaticUser::class);
        StaticAPI::register(StaticPost::class);

        // Verify registration
        $this->assertTrue(StaticAPI::hasType('StaticUser'));
        $this->assertTrue(StaticAPI::hasType('StaticPost'));
        $this->assertCount(2, StaticAPI::getTypeNames());

        // Get fields via StaticAPI
        $userFields = StaticAPI::getTypeFields('StaticUser');
        $this->assertCount(3, $userFields);

        $postFields = StaticAPI::getTypeFields('StaticPost');
        $this->assertCount(4, $postFields);

        // Get specific field
        $userIdField = StaticAPI::getField('StaticUser', 'id');
        $this->assertSame('Int', $userIdField->type);
        $this->assertFalse($userIdField->nullable);

        // Export full schema
        $registry = SchemaRegistry::getInstance();
        $formatter = new SchemaFormatter();
        $schema = $formatter->formatRegistry($registry);

        $this->assertCount(2, $schema->getTypeCount());
    }

    public function testComplexNestedSchema(): void
    {
        // Build a realistic social media schema
        $postBuilder = TypeBuilder::type('Post')
            ->scalarField('id', 'Int', 'Post ID')
            ->scalarField('content', 'String', 'Post content')
            ->scalarField('likes', 'Int', 'Number of likes')
            ->optionalField('image', 'String', 'Image URL');

        $authorBuilder = TypeBuilder::type('Author')
            ->scalarField('id', 'Int', 'Author ID')
            ->scalarField('username', 'String', 'Username')
            ->optionalField('bio', 'String', 'Bio');

        $feedBuilder = TypeBuilder::type('Feed')
            ->field('posts', 'Post', isList: true, description: 'Posts in feed')
            ->field('author', 'Author', description: 'Feed owner');

        $formatter = new SchemaFormatter();
        $schema = $formatter->formatBuilders($postBuilder, $authorBuilder, $feedBuilder);

        // Verify all types exist
        $this->assertCount(3, $schema->getTypeNames());
        $this->assertTrue($schema->hasType('Post'));
        $this->assertTrue($schema->hasType('Author'));
        $this->assertTrue($schema->hasType('Feed'));

        // Verify Feed references other types
        $feed = $schema->getType('Feed');
        $this->assertSame('[Post!]', $feed['fields']['posts']['type']);
        $this->assertSame('Author!', $feed['fields']['author']['type']);
    }

    public function testSchemaWithResolvers(): void
    {
        $userBuilder = TypeBuilder::type('User')
            ->scalarField('id', 'Int')
            ->scalarField('firstName', 'String')
            ->scalarField('lastName', 'String')
            ->field('fullName', 'String')
            ->withResolver('fullName', 'getFullName');

        $formatter = new SchemaFormatter();
        $schema = $formatter->formatBuilder($userBuilder);

        $userType = $schema->getType('User');
        $this->assertArrayHasKey('resolver', $userType['fields']['fullName']);
        $this->assertSame('getFullName', $userType['fields']['fullName']['resolver']);

        // Other fields should not have resolvers
        $this->assertArrayNotHasKey('resolver', $userType['fields']['firstName']);
    }

    public function testLargeSchemaPerformance(): void
    {
        // Build a large schema programmatically
        $builders = [];

        for ($i = 1; $i <= 10; $i++) {
            $builder = TypeBuilder::type("Entity$i")
                ->scalarField('id', 'Int')
                ->scalarField('name', 'String')
                ->scalarField('value', 'Float')
                ->optionalField('description', 'String');

            $builders[] = $builder;
        }

        $formatter = new SchemaFormatter();
        $startTime = microtime(true);
        $schema = $formatter->formatBuilders(...$builders);
        $duration = microtime(true) - $startTime;

        // Should complete quickly
        $this->assertLessThan(0.1, $duration);
        $this->assertCount(10, $schema->getTypeNames());

        // Verify all types
        for ($i = 1; $i <= 10; $i++) {
            $this->assertTrue($schema->hasType("Entity$i"));
            $entity = $schema->getType("Entity$i");
            $this->assertCount(4, $entity['fields']);
        }
    }

    public function testSchemaFileOperations(): void
    {
        $builder = TypeBuilder::type('Sample')
            ->scalarField('id', 'Int')
            ->scalarField('name', 'String');

        $formatter = new SchemaFormatter();
        $schema = $formatter->formatBuilder($builder);

        $tmpFile = tempnam(sys_get_temp_dir(), 'fraiseql_');

        try {
            // Save to file
            $schema->saveToFile($tmpFile);
            $this->assertFileExists($tmpFile);

            // Load from file
            $loaded = JsonSchema::loadFromFile($tmpFile);
            $this->assertSame($schema->version, $loaded->version);
            $this->assertSame($schema->getTypeCount(), $loaded->getTypeCount());
            $this->assertTrue($loaded->hasType('Sample'));

            // Verify content is identical
            $json = $schema->toJson();
            $loadedJson = $loaded->toJson();
            $this->assertSame(json_decode($json), json_decode($loadedJson));
        } finally {
            if (file_exists($tmpFile)) {
                unlink($tmpFile);
            }
        }
    }
}

// Test fixtures
#[GraphQLType(name: 'BlogUser')]
final class BlogUser
{
    #[GraphQLField(type: 'Int')]
    public int $id;

    #[GraphQLField(type: 'String')]
    public string $name;

    #[GraphQLField(type: 'String', nullable: true)]
    public ?string $email;
}

#[GraphQLType(name: 'BlogPost')]
final class BlogPost
{
    #[GraphQLField(type: 'Int')]
    public int $id;

    #[GraphQLField(type: 'String')]
    public string $title;

    #[GraphQLField(type: 'String')]
    public string $content;

    #[GraphQLField(type: 'BlogUser')]
    public BlogUser $author;

    #[GraphQLField(type: 'String', nullable: true)]
    public ?string $publishedAt;
}

#[GraphQLType(name: 'IntegrationUser')]
final class IntegrationUser
{
    #[GraphQLField(type: 'Int')]
    public int $id;

    #[GraphQLField(type: 'String')]
    public string $username;

    #[GraphQLField(type: 'String', nullable: true)]
    public ?string $email;
}

#[GraphQLType(name: 'IntegrationPost')]
final class IntegrationPost
{
    #[GraphQLField(type: 'Int')]
    public int $id;

    #[GraphQLField(type: 'String')]
    public string $title;

    #[GraphQLField(type: 'IntegrationUser')]
    public IntegrationUser $author;

    #[GraphQLField(type: 'String', nullable: true)]
    public ?string $content;
}

#[GraphQLType(name: 'StaticUser')]
final class StaticUser
{
    #[GraphQLField(type: 'Int')]
    public int $id;

    #[GraphQLField(type: 'String')]
    public string $name;

    #[GraphQLField(type: 'String', nullable: true)]
    public ?string $email;
}

#[GraphQLType(name: 'StaticPost')]
final class StaticPost
{
    #[GraphQLField(type: 'Int')]
    public int $id;

    #[GraphQLField(type: 'String')]
    public string $title;

    #[GraphQLField(type: 'StaticUser')]
    public StaticUser $author;

    #[GraphQLField(type: 'String', nullable: true)]
    public ?string $content;
}

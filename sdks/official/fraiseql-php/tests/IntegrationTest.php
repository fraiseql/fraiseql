<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use PHPUnit\Framework\TestCase;
use FraiseQL\SchemaExporter;
use FraiseQL\SchemaRegistry;
use FraiseQL\TypeBuilder;
use FraiseQL\JsonSchema;
use FraiseQL\Attributes\GraphQLType;
use FraiseQL\Attributes\GraphQLField;
use FraiseQL\StaticAPI;

/**
 * Integration tests for real-world usage patterns.
 *
 * Every case here used to assert against `SchemaFormatter`, whose document
 * `fraiseql compile` refuses on the first field — no `name`, no `nullable`, a `resolver`
 * key `IntermediateField` has no member for, fields as a map where the compiler reads a
 * list, and `schema_version` 1.0 where the format is 2.0.0. So the suite's "real-world
 * usage patterns" pinned a shape no real use could compile, and pinned it in detail
 * (#1245).
 *
 * They now assert against `SchemaExporter`, which is what `bin/fraiseql export` runs and
 * what the cross-SDK conformance suite compiles. The assertions are the same facts —
 * these types exist, with these fields, of these types — read off the document that is
 * actually consumed.
 */
final class IntegrationTest extends TestCase
{
    protected function tearDown(): void
    {
        SchemaRegistry::getInstance()->clear();
        parent::tearDown();
    }

    /**
     * The exported document, decoded.
     *
     * @return array<string, mixed>
     */
    private static function exported(): array
    {
        $decoded = json_decode(SchemaExporter::export(), true);
        self::assertIsArray($decoded);

        return $decoded;
    }

    /**
     * One exported type, by name. `types` is a LIST in the compiler's shape, not a map
     * keyed by name — which is one of the four ways the old formatter's document was
     * wrong, so indexing it here is the assertion, not a convenience.
     *
     * @param array<string, mixed> $schema
     * @return array<string, mixed>
     */
    private static function type(array $schema, string $name): array
    {
        foreach ($schema['types'] ?? [] as $type) {
            if (($type['name'] ?? null) === $name) {
                return $type;
            }
        }
        self::fail("type '{$name}' is not in the exported schema");
    }

    /**
     * A type's fields as name => field, for assertions that read better keyed.
     *
     * @param array<string, mixed> $type
     * @return array<string, array<string, mixed>>
     */
    private static function fields(array $type): array
    {
        $byName = [];
        foreach ($type['fields'] ?? [] as $field) {
            $byName[$field['name']] = $field;
        }

        return $byName;
    }

    public function testSimpleBlogSchema(): void
    {
        SchemaRegistry::getInstance()->register(BlogUser::class);
        SchemaRegistry::getInstance()->register(BlogPost::class);

        $schema = self::exported();

        $this->assertSame('2.0.0', $schema['version']);
        $this->assertSame(
            ['BlogUser', 'BlogPost'],
            array_column($schema['types'], 'name'),
        );

        $this->assertCount(3, self::type($schema, 'BlogUser')['fields']);
        $this->assertCount(5, self::type($schema, 'BlogPost')['fields']);
    }

    public function testComplexSchemaWithBuilder(): void
    {
        TypeBuilder::type('User')
            ->scalarField('id', 'Int', 'User ID')
            ->scalarField('name', 'String', 'User name')
            ->optionalField('email', 'String', 'Email address')
            ->listField('roles', 'String', 'User roles')
            ->register();

        $fields = self::fields(self::type(self::exported(), 'User'));

        // The compiler reads `type` and `nullable` as separate members; the old formatter
        // encoded nullability into a GraphQL type string (`Int!`, `[String!]`) that
        // `IntermediateField` has no member for.
        $this->assertSame(['Int', false], [$fields['id']['type'], $fields['id']['nullable']]);
        $this->assertSame(['String', false], [$fields['name']['type'], $fields['name']['nullable']]);
        $this->assertSame(['String', true], [$fields['email']['type'], $fields['email']['nullable']]);
        $this->assertSame('String', $fields['roles']['type']);
    }

    public function testSchemaWithMetadata(): void
    {
        // `JsonSchema` is a value type and stays; it simply has no producer in the SDK
        // now that the formatter is gone. This case is about its metadata handling.
        $schema = new JsonSchema(
            version: '2.0.0',
            types: ['Product' => ['name' => 'Product', 'fields' => []]],
            scalars: ['Float' => 'Float scalar type'],
            description: 'Product catalog',
            metadata: ['author' => 'Test User', 'version' => '1.0.0'],
        );

        $array = $schema->toArray();
        $this->assertArrayHasKey('metadata', $array);
        $this->assertSame('Test User', $array['metadata']['author']);
        $this->assertSame('1.0.0', $array['metadata']['version']);
    }

    public function testSchemaExportAndReimport(): void
    {
        SchemaRegistry::getInstance()->register(IntegrationUser::class);
        SchemaRegistry::getInstance()->register(IntegrationPost::class);

        $json = SchemaExporter::export();
        $this->assertStringContainsString('IntegrationUser', $json);
        $this->assertStringContainsString('IntegrationPost', $json);

        // The export is the compiler's own input format, so re-reading it must give back
        // the same document — this is the round-trip that matters, not one through a
        // second in-memory representation.
        $reimported = json_decode($json, true);
        $this->assertSame(json_decode($json, true), $reimported);
        $this->assertSame(
            ['IntegrationUser', 'IntegrationPost'],
            array_column($reimported['types'], 'name'),
        );
    }

    public function testMultipleSchemaVersions(): void
    {
        TypeBuilder::type('API')
            ->scalarField('version', 'String', 'API version')
            ->scalarField('status', 'String', 'API status')
            ->register();

        $v1 = self::fields(self::type(self::exported(), 'API'));
        $this->assertSame(['version', 'status'], array_keys($v1));

        SchemaRegistry::getInstance()->clear();

        TypeBuilder::type('API')
            ->scalarField('version', 'String', 'API version')
            ->scalarField('status', 'String', 'API status')
            ->scalarField('uptime', 'Int', 'Uptime in seconds')
            ->optionalField('lastUpdated', 'String', 'Last update time')
            ->register();

        $v2 = self::fields(self::type(self::exported(), 'API'));
        $this->assertCount(4, $v2);

        foreach (array_keys($v1) as $fieldName) {
            $this->assertArrayHasKey($fieldName, $v2);
        }
    }

    public function testStaticAPIWorkflow(): void
    {
        StaticAPI::register(StaticUser::class);
        StaticAPI::register(StaticPost::class);

        $this->assertTrue(StaticAPI::hasType('StaticUser'));
        $this->assertTrue(StaticAPI::hasType('StaticPost'));
        $this->assertCount(2, StaticAPI::getTypeNames());

        $this->assertCount(3, StaticAPI::getTypeFields('StaticUser'));
        $this->assertCount(4, StaticAPI::getTypeFields('StaticPost'));

        $userIdField = StaticAPI::getField('StaticUser', 'id');
        $this->assertSame('Int', $userIdField->type);
        $this->assertFalse($userIdField->nullable);

        $this->assertCount(2, self::exported()['types']);
    }

    public function testComplexNestedSchema(): void
    {
        TypeBuilder::type('Post')
            ->scalarField('id', 'Int', 'Post ID')
            ->scalarField('content', 'String', 'Post content')
            ->optionalField('image', 'String', 'Image URL')
            ->register();
        TypeBuilder::type('Author')
            ->scalarField('id', 'Int', 'Author ID')
            ->scalarField('username', 'String', 'Username')
            ->register();
        TypeBuilder::type('Feed')
            ->field('posts', 'Post', isList: true, description: 'Posts in feed')
            ->field('author', 'Author', description: 'Feed owner')
            ->register();

        $schema = self::exported();
        $this->assertSame(['Post', 'Author', 'Feed'], array_column($schema['types'], 'name'));

        // A field whose type is another declared type keeps that type's name, so the
        // reference survives to the compiler rather than being flattened into a string.
        $feed = self::fields(self::type($schema, 'Feed'));
        $this->assertSame('Post', $feed['posts']['type']);
        $this->assertSame('Author', $feed['author']['type']);
    }

    public function testACustomResolverDoesNotReachTheCompiledDocument(): void
    {
        TypeBuilder::type('User')
            ->scalarField('id', 'Int')
            ->scalarField('firstName', 'String')
            ->field('fullName', 'String')
            ->withResolver('fullName', 'getFullName')
            ->register();

        $fields = self::fields(self::type(self::exported(), 'User'));

        // This case asserted the opposite: that `resolver` was IN the document. It was —
        // in the formatter's document, which the compiler refuses, because
        // `IntermediateField` has no `resolver` member and denies unknown fields (#1245).
        //
        // `SchemaExporter` drops it, which is what keeps the document compilable. But
        // that makes `withResolver`/`customResolver` an authoring surface no compile path
        // reads — the author declares a resolver and nothing anywhere says it will not
        // run. Pinned as the current honest behaviour and filed as #1263.
        $this->assertArrayNotHasKey('resolver', $fields['fullName']);
        $this->assertArrayNotHasKey('resolver', $fields['firstName']);
    }

    public function testLargeSchemaExport(): void
    {
        for ($i = 1; $i <= 10; $i++) {
            TypeBuilder::type("Entity$i")
                ->scalarField('id', 'Int')
                ->scalarField('name', 'String')
                ->scalarField('value', 'Float')
                ->optionalField('description', 'String')
                ->register();
        }

        $schema = self::exported();
        $this->assertCount(10, $schema['types']);

        for ($i = 1; $i <= 10; $i++) {
            $this->assertCount(4, self::type($schema, "Entity$i")['fields']);
        }
    }

    public function testSchemaFileOperations(): void
    {
        TypeBuilder::type('Sample')
            ->scalarField('id', 'Int')
            ->scalarField('name', 'String')
            ->register();

        $tmpFile = tempnam(sys_get_temp_dir(), 'fraiseql_');

        try {
            file_put_contents($tmpFile, SchemaExporter::export());
            $this->assertFileExists($tmpFile);

            $loaded = json_decode((string) file_get_contents($tmpFile), true);
            $this->assertSame('2.0.0', $loaded['version']);
            $this->assertSame(['Sample'], array_column($loaded['types'], 'name'));
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

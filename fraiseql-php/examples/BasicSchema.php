<?php

declare(strict_types=1);

namespace FraiseQL\Examples;

use FraiseQL\Attributes\GraphQLType;
use FraiseQL\Attributes\GraphQLField;
use FraiseQL\StaticAPI;
use FraiseQL\SchemaFormatter;

/**
 * Basic example demonstrating core FraiseQL PHP features.
 *
 * This example shows how to:
 * - Define types using PHP 8 attributes
 * - Use the StaticAPI for convenient access
 * - Export schema to JSON format
 */

// Define types using PHP 8 attributes
#[GraphQLType(name: 'User', description: 'Represents a user in the system')]
final class User
{
    #[GraphQLField(type: 'Int', description: 'Unique user ID')]
    public int $id;

    #[GraphQLField(type: 'String', description: 'User full name')]
    public string $name;

    #[GraphQLField(type: 'String', nullable: true, description: 'User email address')]
    public ?string $email;

    #[GraphQLField(type: 'Boolean', description: 'Whether user is active')]
    public bool $active;
}

#[GraphQLType(name: 'Post', description: 'Represents a blog post')]
final class Post
{
    #[GraphQLField(type: 'Int', description: 'Unique post ID')]
    public int $id;

    #[GraphQLField(type: 'String', description: 'Post title')]
    public string $title;

    #[GraphQLField(type: 'String', description: 'Post content')]
    public string $content;

    #[GraphQLField(type: 'User', description: 'Author of the post')]
    public User $author;

    #[GraphQLField(type: 'String', nullable: true, description: 'Publication date')]
    public ?string $publishedAt;
}

#[GraphQLType(name: 'Query', description: 'Root query type')]
final class Query
{
    #[GraphQLField(type: 'User', description: 'Get a user by ID')]
    public User $user;

    #[GraphQLField(type: 'Post', description: 'Get posts by author')]
    public Post $posts;
}

// Example usage
function demonstrateBasicSchema(): void
{
    echo "=== FraiseQL PHP Basic Schema Example ===\n\n";

    // Register types using StaticAPI
    echo "Step 1: Registering types...\n";
    StaticAPI::register(User::class);
    StaticAPI::register(Post::class);
    StaticAPI::register(Query::class);
    echo "✓ Registered 3 types\n\n";

    // Verify types are registered
    echo "Step 2: Verifying types...\n";
    echo "Registered types: " . implode(', ', StaticAPI::getTypeNames()) . "\n";
    echo "Total types: " . count(StaticAPI::getTypeNames()) . "\n\n";

    // Inspect individual types
    echo "Step 3: Inspecting User type...\n";
    $userFields = StaticAPI::getTypeFields('User');
    echo "User has " . count($userFields) . " fields:\n";
    foreach ($userFields as $field) {
        $type = $field->getGraphQLTypeString();
        $description = $field->description ?? '(no description)';
        echo "  - {$field->name}: {$type} - {$description}\n";
    }
    echo "\n";

    // Export schema to JSON
    echo "Step 4: Exporting to JSON...\n";
    $registry = \FraiseQL\SchemaRegistry::getInstance();
    $formatter = new SchemaFormatter();
    $schema = $formatter->formatRegistry(
        $registry,
        description: 'Basic blog platform schema'
    );

    $json = $schema->toJson();
    echo "Schema exported successfully!\n";
    echo "Schema size: " . strlen($json) . " bytes\n";
    echo "Version: " . $schema->version . "\n";
    echo "Type count: " . $schema->getTypeCount() . "\n";
    echo "Scalars: " . implode(', ', $schema->getScalarNames()) . "\n\n";

    // Show JSON structure
    echo "Step 5: JSON Schema structure:\n";
    $schemaArray = $schema->toArray();
    echo json_encode($schemaArray, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES) . "\n";
}

// Only run when executed directly
if (basename(__FILE__) === basename($_SERVER['SCRIPT_NAME'] ?? '')) {
    try {
        demonstrateBasicSchema();
    } catch (\Exception $e) {
        echo "Error: " . $e->getMessage() . "\n";
        exit(1);
    }
}

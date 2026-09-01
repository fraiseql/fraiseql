<?php

declare(strict_types=1);

namespace FraiseQL\Examples;

// Composer's autoloader. Without it these files declared everything correctly and
// died on the first `StaticAPI` reference, so they had never been run (#925).
require_once __DIR__ . '/../vendor/autoload.php';

use FraiseQL\Attributes\GraphQLType;
use FraiseQL\Attributes\GraphQLField;
use FraiseQL\StaticAPI;
use FraiseQL\SchemaExporter;

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

    // Export schema to JSON.
    //
    // `SchemaExporter` is the export path — it is what `bin/fraiseql export` runs and
    // what `fraiseql compile` can read. This example used to call `SchemaFormatter`,
    // whose document the compiler refused on the first field (no `name`, no `nullable`,
    // a `resolver` key `IntermediateField` has no member for, and fields as a map where
    // the compiler reads a list), while printing "Schema exported successfully!" (#1245).
    echo "Step 4: Exporting to JSON...\n";
    $json = SchemaExporter::export();

    // Written to disk, not only echoed. `conformance/check_examples.sh` compiles every
    // `.json` an example leaves behind and reports "ran; emitted no schema" for one that
    // leaves none — so while this example only printed its export, the gate could not see
    // that what it printed did not compile.
    file_put_contents('schema.json', $json);

    $schemaArray = json_decode($json, true);
    echo "Schema exported successfully!\n";
    echo "Schema size: " . strlen($json) . " bytes\n";
    echo "Version: " . $schemaArray['version'] . "\n";
    echo "Type count: " . count($schemaArray['types'] ?? []) . "\n\n";

    // Show JSON structure
    echo "Step 5: JSON Schema structure:\n";
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

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
 * E-commerce schema example demonstrating complex nested types.
 *
 * This example shows how to:
 * - Define complex nested relationships
 * - Use TypeBuilder for programmatic schema construction
 * - Combine attribute-based and builder-based definitions
 * - Handle lists and nullable fields
 */

// Define domain types using attributes
#[GraphQLType(name: 'Category', description: 'Product category')]
final class Category
{
    #[GraphQLField(type: 'Int', description: 'Category ID')]
    public int $id;

    #[GraphQLField(type: 'String', description: 'Category name')]
    public string $name;

    #[GraphQLField(type: 'String', nullable: true)]
    public ?string $description;
}

#[GraphQLType(name: 'Product', description: 'Physical product')]
final class Product
{
    #[GraphQLField(type: 'Int', description: 'Product ID')]
    public int $id;

    #[GraphQLField(type: 'String', description: 'Product name')]
    public string $name;

    #[GraphQLField(type: 'String', description: 'Product description')]
    public string $description;

    #[GraphQLField(type: 'Float', description: 'Price in USD')]
    public float $price;

    #[GraphQLField(type: 'Int', description: 'Stock quantity')]
    public int $stock;

    #[GraphQLField(type: 'Category', description: 'Product category')]
    public Category $category;

    #[GraphQLField(type: 'Boolean', description: 'Whether product is active')]
    public bool $active;
}

#[GraphQLType(name: 'OrderItem', description: 'Item in an order')]
final class OrderItem
{
    #[GraphQLField(type: 'Int', description: 'Item ID')]
    public int $id;

    #[GraphQLField(type: 'Product', description: 'Product ordered')]
    public Product $product;

    #[GraphQLField(type: 'Int', description: 'Quantity ordered')]
    public int $quantity;

    #[GraphQLField(type: 'Float', description: 'Price paid for this item')]
    public float $pricePaid;
}

#[GraphQLType(name: 'Customer', description: 'Customer information')]
final class Customer
{
    #[GraphQLField(type: 'Int', description: 'Customer ID')]
    public int $id;

    #[GraphQLField(type: 'String', description: 'Customer name')]
    public string $name;

    #[GraphQLField(type: 'String', description: 'Email address')]
    public string $email;

    #[GraphQLField(type: 'String', nullable: true, description: 'Phone number')]
    public ?string $phone;
}

#[GraphQLType(name: 'Order', description: 'Customer order')]
final class Order
{
    #[GraphQLField(type: 'Int', description: 'Order ID')]
    public int $id;

    #[GraphQLField(type: 'Customer', description: 'Customer who placed order')]
    public Customer $customer;

    #[GraphQLField(type: 'String', description: 'Order status')]
    public string $status;

    #[GraphQLField(type: 'Float', description: 'Total order amount')]
    public float $total;

    #[GraphQLField(type: 'String', description: 'Order date')]
    public string $createdAt;
}

// Example usage with builder
function demonstrateEcommerceSchema(): void
{
    echo "=== FraiseQL PHP E-Commerce Schema Example ===\n\n";

    // Register attribute-based types
    echo "Step 1: Registering attribute-based types...\n";
    StaticAPI::register(Category::class);
    StaticAPI::register(Product::class);
    StaticAPI::register(OrderItem::class);
    StaticAPI::register(Customer::class);
    StaticAPI::register(Order::class);
    echo "✓ Registered 5 attribute-based types\n\n";

    // Declare queries.
    //
    // FraiseQL has no author-written `Query` root type: a query is declared with
    // `StaticAPI::query()` and the compiler assembles the root. This example used to
    // build object types *named* `Query` and `Mutation` with `TypeBuilder`, which is the
    // schema-first idiom of a different engine — it produced two ordinary types with
    // those names and no root fields at all (#1245).
    echo "Step 2: Declaring queries...\n";
    StaticAPI::query('products')
        ->returnType('Product')->returnsList()->nullable(false)
        ->sqlSource('v_product')->description('List all products')->register();
    StaticAPI::query('product')
        ->returnType('Product')->nullable(true)
        ->sqlSource('v_product')->description('Get product by ID')
        ->argument('id', 'Int', nullable: false)->register();
    StaticAPI::query('categories')
        ->returnType('Category')->returnsList()->nullable(false)
        ->sqlSource('v_category')->description('List all categories')->register();
    StaticAPI::query('orders')
        ->returnType('Order')->returnsList()->nullable(false)
        ->sqlSource('v_order')->description('List orders for customer')->register();

    echo "✓ Declared 4 queries\n\n";

    // Declare mutations. Each names the SQL function that performs the write, and the
    // views it invalidates — without `invalidatesViews` a cached read stays stale for the
    // whole of its TTL after the write that changed it.
    echo "Step 3: Declaring mutations...\n";
    StaticAPI::mutation('createOrder')
        ->returnType('Order')->sqlSource('fn_create_order')->operation('insert')
        ->description('Create a new order')
        ->argument('customerId', 'Int', nullable: false)
        ->invalidatesViews(['v_order'])->register();
    StaticAPI::mutation('updateProduct')
        ->returnType('Product')->sqlSource('fn_update_product')->operation('update')
        ->description('Update product')
        ->argument('id', 'Int', nullable: false)
        ->argument('name', 'String', nullable: true)
        ->invalidatesViews(['v_product'])->register();
    StaticAPI::mutation('cancelOrder')
        ->returnType('Order')->sqlSource('fn_cancel_order')->operation('update')
        ->description('Cancel an order')
        ->argument('id', 'Int', nullable: false)
        ->invalidatesViews(['v_order'])->register();

    echo "✓ Declared 3 mutations\n\n";

    // Export the schema.
    //
    // `SchemaExporter` is the export path — it is what `bin/fraiseql export` runs and
    // what `fraiseql compile` can read.
    echo "Step 4: Exporting complete schema...\n";
    $json = SchemaExporter::export();

    // Written to disk, not only echoed: `conformance/check_examples.sh` compiles every
    // `.json` an example leaves behind, and reports "ran; emitted no schema" for one that
    // leaves none — so while this example only printed its export, the gate could not see
    // that what it printed did not compile.
    file_put_contents('schema.json', $json);

    $schemaArray = json_decode($json, true);
    echo "Schema exported successfully!\n";
    echo "Total schema size: " . strlen($json) . " bytes\n";
    echo "Total types: " . count($schemaArray['types'] ?? []) . "\n";
    echo "Queries: " . count($schemaArray['queries'] ?? []) . "\n";
    echo "Mutations: " . count($schemaArray['mutations'] ?? []) . "\n\n";

    // Inspect specific types
    echo "Step 6: Analyzing schema structure...\n";
    echo "Product type fields:\n";
    $productFields = StaticAPI::getTypeFields('Product');
    foreach ($productFields as $field) {
        $type = $field->getGraphQLTypeString();
        $description = $field->description ?? '(no description)';
        echo "  - {$field->name}: {$type}\n    {$description}\n";
    }
    echo "\n";

    // Show order of types
    echo "Step 7: Registered type names:\n";
    $typeNames = StaticAPI::getTypeNames();
    echo "Types: " . implode(', ', $typeNames) . "\n";
    echo "Total: " . count($typeNames) . " types\n\n";

    // Display sample JSON
    echo "Step 8: Sample JSON export (first 2 types):\n";
    echo json_encode(
        [
            'version' => $schemaArray['version'],
            'types' => array_slice($schemaArray['types'], 0, 2),
        ],
        JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES
    ) . "\n";
}

// Only run when executed directly
if (basename(__FILE__) === basename($_SERVER['SCRIPT_NAME'] ?? '')) {
    try {
        demonstrateEcommerceSchema();
    } catch (\Exception $e) {
        echo "Error: " . $e->getMessage() . "\n";
        exit(1);
    }
}

<?php

declare(strict_types=1);

namespace FraiseQL\Examples;

use FraiseQL\Attributes\GraphQLType;
use FraiseQL\Attributes\GraphQLField;
use FraiseQL\StaticAPI;
use FraiseQL\SchemaFormatter;

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

    // Create Query and Mutation types using TypeBuilder
    echo "Step 2: Building Query type with fluent API...\n";
    $queryBuilder = \FraiseQL\TypeBuilder::type('Query')
        ->description('Root query type for e-commerce API')
        ->field('products', 'Product', isList: true, description: 'List all products')
        ->field('product', 'Product', nullable: true, description: 'Get product by ID')
        ->field('categories', 'Category', isList: true, description: 'List all categories')
        ->field('orders', 'Order', isList: true, description: 'List orders for customer')
        ->field('order', 'Order', nullable: true, description: 'Get order details');

    echo "✓ Query type has " . $queryBuilder->getFieldCount() . " fields\n\n";

    // Create Mutation type
    echo "Step 3: Building Mutation type...\n";
    $mutationBuilder = \FraiseQL\TypeBuilder::type('Mutation')
        ->description('Root mutation type for e-commerce API')
        ->field('createOrder', 'Order', description: 'Create a new order')
        ->field('updateProduct', 'Product', nullable: true, description: 'Update product')
        ->field('cancelOrder', 'Boolean', description: 'Cancel an order')
        ->field('addToCart', 'Boolean', description: 'Add item to shopping cart');

    echo "✓ Mutation type has " . $mutationBuilder->getFieldCount() . " fields\n\n";

    // Format and export schema
    echo "Step 4: Formatting complete schema...\n";
    $registry = \FraiseQL\SchemaRegistry::getInstance();
    $formatter = new SchemaFormatter();

    $schema = $formatter->formatBuilders(
        $queryBuilder,
        $mutationBuilder
    );

    echo "✓ Schema created with " . $schema->getTypeCount() . " root types\n\n";

    // Combine attribute types with builders
    echo "Step 5: Exporting complete schema...\n";
    $attributeSchema = $formatter->formatRegistry(
        $registry,
        description: 'Complete e-commerce GraphQL schema'
    );

    $json = $attributeSchema->toJson();
    echo "Schema exported successfully!\n";
    echo "Total schema size: " . strlen($json) . " bytes\n";
    echo "Total types: " . $attributeSchema->getTypeCount() . "\n";
    echo "Scalars: " . implode(', ', $attributeSchema->getScalarNames()) . "\n\n";

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
    $schemaArray = $attributeSchema->toArray();
    $sampleTypes = array_slice($schemaArray['types'], 0, 2, true);
    echo json_encode(
        [
            'version' => $schemaArray['version'],
            'description' => $schemaArray['description'] ?? null,
            'types' => $sampleTypes,
            'scalars' => $schemaArray['scalars'],
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

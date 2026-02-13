package com.fraiseql.examples;

import com.fraiseql.core.*;

/**
 * EcommerceSchema - Advanced example with product catalog, shopping cart, and orders
 *
 * This example demonstrates:
 * 1. Complex types with multiple fields
 * 2. Type relationships (Product, Cart, Order, etc.)
 * 3. Advanced queries with filtering arguments
 * 4. Mutations for shopping operations
 * 5. Real-world GraphQL schema design
 *
 * Output: ecommerce-schema.json - Ready for fraiseql-cli compile
 *
 * Usage:
 *   mvn exec:java -Dexec.mainClass="com.fraiseql.examples.EcommerceSchema"
 */
public class EcommerceSchema {

    public static void main(String[] args) {
        try {
            System.out.println("FraiseQL Java - Ecommerce Schema Example");
            System.out.println("========================================\n");

            // Register types
            System.out.println("1. Registering ecommerce types...");
            FraiseQL.registerTypes(
                Product.class,
                Category.class,
                Customer.class,
                CartItem.class,
                Order.class,
                OrderItem.class,
                Review.class
            );
            System.out.println("   ✓ 7 types registered\n");

            // Register queries
            System.out.println("2. Registering queries...");

            FraiseQL.query("products")
                .returnType(Product.class)
                .returnsArray(true)
                .arg("categoryId", "Int")
                .arg("limit", "Int")
                .arg("offset", "Int")
                .description("Get products with optional category and pagination")
                .register();
            System.out.println("   ✓ products");

            FraiseQL.query("product")
                .returnType(Product.class)
                .arg("id", "Int")
                .description("Get a specific product")
                .register();
            System.out.println("   ✓ product");

            FraiseQL.query("categories")
                .returnType(Category.class)
                .returnsArray(true)
                .description("Get all product categories")
                .register();
            System.out.println("   ✓ categories");

            FraiseQL.query("customer")
                .returnType(Customer.class)
                .arg("id", "Int")
                .description("Get customer profile")
                .register();
            System.out.println("   ✓ customer");

            FraiseQL.query("orders")
                .returnType(Order.class)
                .returnsArray(true)
                .arg("customerId", "Int")
                .arg("limit", "Int")
                .description("Get customer's orders")
                .register();
            System.out.println("   ✓ orders");

            FraiseQL.query("reviews")
                .returnType(Review.class)
                .returnsArray(true)
                .arg("productId", "Int")
                .description("Get reviews for a product")
                .register();
            System.out.println("   ✓ reviews\n");

            // Register mutations
            System.out.println("3. Registering mutations...");

            FraiseQL.mutation("createCustomer")
                .returnType(Customer.class)
                .arg("name", "String")
                .arg("email", "String")
                .arg("phone", "String")
                .description("Create a new customer account")
                .register();
            System.out.println("   ✓ createCustomer");

            FraiseQL.mutation("updateCustomer")
                .returnType(Customer.class)
                .arg("id", "Int")
                .arg("name", "String")
                .arg("email", "String")
                .description("Update customer profile")
                .register();
            System.out.println("   ✓ updateCustomer");

            FraiseQL.mutation("addToCart")
                .returnType(CartItem.class)
                .arg("customerId", "Int")
                .arg("productId", "Int")
                .arg("quantity", "Int")
                .description("Add item to shopping cart")
                .register();
            System.out.println("   ✓ addToCart");

            FraiseQL.mutation("removeFromCart")
                .returnType(CartItem.class)
                .arg("customerId", "Int")
                .arg("productId", "Int")
                .description("Remove item from shopping cart")
                .register();
            System.out.println("   ✓ removeFromCart");

            FraiseQL.mutation("checkout")
                .returnType(Order.class)
                .arg("customerId", "Int")
                .arg("shippingAddress", "String")
                .description("Checkout and create order")
                .register();
            System.out.println("   ✓ checkout");

            FraiseQL.mutation("createReview")
                .returnType(Review.class)
                .arg("productId", "Int")
                .arg("customerId", "Int")
                .arg("rating", "Int")
                .arg("text", "String")
                .description("Create a product review")
                .register();
            System.out.println("   ✓ createReview\n");

            // Export schema
            System.out.println("4. Exporting schema to ecommerce-schema.json...");
            FraiseQL.exportSchema("ecommerce-schema.json");
            System.out.println("   ✓ Schema exported\n");

            // Print summary
            SchemaRegistry registry = FraiseQL.getRegistry();
            System.out.println("Ecommerce Schema Summary:");
            System.out.println("------------------------");
            System.out.println("Types:     " + registry.getAllTypes().size());
            System.out.println("Queries:   " + registry.getAllQueries().size());
            System.out.println("Mutations: " + registry.getAllMutations().size());
            System.out.println("\nTypes:");
            for (String typeName : registry.getAllTypes().keySet()) {
                System.out.println("  - " + typeName);
            }

        } catch (Exception e) {
            System.err.println("Error: " + e.getMessage());
            e.printStackTrace();
            System.exit(1);
        }
    }

    @GraphQLType(description = "A product in the catalog")
    public static class Product {
        @GraphQLField(description = "Product ID")
        public int id;

        @GraphQLField(description = "Product name")
        public String name;

        @GraphQLField(description = "Product description")
        public String description;

        @GraphQLField(description = "Price in cents")
        public int price;

        @GraphQLField(description = "Category ID")
        public int categoryId;

        @GraphQLField(description = "Stock quantity")
        public int stock;

        @GraphQLField(name = "created_at", description = "When product was added")
        public String createdAt;
    }

    @GraphQLType(description = "A product category")
    public static class Category {
        @GraphQLField(description = "Category ID")
        public int id;

        @GraphQLField(description = "Category name")
        public String name;

        @GraphQLField(description = "Number of products in category")
        public int productCount;
    }

    @GraphQLType(description = "A customer account")
    public static class Customer {
        @GraphQLField(description = "Customer ID")
        public int id;

        @GraphQLField(description = "Customer name")
        public String name;

        @GraphQLField(description = "Customer email")
        public String email;

        @GraphQLField(description = "Customer phone")
        public String phone;

        @GraphQLField(description = "Total spent in cents")
        public int totalSpent;

        @GraphQLField(name = "created_at", description = "Account creation date")
        public String createdAt;
    }

    @GraphQLType(description = "Item in a shopping cart")
    public static class CartItem {
        @GraphQLField(description = "Cart item ID")
        public int id;

        @GraphQLField(description = "Customer ID")
        public int customerId;

        @GraphQLField(description = "Product ID")
        public int productId;

        @GraphQLField(description = "Quantity in cart")
        public int quantity;

        @GraphQLField(description = "Added to cart at")
        public String addedAt;
    }

    @GraphQLType(description = "A customer order")
    public static class Order {
        @GraphQLField(description = "Order ID")
        public int id;

        @GraphQLField(description = "Customer ID")
        public int customerId;

        @GraphQLField(description = "Total price in cents")
        public int total;

        @GraphQLField(description = "Order status (pending, processing, shipped, delivered)")
        public String status;

        @GraphQLField(description = "Shipping address")
        public String shippingAddress;

        @GraphQLField(name = "created_at", description = "Order date")
        public String createdAt;

        @GraphQLField(name = "shipped_at", description = "Ship date")
        public String shippedAt;
    }

    @GraphQLType(description = "Item in an order")
    public static class OrderItem {
        @GraphQLField(description = "Order item ID")
        public int id;

        @GraphQLField(description = "Order ID")
        public int orderId;

        @GraphQLField(description = "Product ID")
        public int productId;

        @GraphQLField(description = "Quantity ordered")
        public int quantity;

        @GraphQLField(description = "Price at time of order")
        public int price;
    }

    @GraphQLType(description = "Product review")
    public static class Review {
        @GraphQLField(description = "Review ID")
        public int id;

        @GraphQLField(description = "Product ID")
        public int productId;

        @GraphQLField(description = "Customer ID")
        public int customerId;

        @GraphQLField(description = "Star rating (1-5)")
        public int rating;

        @GraphQLField(description = "Review text")
        public String text;

        @GraphQLField(name = "created_at", description = "Review date")
        public String createdAt;

        @GraphQLField(description = "Number of helpful votes")
        public int helpfulVotes;
    }
}

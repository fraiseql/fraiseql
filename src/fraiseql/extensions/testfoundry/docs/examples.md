# TestFoundry Examples

This document provides complete, working examples of TestFoundry configurations for various scenarios.

## Table of Contents

1. [Simple Blog Application](#simple-blog-application)
2. [E-Commerce System](#e-commerce-system)
3. [Multi-Tenant SaaS](#multi-tenant-saas)
4. [Geographic Data with Group Leaders](#geographic-data-with-group-leaders)
5. [Time-Series Data](#time-series-data)
6. [Complex Business Rules](#complex-business-rules)

## Simple Blog Application

A basic blog with users, posts, and comments.

### Schema

```sql
-- Users table
CREATE TABLE tb_users (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    email TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL,
    bio TEXT,
    avatar_url TEXT,
    created_at TIMESTAMP DEFAULT NOW(),
    deleted_at TIMESTAMP
);

-- Posts table
CREATE TABLE tb_posts (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    author_id UUID REFERENCES tb_users(id),
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    excerpt TEXT,
    tags TEXT[],
    is_published BOOLEAN DEFAULT FALSE,
    published_at TIMESTAMP,
    created_at TIMESTAMP DEFAULT NOW(),
    deleted_at TIMESTAMP
);

-- Comments table
CREATE TABLE tb_comments (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    post_id UUID REFERENCES tb_posts(id),
    author_id UUID REFERENCES tb_users(id),
    content TEXT NOT NULL,
    is_approved BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMP DEFAULT NOW(),
    deleted_at TIMESTAMP
);
```

### Input Types

```sql
-- User input
CREATE TYPE type_user_input AS (
    email TEXT,
    name TEXT,
    bio TEXT,
    avatar_url TEXT
);

-- Post input
CREATE TYPE type_post_input AS (
    author_id UUID,
    title TEXT,
    content TEXT,
    excerpt TEXT,
    tags TEXT[],
    is_published BOOLEAN
);

-- Comment input
CREATE TYPE type_comment_input AS (
    post_id UUID,
    author_id UUID,
    content TEXT,
    is_approved BOOLEAN
);
```

### TestFoundry Metadata

```sql
-- User field mappings
INSERT INTO testfoundry.testfoundry_tb_input_field_mapping
(input_type, field_name, generator_type, random_function, required)
VALUES
('user_input', 'email', 'random', 'testfoundry_random_email', TRUE),
('user_input', 'name', 'random', 'testfoundry_random_person_name', TRUE),
('user_input', 'bio', 'random', 'testfoundry_random_text', FALSE),
('user_input', 'avatar_url', 'random', 'testfoundry_random_avatar_url', FALSE);

-- Post field mappings
INSERT INTO testfoundry.testfoundry_tb_input_field_mapping
(input_type, field_name, generator_type, fk_mapping_key, random_function, required)
VALUES
('post_input', 'author_id', 'resolve_fk', 'user_id', NULL, TRUE),
('post_input', 'title', 'random', NULL, 'testfoundry_random_blog_title', TRUE),
('post_input', 'content', 'random', NULL, 'testfoundry_random_blog_content', TRUE),
('post_input', 'excerpt', 'random', NULL, 'testfoundry_random_text', FALSE),
('post_input', 'tags', 'random', NULL, 'testfoundry_random_tags', FALSE),
('post_input', 'is_published', 'random', NULL, 'testfoundry_random_boolean', FALSE);

-- Comment field mappings
INSERT INTO testfoundry.testfoundry_tb_input_field_mapping
(input_type, field_name, generator_type, fk_mapping_key, required)
VALUES
('comment_input', 'post_id', 'resolve_fk', 'post_id', TRUE),
('comment_input', 'author_id', 'resolve_fk', 'user_id', TRUE),
('comment_input', 'content', 'random', NULL, TRUE),
('comment_input', 'is_approved', 'random', NULL, FALSE);

-- FK mappings
INSERT INTO testfoundry.testfoundry_tb_fk_mapping
(input_type, from_expression, select_field, random_pk_field, random_value_field, random_select_where)
VALUES
('user_id', 'tb_users', 'id', 'id', 'email', 'deleted_at IS NULL'),
('post_id', 'tb_posts', 'id', 'id', 'title', 'deleted_at IS NULL AND is_published = true');

-- Entity dependencies
INSERT INTO testfoundry.testfoundry_tb_entity_dependents
(entity_name, dependent_entity_name, dependency_type, parent_pk_fields, child_fk_fields)
VALUES
('users', 'posts', 'RESTRICT', ARRAY['id'], ARRAY['author_id']),
('users', 'comments', 'RESTRICT', ARRAY['id'], ARRAY['author_id']),
('posts', 'comments', 'CASCADE', ARRAY['id'], ARRAY['post_id']);
```

### Custom Random Functions

```sql
-- Random person name
CREATE OR REPLACE FUNCTION testfoundry_random_person_name()
RETURNS TEXT AS $$
DECLARE
    first_names TEXT[] := ARRAY['John', 'Jane', 'Bob', 'Alice', 'Charlie', 'Emma', 'Oliver', 'Sophia'];
    last_names TEXT[] := ARRAY['Smith', 'Johnson', 'Williams', 'Brown', 'Jones', 'Garcia', 'Miller', 'Davis'];
BEGIN
    RETURN first_names[1 + FLOOR(RANDOM() * array_length(first_names, 1))] || ' ' ||
           last_names[1 + FLOOR(RANDOM() * array_length(last_names, 1))];
END;
$$ LANGUAGE plpgsql;

-- Random blog title
CREATE OR REPLACE FUNCTION testfoundry_random_blog_title()
RETURNS TEXT AS $$
DECLARE
    templates TEXT[] := ARRAY[
        'How to %s in %s Easy Steps',
        'The Ultimate Guide to %s',
        '%s: What You Need to Know',
        'Why %s Matters in 2024',
        'Getting Started with %s'
    ];
    topics TEXT[] := ARRAY['PostgreSQL', 'GraphQL', 'Testing', 'DevOps', 'Security'];
BEGIN
    RETURN REPLACE(
        templates[1 + FLOOR(RANDOM() * array_length(templates, 1))],
        '%s',
        topics[1 + FLOOR(RANDOM() * array_length(topics, 1))]
    );
END;
$$ LANGUAGE plpgsql;

-- Random tags
CREATE OR REPLACE FUNCTION testfoundry_random_tags()
RETURNS TEXT[] AS $$
DECLARE
    all_tags TEXT[] := ARRAY['postgresql', 'graphql', 'testing', 'tutorial', 'devops', 'security', 'performance'];
    num_tags INT := 1 + FLOOR(RANDOM() * 4);
    selected_tags TEXT[] := '{}';
    i INT;
BEGIN
    FOR i IN 1..num_tags LOOP
        selected_tags := selected_tags || all_tags[1 + FLOOR(RANDOM() * array_length(all_tags, 1))];
    END LOOP;
    RETURN selected_tags;
END;
$$ LANGUAGE plpgsql;
```

### Generate Tests

```sql
-- Generate happy path tests
SELECT testfoundry_generate_happy_create('users');
SELECT testfoundry_generate_happy_create('posts');
SELECT testfoundry_generate_happy_create('comments');

-- Generate constraint tests
SELECT testfoundry_generate_duplicate_create('users');  -- Email uniqueness
SELECT testfoundry_generate_fk_violation_create('posts');  -- Invalid author
SELECT testfoundry_generate_fk_violation_create('comments');  -- Invalid post/author
```

## E-Commerce System

A complete e-commerce system with products, orders, and inventory.

### Schema

```sql
-- Categories
CREATE TABLE tb_categories (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    name TEXT NOT NULL,
    slug TEXT UNIQUE NOT NULL,
    parent_id UUID REFERENCES tb_categories(id),
    deleted_at TIMESTAMP
);

-- Products
CREATE TABLE tb_products (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    sku TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    category_id UUID REFERENCES tb_categories(id),
    price DECIMAL(10,2) NOT NULL,
    cost DECIMAL(10,2),
    weight DECIMAL(8,3),
    deleted_at TIMESTAMP
);

-- Inventory
CREATE TABLE tb_inventory (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    product_id UUID REFERENCES tb_products(id),
    warehouse_id UUID NOT NULL,
    quantity INTEGER NOT NULL DEFAULT 0,
    reserved_quantity INTEGER NOT NULL DEFAULT 0,
    reorder_point INTEGER,
    reorder_quantity INTEGER,
    UNIQUE(product_id, warehouse_id)
);

-- Customers
CREATE TABLE tb_customers (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    email TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL,
    phone TEXT,
    deleted_at TIMESTAMP
);

-- Orders
CREATE TABLE tb_orders (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    order_number TEXT UNIQUE NOT NULL,
    customer_id UUID REFERENCES tb_customers(id),
    status TEXT NOT NULL DEFAULT 'pending',
    total DECIMAL(10,2) NOT NULL,
    created_at TIMESTAMP DEFAULT NOW()
);
```

### Complex Group Leader Example

```sql
-- Product variant with pricing tiers (group leader pattern)
INSERT INTO testfoundry.testfoundry_tb_input_field_mapping
(input_type, field_name, generator_type, fk_mapping_key, group_leader, generator_group, group_dependency_fields)
VALUES
-- Category is the group leader - it determines product and pricing
('product_variant_input', 'category', 'resolve_fk', 'category_with_product', TRUE, 'product_group',
 ARRAY['category', 'product_id', 'base_price', 'tier_prices']),
('product_variant_input', 'product_id', 'resolve_fk', NULL, FALSE, 'product_group', NULL),
('product_variant_input', 'base_price', 'resolve_fk', NULL, FALSE, 'product_group', NULL),
('product_variant_input', 'tier_prices', 'resolve_fk', NULL, FALSE, 'product_group', NULL);

-- Complex FK mapping that returns multiple related values
INSERT INTO testfoundry.testfoundry_tb_fk_mapping
(input_type, from_expression, random_select_expression)
VALUES
(
    'category_with_product',
    '
    tb_categories c
    JOIN tb_products p ON p.category_id = c.id
    LEFT JOIN LATERAL (
        SELECT jsonb_agg(
            jsonb_build_object(
                ''min_quantity'', pt.min_quantity,
                ''price'', pt.price
            ) ORDER BY pt.min_quantity
        ) as tier_prices
        FROM tb_price_tiers pt
        WHERE pt.product_id = p.id
    ) tiers ON true
    ',
    'jsonb_build_object(
        ''category'', c.name,
        ''product_id'', p.id,
        ''base_price'', p.price,
        ''tier_prices'', COALESCE(tiers.tier_prices, ''[]''::jsonb)
    )'
);
```

## Multi-Tenant SaaS

A multi-tenant system with organizations and role-based access.

### Schema with RLS

```sql
-- Organizations
CREATE TABLE tb_organizations (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    name TEXT NOT NULL,
    slug TEXT UNIQUE NOT NULL,
    plan TEXT NOT NULL DEFAULT 'free',
    deleted_at TIMESTAMP
);

-- Users with organization
CREATE TABLE tb_users (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    email TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL,
    deleted_at TIMESTAMP
);

-- Organization membership
CREATE TABLE tb_organization_members (
    organization_id UUID REFERENCES tb_organizations(id),
    user_id UUID REFERENCES tb_users(id),
    role TEXT NOT NULL DEFAULT 'member',
    joined_at TIMESTAMP DEFAULT NOW(),
    PRIMARY KEY (organization_id, user_id)
);

-- Enable RLS
ALTER TABLE tb_organizations ENABLE ROW LEVEL SECURITY;
ALTER TABLE tb_organization_members ENABLE ROW LEVEL SECURITY;
```

### Authorization Testing

```sql
-- Manual scenarios for authorization testing
INSERT INTO testfoundry.test_manual_scenarios
(entity_name, scenario_type, input_json, expected_result, description)
VALUES
-- Admin can create any org
('organizations', 'auth_admin',
 '{"name": "Test Org", "plan": "enterprise", "_auth": {"role": "admin"}}',
 'success',
 'Admin can create enterprise org'),

-- Regular user cannot create enterprise
('organizations', 'auth_user',
 '{"name": "Test Org", "plan": "enterprise", "_auth": {"role": "user"}}',
 'error',
 'Regular user cannot create enterprise org'),

-- Member can view their org
('organization_members', 'auth_member',
 '{"action": "select", "_auth": {"user_id": "{{user_id}}", "org_id": "{{org_id}}"}}',
 'success',
 'Member can view their organization');
```

## Geographic Data with Group Leaders

Complex geographic relationships maintaining consistency.

### Schema

```sql
-- Countries
CREATE TABLE tb_countries (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    code TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL
);

-- States/Provinces
CREATE TABLE tb_states (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    country_id UUID REFERENCES tb_countries(id),
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    UNIQUE(country_id, code)
);

-- Cities
CREATE TABLE tb_cities (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    state_id UUID REFERENCES tb_states(id),
    name TEXT NOT NULL,
    latitude DECIMAL(10,8),
    longitude DECIMAL(11,8)
);

-- Postal codes
CREATE TABLE tb_postal_codes (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    city_id UUID REFERENCES tb_cities(id),
    code TEXT NOT NULL,
    UNIQUE(city_id, code)
);
```

### Complete Address Group

```sql
-- Address input with full geographic consistency
INSERT INTO testfoundry.testfoundry_tb_input_field_mapping
(input_type, field_name, generator_type, fk_mapping_key, group_leader, generator_group, group_dependency_fields)
VALUES
-- Country is the group leader - generates entire address
('address_input', 'country', 'resolve_fk', 'complete_address', TRUE, 'geo_group',
 ARRAY['country', 'state', 'city', 'postal_code', 'latitude', 'longitude']),
('address_input', 'state', 'resolve_fk', NULL, FALSE, 'geo_group', NULL),
('address_input', 'city', 'resolve_fk', NULL, FALSE, 'geo_group', NULL),
('address_input', 'postal_code', 'resolve_fk', NULL, FALSE, 'geo_group', NULL),
('address_input', 'latitude', 'resolve_fk', NULL, FALSE, 'geo_group', NULL),
('address_input', 'longitude', 'resolve_fk', NULL, FALSE, 'geo_group', NULL),
-- Street details are random
('address_input', 'street_number', 'random', NULL, FALSE, NULL, NULL),
('address_input', 'street_name', 'random', NULL, FALSE, NULL, NULL);

-- Complex geographic FK mapping
INSERT INTO testfoundry.testfoundry_tb_fk_mapping
(input_type, from_expression, random_select_expression)
VALUES
(
    'complete_address',
    '
    tb_countries co
    JOIN tb_states s ON s.country_id = co.id
    JOIN tb_cities ci ON ci.state_id = s.id
    JOIN tb_postal_codes pc ON pc.city_id = ci.id
    ',
    'jsonb_build_object(
        ''country'', co.name,
        ''state'', s.name,
        ''city'', ci.name,
        ''postal_code'', pc.code,
        ''latitude'', ci.latitude,
        ''longitude'', ci.longitude
    )'
);
```

## Time-Series Data

Testing time-series data with proper temporal relationships.

### Schema

```sql
-- Sensors
CREATE TABLE tb_sensors (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    name TEXT NOT NULL,
    type TEXT NOT NULL,
    location JSONB,
    installed_at TIMESTAMP NOT NULL,
    decommissioned_at TIMESTAMP
);

-- Readings
CREATE TABLE tb_readings (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    sensor_id UUID REFERENCES tb_sensors(id),
    timestamp TIMESTAMP NOT NULL,
    value DECIMAL(20,6) NOT NULL,
    unit TEXT NOT NULL,
    quality_flag TEXT,
    UNIQUE(sensor_id, timestamp)
);
```

### Time-Aware Test Data

```sql
-- Custom function for time-series data
CREATE OR REPLACE FUNCTION testfoundry_random_reading_timestamp(
    p_sensor_data JSONB
)
RETURNS TIMESTAMP AS $$
DECLARE
    installed_at TIMESTAMP;
    decommissioned_at TIMESTAMP;
    min_time TIMESTAMP;
    max_time TIMESTAMP;
BEGIN
    -- Extract sensor lifecycle from passed data
    installed_at := (p_sensor_data->>'installed_at')::TIMESTAMP;
    decommissioned_at := (p_sensor_data->>'decommissioned_at')::TIMESTAMP;

    -- Determine valid time range
    min_time := GREATEST(installed_at, NOW() - INTERVAL '30 days');
    max_time := LEAST(
        COALESCE(decommissioned_at, NOW()),
        NOW()
    );

    -- Generate random timestamp within valid range
    RETURN min_time + (RANDOM() * (max_time - min_time));
END;
$$ LANGUAGE plpgsql;

-- Reading input with sensor-aware timestamp
INSERT INTO testfoundry.testfoundry_tb_input_field_mapping
(input_type, field_name, generator_type, fk_mapping_key, fk_dependency_fields, random_function)
VALUES
('reading_input', 'sensor_id', 'resolve_fk', 'active_sensor', NULL, NULL),
('reading_input', 'timestamp', 'random', NULL, ARRAY['sensor_id'], 'testfoundry_random_reading_timestamp'),
('reading_input', 'value', 'random', NULL, NULL, 'testfoundry_random_sensor_value'),
('reading_input', 'unit', 'random', NULL, NULL, 'testfoundry_random_unit');
```

## Complex Business Rules

Testing complex business logic with interdependent validations.

### Order Processing Rules

```sql
-- Order validation scenarios
INSERT INTO testfoundry.test_manual_scenarios
(entity_name, scenario_type, input_json, expected_result, description)
VALUES
-- Minimum order amount
('orders', 'min_amount',
 '{"customer_id": "{{customer_id}}", "items": [{"product_id": "{{product_id}}", "quantity": 1, "price": 5.00}], "total": 5.00}',
 'error',
 'Order below minimum amount should fail'),

-- Inventory check
('orders', 'insufficient_inventory',
 '{"customer_id": "{{customer_id}}", "items": [{"product_id": "{{low_stock_product}}", "quantity": 1000}]}',
 'error',
 'Order exceeding inventory should fail'),

-- Discount validation
('orders', 'invalid_discount',
 '{"customer_id": "{{customer_id}}", "items": [{"product_id": "{{product_id}}", "quantity": 1}], "discount_code": "EXPIRED2023"}',
 'error',
 'Expired discount code should fail'),

-- VIP customer benefits
('orders', 'vip_benefits',
 '{"customer_id": "{{vip_customer_id}}", "items": [{"product_id": "{{product_id}}", "quantity": 1}], "expedited_shipping": true}',
 'success',
 'VIP customer gets free expedited shipping');
```

### State Machine Testing

```sql
-- Order state transitions
CREATE OR REPLACE FUNCTION testfoundry_generate_order_state_tests(
    p_entity_name TEXT
)
RETURNS TEXT AS $$
BEGIN
    RETURN format($test$
-- Test order state machine transitions
CREATE OR REPLACE FUNCTION test_%I_state_transitions()
RETURNS SETOF TEXT AS $func$
DECLARE
    v_order_id UUID;
    v_result RECORD;
BEGIN
    -- Create order in pending state
    v_order_id := create_order(
        testfoundry_generate_random_input('order_input')::jsonb
    );

    -- Test valid transitions
    PERFORM update_order_status(v_order_id, 'confirmed');
    RETURN NEXT pass('Pending -> Confirmed transition successful');

    PERFORM update_order_status(v_order_id, 'processing');
    RETURN NEXT pass('Confirmed -> Processing transition successful');

    -- Test invalid transition
    BEGIN
        PERFORM update_order_status(v_order_id, 'pending');
        RETURN NEXT fail('Should not allow Processing -> Pending');
    EXCEPTION WHEN OTHERS THEN
        RETURN NEXT pass('Invalid transition correctly blocked');
    END;

    RETURN;
END;
$func$ LANGUAGE plpgsql;
$test$, p_entity_name);
END;
$$ LANGUAGE plpgsql;
```

## Running the Examples

### 1. Install TestFoundry

```python
from fraiseql.extensions.testfoundry import FoundrySetup

setup = FoundrySetup(repository)
await setup.install()
```

### 2. Load Example Metadata

```bash
# Choose an example and run its SQL
psql -d your_db -f examples/blog_metadata.sql
```

### 3. Generate Tests

```python
from fraiseql.extensions.testfoundry import FoundryGenerator

generator = FoundryGenerator(repository)

# Generate for all entities
entities = [
    {"entity_name": "users", "table_name": "tb_users", "input_type_name": "user_input"},
    {"entity_name": "posts", "table_name": "tb_posts", "input_type_name": "post_input"},
    {"entity_name": "comments", "table_name": "tb_comments", "input_type_name": "comment_input"}
]

await generator.generate_all_tests(entities)
```

### 4. Run Tests

```bash
# With pgTAP
pg_prove -d your_db tests/generated/*.sql

# With pytest
pytest tests/generated/
```

## Tips for Creating Your Own

1. **Start with Schema**: Define your tables with proper constraints
2. **Create Input Types**: Match your GraphQL/API input structures
3. **Map Simple Fields First**: Get random generation working
4. **Add FK Relationships**: One at a time, test each
5. **Implement Group Leaders**: For complex related data
6. **Add Custom Functions**: For domain-specific data
7. **Define Manual Scenarios**: For edge cases and business rules
8. **Test Incrementally**: Verify each addition works

## Debugging Generated Tests

Enable debug mode to see how data is generated:

```sql
-- See step-by-step generation
SELECT testfoundry_generate_random_input('user_input', true);

-- Check specific FK resolution
SELECT testfoundry_random_value_from_mapping('user_id', NULL);

-- Verify group leader configuration
SELECT * FROM testfoundry.testfoundry_tb_input_field_mapping
WHERE generator_group IS NOT NULL
ORDER BY generator_group, group_leader DESC;
```

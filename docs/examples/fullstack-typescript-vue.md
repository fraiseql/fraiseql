# Full-Stack E-Commerce with TypeScript Schema, FraiseQL Backend, and Vue 3 Frontend

**Duration**: 2-3 hours (complete walkthrough)
**Outcome**: Fully functional e-commerce application with product catalog, shopping cart, and order management
**Prerequisites**: Node.js 18+, Rust 1.70+, PostgreSQL 14+, basic knowledge of TypeScript, GraphQL, and Vue 3

This guide demonstrates the complete FraiseQL workflow: schema authoring in TypeScript → compilation to Rust → deployment as a production GraphQL server → frontend consumption with Vue 3.

**Key Insight**: TypeScript defines *what* your API looks like. FraiseQL's Rust compiler *how* it executes efficiently. Vue *consumes* the generated API. Each layer is independent.

---

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Part 1: TypeScript Schema Authoring](#part-1-typescript-schema-authoring)
3. [Part 2: Database Schema](#part-2-database-schema)
4. [Part 3: Export and Compilation](#part-3-export-and-compilation)
5. [Part 4: FraiseQL Server Deployment](#part-4-fraiseql-server-deployment)
6. [Part 5: Vue 3 Frontend](#part-5-vue-3-frontend)
7. [Part 6: Project Structure](#part-6-project-structure)
8. [Part 7: Running the Complete Stack](#part-7-running-the-complete-stack)
9. [Part 8: Example Workflows](#part-8-example-workflows)
10. [Part 9: Production Deployment](#part-9-production-deployment)
11. [Part 10: Troubleshooting](#part-10-troubleshooting)

---

## Architecture Overview

```text
┌─────────────────────────────────────────────────────────────────────┐
│                          YOUR APPLICATION                           │
└─────────────────────────────────────────────────────────────────────┘

┌──────────────────────┐         ┌──────────────────────────────────────┐
│   TypeScript Layer   │         │      Development Time                │
│                      │         │  (Your machine, one-time setup)      │
│  • schema.ts         │         │                                      │
│  • types/User.ts     │         │  • Define your API shape             │
│  • types/Product.ts  │         │  • Use decorators for validation     │
│  • types/Order.ts    │         │  • TypeScript handles IDE support    │
└──────────────────────┘         └──────────────────────────────────────┘
         │
         │ npm run export
         │ (generates schema.json)
         ↓
┌──────────────────────┐         ┌──────────────────────────────────────┐
│  Compilation Layer   │         │      Build Time                      │
│                      │         │  (CI/CD pipeline or local build)     │
│  • schema.json       │         │                                      │
│  • fraiseql.toml     │         │  • Validate schema structure         │
│  • SQL generation    │         │  • Generate optimized SQL templates  │
│  • Type checking     │         │  • Create compiled schema            │
└──────────────────────┘         └──────────────────────────────────────┘
         │
         │ fraiseql-cli compile
         │ (generates schema.compiled.json)
         ↓
┌──────────────────────┐         ┌──────────────────────────────────────┐
│   Runtime Layer      │         │      Production                      │
│                      │         │  (Rust server in Docker/Cloud)       │
│  • fraiseql-server   │         │                                      │
│  • PostgreSQL        │         │  • Load compiled schema              │
│  • GraphQL Endpoint  │         │  • Execute queries                   │
│  • HTTP API :8080    │         │  • Return JSON results               │
└──────────────────────┘         └──────────────────────────────────────┘
         │
         │ GraphQL Queries
         │ (HTTP POST to /graphql)
         ↓
┌──────────────────────┐         ┌──────────────────────────────────────┐
│   Vue 3 Frontend     │         │      Client Browser                  │
│                      │         │                                      │
│  • Apollo Client     │         │  • ProductGrid component             │
│  • useQuery hooks    │         │  • Shopping cart state               │
│  • useMutation hooks │         │  • Real-time updates                 │
│  • Reactive state    │         │  • User-friendly UI                  │
└──────────────────────┘         └──────────────────────────────────────┘
```text

**Flow Summary**:

1. **Write** TypeScript schema with decorators (author layer)
2. **Export** to JSON schema (npm script)
3. **Compile** with FraiseQL CLI (Rust compilation, generates optimized SQL)
4. **Deploy** compiled schema to FraiseQL server (Rust binary)
5. **Consume** GraphQL queries from Vue 3 frontend (Apollo Client)

---

## Part 1: TypeScript Schema Authoring

TypeScript is the **authoring language only**. You define your GraphQL schema using decorators, which generates `schema.json`. The Rust compiler never runs TypeScript code—it only consumes the JSON output.

### File: `schema.ts`

Create the main schema definition with all types, queries, and mutations:

```typescript
// schema.ts
import { Type, Query, Mutation, Field, ID, String, Int, Float, DateTime } from './decorators';

/**
 * User type: represents an authenticated user in the system
 */
@Type('User')
export class User {
  @Field(ID, { required: true })
  id!: string;

  @Field(String, { required: true })
  email!: string;

  @Field(String, { required: true })
  username!: string;

  @Field(String)
  fullName?: string;

  @Field(DateTime, { required: true })
  createdAt!: Date;

  @Field(DateTime, { required: true })
  updatedAt!: Date;
}

/**
 * Product type: represents a product in the catalog
 */
@Type('Product')
export class Product {
  @Field(ID, { required: true })
  id!: string;

  @Field(String, { required: true })
  name!: string;

  @Field(String, { required: true })
  description!: string;

  @Field(Float, { required: true })
  price!: number;

  @Field(String)
  sku?: string;

  @Field(Int, { required: true })
  inventory!: number;

  @Field(String)
  category?: string;

  @Field([String])
  tags?: string[];

  @Field(DateTime, { required: true })
  createdAt!: Date;

  @Field(DateTime, { required: true })
  updatedAt!: Date;
}

/**
 * OrderItem type: represents a line item in an order
 */
@Type('OrderItem')
export class OrderItem {
  @Field(ID, { required: true })
  id!: string;

  @Field(ID, { required: true })
  orderId!: string;

  @Field(ID, { required: true })
  productId!: string;

  @Field(Product, { required: true })
  product!: Product;

  @Field(Int, { required: true })
  quantity!: number;

  @Field(Float, { required: true })
  unitPrice!: number;

  @Field(Float, { required: true })
  subtotal!: number;
}

/**
 * Order type: represents a customer order
 */
@Type('Order')
export class Order {
  @Field(ID, { required: true })
  id!: string;

  @Field(ID, { required: true })
  userId!: string;

  @Field(User, { required: true })
  user!: User;

  @Field([OrderItem], { required: true })
  items!: OrderItem[];

  @Field(Float, { required: true })
  total!: number;

  @Field(String, { required: true })
  status!: 'pending' | 'confirmed' | 'shipped' | 'delivered' | 'cancelled';

  @Field(String)
  shippingAddress?: string;

  @Field(DateTime, { required: true })
  createdAt!: Date;

  @Field(DateTime, { required: true })
  updatedAt!: Date;
}

/**
 * Review type: represents a product review
 */
@Type('Review')
export class Review {
  @Field(ID, { required: true })
  id!: string;

  @Field(ID, { required: true })
  productId!: string;

  @Field(Product, { required: true })
  product!: Product;

  @Field(ID, { required: true })
  userId!: string;

  @Field(User, { required: true })
  user!: User;

  @Field(Int, { required: true })
  rating!: number; // 1-5

  @Field(String, { required: true })
  title!: string;

  @Field(String, { required: true })
  content!: string;

  @Field(DateTime, { required: true })
  createdAt!: Date;

  @Field(DateTime, { required: true })
  updatedAt!: Date;
}

/**
 * QueryRoot: all available queries
 */
@Query()
export class QueryRoot {
  /**
   * List all products with pagination
   */
  @Field([Product], { required: true })
  listProducts(
    @Field(Int) limit: number = 10,
    @Field(Int) offset: number = 0
  ): Product[] {
    return [];
  }

  /**
   * Get a single product by ID
   */
  @Field(Product)
  getProduct(@Field(ID, { required: true }) id: string): Product | null {
    return null;
  }

  /**
   * Search products by name, description, or tags
   */
  @Field([Product], { required: true })
  searchProducts(
    @Field(String, { required: true }) query: string,
    @Field(Int) limit: number = 20
  ): Product[] {
    return [];
  }

  /**
   * Get all orders for the current user
   */
  @Field([Order], { required: true })
  getOrders(
    @Field(String) status?: string,
    @Field(Int) limit: number = 10
  ): Order[] {
    return [];
  }

  /**
   * Get detailed information about an order
   */
  @Field(Order)
  getOrderDetails(@Field(ID, { required: true }) orderId: string): Order | null {
    return null;
  }

  /**
   * Get reviews for a product
   */
  @Field([Review], { required: true })
  getProductReviews(
    @Field(ID, { required: true }) productId: string,
    @Field(Int) limit: number = 10
  ): Review[] {
    return [];
  }

  /**
   * Get current user info
   */
  @Field(User)
  currentUser(): User | null {
    return null;
  }
}

/**
 * MutationRoot: all available mutations
 */
@Mutation()
export class MutationRoot {
  /**
   * Create a new order from cart items
   */
  @Field(Order)
  createOrder(
    @Field([ID], { required: true }) productIds: string[],
    @Field([Int], { required: true }) quantities: number[],
    @Field(String, { required: true }) shippingAddress: string
  ): Order | null {
    return null;
  }

  /**
   * Update an order status (admin only)
   */
  @Field(Order)
  updateOrder(
    @Field(ID, { required: true }) orderId: string,
    @Field(String) status?: string
  ): Order | null {
    return null;
  }

  /**
   * Add a review to a product
   */
  @Field(Review)
  addReview(
    @Field(ID, { required: true }) productId: string,
    @Field(Int, { required: true }) rating: number,
    @Field(String, { required: true }) title: string,
    @Field(String, { required: true }) content: string
  ): Review | null {
    return null;
  }

  /**
   * Update a product (admin only)
   */
  @Field(Product)
  updateProduct(
    @Field(ID, { required: true }) id: string,
    @Field(String) name?: string,
    @Field(String) description?: string,
    @Field(Float) price?: number,
    @Field(Int) inventory?: number
  ): Product | null {
    return null;
  }
}

export default {
  QueryRoot,
  MutationRoot,
  User,
  Product,
  Order,
  OrderItem,
  Review,
};
```text

### File: `types/User.ts`

Organize types in separate modules for scalability:

```typescript
// types/User.ts
import { Type, Field, ID, String, DateTime } from '../decorators';

@Type('User')
export class User {
  @Field(ID, { required: true })
  id!: string;

  @Field(String, { required: true })
  email!: string;

  @Field(String, { required: true })
  username!: string;

  @Field(String)
  fullName?: string;

  @Field(String)
  avatar?: string;

  @Field(DateTime, { required: true })
  createdAt!: Date;

  @Field(DateTime, { required: true })
  updatedAt!: Date;

  /**
   * Metadata for user preferences
   */
  @Field(String)
  preferences?: string; // JSON string in GraphQL
}
```text

### File: `types/Product.ts`

```typescript
// types/Product.ts
import { Type, Field, ID, String, Int, Float, DateTime } from '../decorators';

@Type('Product')
export class Product {
  @Field(ID, { required: true })
  id!: string;

  @Field(String, { required: true })
  name!: string;

  @Field(String, { required: true })
  description!: string;

  @Field(Float, { required: true })
  price!: number;

  @Field(String)
  sku?: string;

  @Field(Int, { required: true })
  inventory!: number;

  @Field(String)
  category?: string;

  @Field([String])
  tags?: string[];

  @Field(Float)
  rating?: number; // Average rating

  @Field(Int)
  reviewCount?: number;

  @Field(DateTime, { required: true })
  createdAt!: Date;

  @Field(DateTime, { required: true })
  updatedAt!: Date;
}
```text

### File: `types/Order.ts`

```typescript
// types/Order.ts
import { Type, Field, ID, String, Int, Float, DateTime } from '../decorators';
import { User } from './User';
import { OrderItem } from './OrderItem';

@Type('Order')
export class Order {
  @Field(ID, { required: true })
  id!: string;

  @Field(ID, { required: true })
  userId!: string;

  @Field(User, { required: true })
  user!: User;

  @Field([OrderItem], { required: true })
  items!: OrderItem[];

  @Field(Float, { required: true })
  subtotal!: number;

  @Field(Float)
  tax?: number;

  @Field(Float)
  shipping?: number;

  @Field(Float, { required: true })
  total!: number;

  @Field(String, { required: true })
  status!: 'pending' | 'confirmed' | 'shipped' | 'delivered' | 'cancelled';

  @Field(String)
  shippingAddress?: string;

  @Field(String)
  trackingNumber?: string;

  @Field(DateTime)
  shippedAt?: Date;

  @Field(DateTime)
  deliveredAt?: Date;

  @Field(DateTime, { required: true })
  createdAt!: Date;

  @Field(DateTime, { required: true })
  updatedAt!: Date;
}
```text

### File: `types/OrderItem.ts`

```typescript
// types/OrderItem.ts
import { Type, Field, ID, Int, Float } from '../decorators';
import { Product } from './Product';

@Type('OrderItem')
export class OrderItem {
  @Field(ID, { required: true })
  id!: string;

  @Field(ID, { required: true })
  orderId!: string;

  @Field(ID, { required: true })
  productId!: string;

  @Field(Product, { required: true })
  product!: Product;

  @Field(Int, { required: true })
  quantity!: number;

  @Field(Float, { required: true })
  unitPrice!: number;

  @Field(Float, { required: true })
  subtotal!: number;
}
```text

### File: `types/Review.ts`

```typescript
// types/Review.ts
import { Type, Field, ID, String, Int, DateTime } from '../decorators';
import { User } from './User';
import { Product } from './Product';

@Type('Review')
export class Review {
  @Field(ID, { required: true })
  id!: string;

  @Field(ID, { required: true })
  productId!: string;

  @Field(Product, { required: true })
  product!: Product;

  @Field(ID, { required: true })
  userId!: string;

  @Field(User, { required: true })
  user!: User;

  @Field(Int, { required: true })
  rating!: number; // 1-5

  @Field(String, { required: true })
  title!: string;

  @Field(String, { required: true })
  content!: string;

  @Field(Int)
  helpfulCount?: number;

  @Field(DateTime, { required: true })
  createdAt!: Date;

  @Field(DateTime, { required: true })
  updatedAt!: Date;
}
```text

### File: `decorators.ts`

Define the decorator API for schema definition:

```typescript
// decorators.ts
/**
 * Decorators for FraiseQL schema definition
 * These are development-time only and are compiled away.
 * No runtime FFI or bindings—just TypeScript metadata.
 */

// Scalar types
export const ID = Symbol('ID');
export const String = Symbol('String');
export const Int = Symbol('Int');
export const Float = Symbol('Float');
export const Boolean = Symbol('Boolean');
export const DateTime = Symbol('DateTime');
export const JSON = Symbol('JSON');

export interface FieldOptions {
  required?: boolean;
  description?: string;
  default?: any;
}

export interface TypeOptions {
  description?: string;
}

/**
 * Field decorator for type properties
 */
export function Field(
  type: symbol | Function | any[],
  options: FieldOptions = {}
) {
  return function (target: any, propertyKey: string) {
    const existingMetadata = Reflect.getOwnMetadata('fields', target) || {};
    existingMetadata[propertyKey] = {
      type,
      required: options.required ?? false,
      description: options.description,
      default: options.default,
    };
    Reflect.defineMetadata('fields', existingMetadata, target);
  };
}

/**
 * Type decorator for classes
 */
export function Type(name: string, options: TypeOptions = {}) {
  return function <T extends { new (...args: any[]): {} }>(constructor: T) {
    Reflect.defineMetadata('type:name', name, constructor);
    Reflect.defineMetadata('type:description', options.description, constructor);
    return constructor;
  };
}

/**
 * Query decorator for root query type
 */
export function Query(options: TypeOptions = {}) {
  return function <T extends { new (...args: any[]): {} }>(constructor: T) {
    Reflect.defineMetadata('query:root', true, constructor);
    Reflect.defineMetadata('query:description', options.description, constructor);
    return constructor;
  };
}

/**
 * Mutation decorator for root mutation type
 */
export function Mutation(options: TypeOptions = {}) {
  return function <T extends { new (...args: any[]): {} }>(constructor: T) {
    Reflect.defineMetadata('mutation:root', true, constructor);
    Reflect.defineMetadata('mutation:description', options.description, constructor);
    return constructor;
  };
}
```text

### File: `tsconfig.json`

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "ESNext",
    "lib": ["ES2020"],
    "experimentalDecorators": true,
    "emitDecoratorMetadata": true,
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "declaration": true,
    "declarationMap": true,
    "sourceMap": true,
    "outDir": "./dist",
    "rootDir": "./src"
  },
  "include": ["src/**/*"],
  "exclude": ["node_modules", "dist"]
}
```text

### File: `package.json` (Schema Authoring)

```json
{
  "name": "ecommerce-schema",
  "version": "1.0.0",
  "description": "E-commerce GraphQL schema for FraiseQL",
  "main": "dist/schema.js",
  "scripts": {
    "build": "tsc",
    "export": "node scripts/export-schema.js",
    "dev": "tsc --watch"
  },
  "dependencies": {
    "reflect-metadata": "^0.1.13"
  },
  "devDependencies": {
    "typescript": "^5.0.0",
    "@types/node": "^20.0.0"
  }
}
```text

### File: `scripts/export-schema.js`

Export compiled TypeScript metadata to `schema.json`:

```javascript
// scripts/export-schema.js
const fs = require('fs');
const path = require('path');

/**
 * Export TypeScript decorators to FraiseQL JSON schema
 * This script reads the compiled JavaScript with embedded metadata
 * and generates schema.json for compilation.
 */

require('reflect-metadata');
const schema = require('../dist/schema');

function extractTypeInfo(type) {
  const name = Reflect.getMetadata('type:name', type);
  const description = Reflect.getMetadata('type:description', type);
  const fieldsMetadata = Reflect.getOwnMetadata('fields', type.prototype) || {};

  const fields = Object.entries(fieldsMetadata).map(([key, meta]) => ({
    name: key,
    type: getTypeName(meta.type),
    required: meta.required,
    description: meta.description,
  }));

  return { name, description, fields };
}

function getTypeName(type) {
  if (typeof type === 'symbol') {
    const str = Symbol.keyFor(type) || type.toString();
    const match = str.match(/Symbol\((.*)\)/);
    return match ? match[1] : 'String';
  }
  if (typeof type === 'function') {
    return type.name;
  }
  if (Array.isArray(type)) {
    return `[${getTypeName(type[0])}]`;
  }
  return 'String';
}

function extractQueries(QueryRoot) {
  const isQuery = Reflect.getMetadata('query:root', QueryRoot);
  if (!isQuery) return [];

  const fieldsMetadata = Reflect.getOwnMetadata('fields', QueryRoot.prototype) || {};
  return Object.entries(fieldsMetadata).map(([key, meta]) => ({
    name: key,
    returnType: getTypeName(meta.type),
    isList: getTypeName(meta.type).startsWith('['),
  }));
}

const exportedSchema = schema.default;

const types = [
  'User',
  'Product',
  'Order',
  'OrderItem',
  'Review',
].map((typeName) => {
  const typeClass = exportedSchema[typeName];
  return extractTypeInfo(typeClass);
});

const queries = extractQueries(exportedSchema.QueryRoot);

const outputSchema = {
  version: '2.0.0',
  types,
  queries,
  mutations: [],
};

const outputPath = path.join(__dirname, '../schema.json');
fs.writeFileSync(outputPath, JSON.stringify(outputSchema, null, 2));
console.log(`✅ Schema exported to ${outputPath}`);
```text

---

## Part 2: Database Schema

The FraiseQL compiler generates SQL templates from the TypeScript schema. You define the actual database tables that back these types.

### File: `database/schema.sql`

```sql
-- Database schema for e-commerce application
-- These tables back the GraphQL types defined in schema.ts

-- Users table
CREATE TABLE users (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  email VARCHAR(255) NOT NULL UNIQUE,
  username VARCHAR(100) NOT NULL UNIQUE,
  full_name VARCHAR(255),
  avatar VARCHAR(255),
  preferences JSONB,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Products table
CREATE TABLE products (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  name VARCHAR(255) NOT NULL,
  description TEXT NOT NULL,
  price NUMERIC(10, 2) NOT NULL,
  sku VARCHAR(100) UNIQUE,
  inventory INTEGER NOT NULL DEFAULT 0,
  category VARCHAR(100),
  tags TEXT[] DEFAULT '{}',
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Indexes for common queries
CREATE INDEX idx_products_category ON products(category);
CREATE INDEX idx_products_tags ON products USING GIN(tags);
CREATE INDEX idx_products_name ON products(name);

-- Orders table
CREATE TABLE orders (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  subtotal NUMERIC(10, 2) NOT NULL,
  tax NUMERIC(10, 2),
  shipping NUMERIC(10, 2),
  total NUMERIC(10, 2) NOT NULL,
  status VARCHAR(20) NOT NULL DEFAULT 'pending',
  shipping_address TEXT,
  tracking_number VARCHAR(100),
  shipped_at TIMESTAMP,
  delivered_at TIMESTAMP,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_orders_user_id ON orders(user_id);
CREATE INDEX idx_orders_status ON orders(status);
CREATE INDEX idx_orders_created_at ON orders(created_at DESC);

-- Order items (line items in orders)
CREATE TABLE order_items (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  order_id UUID NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
  product_id UUID NOT NULL REFERENCES products(id) ON DELETE RESTRICT,
  quantity INTEGER NOT NULL CHECK (quantity > 0),
  unit_price NUMERIC(10, 2) NOT NULL,
  subtotal NUMERIC(10, 2) NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_order_items_order_id ON order_items(order_id);
CREATE INDEX idx_order_items_product_id ON order_items(product_id);

-- Reviews table
CREATE TABLE reviews (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
  user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  rating INTEGER NOT NULL CHECK (rating >= 1 AND rating <= 5),
  title VARCHAR(255) NOT NULL,
  content TEXT NOT NULL,
  helpful_count INTEGER NOT NULL DEFAULT 0,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_reviews_product_id ON reviews(product_id);
CREATE INDEX idx_reviews_user_id ON reviews(user_id);
CREATE INDEX idx_reviews_rating ON reviews(rating);

-- View for product information with aggregated data
CREATE OR REPLACE VIEW product_details AS
SELECT
  p.id,
  p.name,
  p.description,
  p.price,
  p.sku,
  p.inventory,
  p.category,
  p.tags,
  COALESCE(ROUND(AVG(r.rating), 2), 0) AS rating,
  COUNT(r.id) AS review_count,
  p.created_at,
  p.updated_at
FROM products p
LEFT JOIN reviews r ON p.id = r.product_id
GROUP BY p.id;

-- View for order details with items
CREATE OR REPLACE VIEW order_details AS
SELECT
  o.id,
  o.user_id,
  o.subtotal,
  o.tax,
  o.shipping,
  o.total,
  o.status,
  o.shipping_address,
  o.tracking_number,
  o.shipped_at,
  o.delivered_at,
  jsonb_agg(
    jsonb_build_object(
      'id', oi.id,
      'product_id', oi.product_id,
      'quantity', oi.quantity,
      'unit_price', oi.unit_price,
      'subtotal', oi.subtotal
    )
  ) AS items,
  o.created_at,
  o.updated_at
FROM orders o
LEFT JOIN order_items oi ON o.id = oi.order_id
GROUP BY o.id;

-- Function for product search
CREATE OR REPLACE FUNCTION search_products(search_query TEXT, limit_count INT DEFAULT 20)
RETURNS TABLE (id UUID, name VARCHAR, description TEXT, price NUMERIC, inventory INTEGER, category VARCHAR, tags TEXT[], rating NUMERIC, review_count BIGINT, created_at TIMESTAMP, updated_at TIMESTAMP)
LANGUAGE SQL
STABLE
AS $$
  SELECT p.id, p.name, p.description, p.price, p.inventory, p.category, p.tags, pd.rating, pd.review_count, p.created_at, p.updated_at
  FROM products p
  JOIN product_details pd ON p.id = pd.id
  WHERE
    p.name ILIKE '%' || search_query || '%'
    OR p.description ILIKE '%' || search_query || '%'
    OR search_query = ANY(p.tags)
  ORDER BY pd.rating DESC, pd.review_count DESC
  LIMIT limit_count;
$$;

-- Function for creating orders
CREATE OR REPLACE FUNCTION create_order_from_cart(
  p_user_id UUID,
  p_product_ids UUID[],
  p_quantities INT[],
  p_shipping_address TEXT
)
RETURNS UUID
LANGUAGE plpgsql
AS $$
DECLARE
  v_order_id UUID;
  v_total NUMERIC := 0;
  v_i INT;
  v_product_id UUID;
  v_quantity INT;
  v_price NUMERIC;
BEGIN
  -- Create order
  INSERT INTO orders (user_id, subtotal, total, shipping_address, status)
  VALUES (p_user_id, 0, 0, p_shipping_address, 'pending')
  RETURNING id INTO v_order_id;

  -- Add order items
  FOR v_i IN 1..array_length(p_product_ids, 1) LOOP
    v_product_id := p_product_ids[v_i];
    v_quantity := p_quantities[v_i];

    SELECT price INTO v_price FROM products WHERE id = v_product_id;

    INSERT INTO order_items (order_id, product_id, quantity, unit_price, subtotal)
    VALUES (v_order_id, v_product_id, v_quantity, v_price, v_price * v_quantity);

    v_total := v_total + (v_price * v_quantity);
  END LOOP;

  -- Update order total
  UPDATE orders SET subtotal = v_total, total = v_total, updated_at = CURRENT_TIMESTAMP
  WHERE id = v_order_id;

  RETURN v_order_id;
END;
$$;

-- Triggers for updated_at
CREATE OR REPLACE FUNCTION update_timestamp()
RETURNS TRIGGER AS $$
BEGIN
  NEW.updated_at = CURRENT_TIMESTAMP;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER users_updated_at BEFORE UPDATE ON users
  FOR EACH ROW EXECUTE FUNCTION update_timestamp();

CREATE TRIGGER products_updated_at BEFORE UPDATE ON products
  FOR EACH ROW EXECUTE FUNCTION update_timestamp();

CREATE TRIGGER orders_updated_at BEFORE UPDATE ON orders
  FOR EACH ROW EXECUTE FUNCTION update_timestamp();

CREATE TRIGGER reviews_updated_at BEFORE UPDATE ON reviews
  FOR EACH ROW EXECUTE FUNCTION update_timestamp();
```text

### File: `database/seed.sql`

Sample data for development:

```sql
-- Seed data for development

INSERT INTO users (email, username, full_name) VALUES
  ('alice@example.com', 'alice', 'Alice Johnson'),
  ('bob@example.com', 'bob', 'Bob Smith'),
  ('charlie@example.com', 'charlie', 'Charlie Davis');

INSERT INTO products (name, description, price, sku, inventory, category, tags) VALUES
  ('Laptop', 'Powerful laptop for development', 1299.99, 'LAPTOP-001', 5, 'Electronics', '{"laptop", "computer", "work"}'),
  ('Mechanical Keyboard', 'RGB mechanical keyboard', 149.99, 'KEYBOARD-001', 20, 'Accessories', '{"keyboard", "gaming", "mechanical"}'),
  ('Monitor', '4K UltraHD Monitor', 399.99, 'MONITOR-001', 8, 'Electronics', '{"monitor", "display", "4k"}'),
  ('Mouse', 'Ergonomic wireless mouse', 49.99, 'MOUSE-001', 50, 'Accessories', '{"mouse", "wireless", "ergonomic"}'),
  ('USB-C Cable', 'Premium USB-C charging cable', 19.99, 'CABLE-001', 100, 'Accessories', '{"cable", "usb-c", "charging"}');

INSERT INTO reviews (product_id, user_id, rating, title, content) VALUES
  ((SELECT id FROM products WHERE sku = 'LAPTOP-001'), (SELECT id FROM users WHERE username = 'alice'), 5, 'Excellent laptop!', 'Great performance and build quality.'),
  ((SELECT id FROM products WHERE sku = 'LAPTOP-001'), (SELECT id FROM users WHERE username = 'bob'), 4, 'Good but pricey', 'Works well but could be cheaper.'),
  ((SELECT id FROM products WHERE sku = 'KEYBOARD-001'), (SELECT id FROM users WHERE username = 'charlie'), 5, 'Perfect for gaming!', 'Love the RGB and mechanical switches.');
```text

---

## Part 3: Export and Compilation

### Step 1: Export TypeScript to JSON Schema

Run the export script to generate `schema.json`:

```bash
cd schema-authoring
npm install
npm run build
npm run export
```text

Output: `/schema.json`

```json
{
  "version": "2.0.0",
  "types": [
    {
      "name": "User",
      "fields": [
        { "name": "id", "type": "ID", "required": true },
        { "name": "email", "type": "String", "required": true },
        { "name": "username", "type": "String", "required": true },
        { "name": "fullName", "type": "String", "required": false },
        { "name": "createdAt", "type": "DateTime", "required": true },
        { "name": "updatedAt", "type": "DateTime", "required": true }
      ]
    },
    {
      "name": "Product",
      "fields": [
        { "name": "id", "type": "ID", "required": true },
        { "name": "name", "type": "String", "required": true },
        { "name": "description", "type": "String", "required": true },
        { "name": "price", "type": "Float", "required": true },
        { "name": "inventory", "type": "Int", "required": true },
        { "name": "category", "type": "String", "required": false },
        { "name": "tags", "type": "[String]", "required": false }
      ]
    }
  ],
  "queries": [
    { "name": "listProducts", "returnType": "Product", "isList": true },
    { "name": "getProduct", "returnType": "Product", "isList": false },
    { "name": "searchProducts", "returnType": "Product", "isList": true },
    { "name": "getOrders", "returnType": "Order", "isList": true },
    { "name": "getProductReviews", "returnType": "Review", "isList": true }
  ]
}
```text

### Step 2: Create FraiseQL Configuration

Create `fraiseql.toml`:

```toml
# fraiseql.toml
[fraiseql]
name = "ecommerce-api"
version = "1.0.0"
description = "E-commerce GraphQL API"

[fraiseql.database]
driver = "postgresql"
connection_string = "${DATABASE_URL:postgresql://localhost/ecommerce}"
pool_size = 20
pool_timeout_secs = 30

[fraiseql.server]
host = "0.0.0.0"
port = 8080
graphql_path = "/graphql"
introspection_enabled = true

[fraiseql.security]
# Authentication
auth_enabled = true
auth_jwt_secret = "${JWT_SECRET:dev-secret-key}"
auth_jwt_algorithm = "HS256"

# Rate limiting
rate_limiting_enabled = true
rate_limit_requests = 1000
rate_limit_window_secs = 60

# Error sanitization
error_sanitization_enabled = true
error_include_stack_trace = false

[fraiseql.logging]
level = "info"
format = "json"

[fraiseql.performance]
# Connection pooling
enable_connection_pooling = true
# Query caching
enable_query_cache = true
query_cache_ttl_secs = 3600
# APQ (Automatic Persisted Queries)
enable_apq = true
apq_cache_ttl_secs = 86400
```text

### Step 3: Compile with FraiseQL CLI

```bash
# Install fraiseql-cli (or use cargo)
cargo install fraiseql-cli

# Compile schema
fraiseql-cli compile schema.json fraiseql.toml --output schema.compiled.json

# Verify compilation
cat schema.compiled.json | head -50
```text

Output: `schema.compiled.json`

This file contains:

- Validated type definitions
- Optimized SQL templates (pre-compiled, zero runtime parsing)
- Embedded configuration (security, rate limiting, etc.)
- Query execution plan metadata

---

## Part 4: FraiseQL Server Deployment

### File: `Dockerfile`

```dockerfile
# Stage 1: Builder
FROM rust:1.75 as builder

WORKDIR /app

# Copy Cargo files
COPY Cargo.toml Cargo.lock ./

# Copy crates
COPY crates ./crates

# Build release
RUN cargo build --release --bin fraiseql-server

# Stage 2: Runtime
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
  ca-certificates \
  postgresql-client \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/target/release/fraiseql-server .

# Copy schema and config
COPY schema.compiled.json .
COPY fraiseql.toml .

# Health check
HEALTHCHECK --interval=10s --timeout=5s --start-period=5s --retries=3 \
  CMD /app/fraiseql-server health || exit 1

EXPOSE 8080

CMD ["./fraiseql-server", "--config", "fraiseql.toml", "--schema", "schema.compiled.json"]
```text

### File: `docker-compose.yml`

```yaml
version: '3.8'

services:
  postgres:
    image: postgres:16-alpine
    container_name: ecommerce-db
    environment:
      POSTGRES_DB: ecommerce
      POSTGRES_USER: fraiseql
      POSTGRES_PASSWORD: dev-password
    volumes:
      - postgres_data:/var/lib/postgresql/data
      - ./database/schema.sql:/docker-entrypoint-initdb.d/01-schema.sql
      - ./database/seed.sql:/docker-entrypoint-initdb.d/02-seed.sql
    ports:
      - "5432:5432"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U fraiseql"]
      interval: 10s
      timeout: 5s
      retries: 5

  graphql-api:
    build:
      context: ./backend
      dockerfile: Dockerfile
    container_name: ecommerce-api
    environment:
      DATABASE_URL: postgresql://fraiseql:dev-password@postgres:5432/ecommerce
      JWT_SECRET: dev-secret-key
      RUST_LOG: info
      RUST_BACKTRACE: 1
    ports:
      - "8080:8080"
    depends_on:
      postgres:
        condition: service_healthy
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 10s
      timeout: 5s
      retries: 3

  frontend:
    build:
      context: ./frontend
      dockerfile: Dockerfile
    container_name: ecommerce-frontend
    environment:
      VITE_API_URL: http://localhost:8080/graphql
    ports:
      - "5173:5173"
    depends_on:
      - graphql-api

volumes:
  postgres_data:

networks:
  default:
    name: ecommerce-network
```text

### File: `backend/.env.example`

```bash
# Database
DATABASE_URL=postgresql://fraiseql:dev-password@localhost:5432/ecommerce

# Authentication
JWT_SECRET=your-secret-key-change-in-production

# Logging
RUST_LOG=info

# Server
GRAPHQL_HOST=0.0.0.0
GRAPHQL_PORT=8080
```text

---

## Part 5: Vue 3 Frontend

### File: `frontend/src/apollo.ts`

Apollo Client configuration:

```typescript
// src/apollo.ts
import { ApolloClient, InMemoryCache, HttpLink, gql } from '@apollo/client/core';

const httpLink = new HttpLink({
  uri: import.meta.env.VITE_API_URL || 'http://localhost:8080/graphql',
  credentials: 'include',
  headers: {
    'Authorization': `Bearer ${localStorage.getItem('token') || ''}`,
  },
});

export const apolloClient = new ApolloClient({
  link: httpLink,
  cache: new InMemoryCache(),
  defaultOptions: {
    watchQuery: {
      fetchPolicy: 'cache-and-network',
    },
  },
});
```text

### File: `frontend/src/types/graphql.ts`

TypeScript types generated from schema:

```typescript
// src/types/graphql.ts
export interface User {
  id: string;
  email: string;
  username: string;
  fullName?: string;
  avatar?: string;
  createdAt: Date;
  updatedAt: Date;
}

export interface Product {
  id: string;
  name: string;
  description: string;
  price: number;
  sku?: string;
  inventory: number;
  category?: string;
  tags?: string[];
  rating?: number;
  reviewCount?: number;
  createdAt: Date;
  updatedAt: Date;
}

export interface OrderItem {
  id: string;
  product: Product;
  quantity: number;
  unitPrice: number;
  subtotal: number;
}

export interface Order {
  id: string;
  user: User;
  items: OrderItem[];
  subtotal: number;
  tax?: number;
  shipping?: number;
  total: number;
  status: 'pending' | 'confirmed' | 'shipped' | 'delivered' | 'cancelled';
  shippingAddress?: string;
  createdAt: Date;
  updatedAt: Date;
}

export interface Review {
  id: string;
  product: Product;
  user: User;
  rating: number;
  title: string;
  content: string;
  createdAt: Date;
  updatedAt: Date;
}

export interface CartItem {
  productId: string;
  quantity: number;
}
```text

### File: `frontend/src/composables/useProducts.ts`

Product querying composable:

```typescript
// src/composables/useProducts.ts
import { ref, computed } from 'vue';
import { useQuery, useLazyQuery } from '@vue/apollo-composable';
import { gql } from '@apollo/client/core';
import type { Product } from '@/types/graphql';

const PRODUCTS_QUERY = gql`
  query ListProducts($limit: Int, $offset: Int) {
    listProducts(limit: $limit, offset: $offset) {
      id
      name
      description
      price
      inventory
      category
      tags
      rating
      reviewCount
      createdAt
    }
  }
`;

const PRODUCT_SEARCH_QUERY = gql`
  query SearchProducts($query: String!, $limit: Int) {
    searchProducts(query: $query, limit: $limit) {
      id
      name
      description
      price
      inventory
      category
      rating
      reviewCount
    }
  }
`;

const SINGLE_PRODUCT_QUERY = gql`
  query GetProduct($id: ID!) {
    getProduct(id: $id) {
      id
      name
      description
      price
      sku
      inventory
      category
      tags
      rating
      reviewCount
      createdAt
      updatedAt
    }
  }
`;

export function useProducts() {
  const limit = ref(10);
  const offset = ref(0);

  const { result, loading, error, refetch } = useQuery(
    PRODUCTS_QUERY,
    { limit: limit.value, offset: offset.value }
  );

  const products = computed(() => result.value?.listProducts || []);

  return {
    products,
    loading,
    error,
    refetch,
    limit,
    offset,
  };
}

export function useSearchProducts() {
  const query = ref('');
  const { load: search, result, loading, error } = useLazyQuery(
    PRODUCT_SEARCH_QUERY
  );

  const results = computed(() => result.value?.searchProducts || []);

  const performSearch = (searchQuery: string) => {
    query.value = searchQuery;
    search({ query: searchQuery, limit: 20 });
  };

  return {
    query,
    results,
    loading,
    error,
    performSearch,
  };
}

export function useProductDetail(productId: string) {
  const { result, loading, error } = useQuery(
    SINGLE_PRODUCT_QUERY,
    { id: productId }
  );

  const product = computed(() => result.value?.getProduct || null);

  return {
    product,
    loading,
    error,
  };
}
```text

### File: `frontend/src/composables/useOrders.ts`

Order management composable:

```typescript
// src/composables/useOrders.ts
import { ref, computed } from 'vue';
import { useQuery, useMutation } from '@vue/apollo-composable';
import { gql } from '@apollo/client/core';
import type { Order, CartItem } from '@/types/graphql';

const ORDERS_QUERY = gql`
  query GetOrders($status: String, $limit: Int) {
    getOrders(status: $status, limit: $limit) {
      id
      user {
        id
        email
        username
      }
      items {
        id
        product {
          id
          name
          price
        }
        quantity
        unitPrice
        subtotal
      }
      subtotal
      tax
      shipping
      total
      status
      createdAt
    }
  }
`;

const CREATE_ORDER_MUTATION = gql`
  mutation CreateOrder($productIds: [ID]!, $quantities: [Int]!, $shippingAddress: String!) {
    createOrder(productIds: $productIds, quantities: $quantities, shippingAddress: $shippingAddress) {
      id
      total
      status
      createdAt
    }
  }
`;

const UPDATE_ORDER_MUTATION = gql`
  mutation UpdateOrder($orderId: ID!, $status: String) {
    updateOrder(orderId: $orderId, status: $status) {
      id
      status
      updatedAt
    }
  }
`;

export function useOrders(status?: string) {
  const { result, loading, error, refetch } = useQuery(ORDERS_QUERY, {
    status: status || null,
    limit: 10,
  });

  const orders = computed(() => result.value?.getOrders || []);

  return {
    orders,
    loading,
    error,
    refetch,
  };
}

export function useCreateOrder() {
  const { mutate, loading, error } = useMutation(CREATE_ORDER_MUTATION);

  const createOrder = async (
    cart: CartItem[],
    shippingAddress: string
  ) => {
    const productIds = cart.map((item) => item.productId);
    const quantities = cart.map((item) => item.quantity);

    const result = await mutate({
      productIds,
      quantities,
      shippingAddress,
    });

    return result?.data?.createOrder;
  };

  return {
    createOrder,
    loading,
    error,
  };
}

export function useUpdateOrder() {
  const { mutate, loading, error } = useMutation(UPDATE_ORDER_MUTATION);

  const updateOrder = async (orderId: string, status: string) => {
    const result = await mutate({ orderId, status });
    return result?.data?.updateOrder;
  };

  return {
    updateOrder,
    loading,
    error,
  };
}
```text

### File: `frontend/src/components/ProductGrid.vue`

Product listing component:

```vue
<!-- src/components/ProductGrid.vue -->
<template>
  <div class="product-grid">
    <div v-if="loading" class="loading">Loading products...</div>
    <div v-else-if="error" class="error">
      Error loading products: {{ error.message }}
    </div>
    <div v-else class="grid">
      <div
        v-for="product in products"
        :key="product.id"
        class="product-card"
      >
        <div class="product-header">
          <h3>{{ product.name }}</h3>
          <span v-if="product.rating" class="rating">
            ⭐ {{ product.rating }} ({{ product.reviewCount }} reviews)
          </span>
        </div>

        <p class="description">{{ product.description }}</p>

        <div class="product-footer">
          <div class="price">${{ product.price.toFixed(2) }}</div>
          <div class="inventory" :class="{ 'out-of-stock': product.inventory === 0 }">
            {{ product.inventory > 0 ? `${product.inventory} in stock` : 'Out of stock' }}
          </div>
        </div>

        <div class="actions">
          <router-link
            :to="`/products/${product.id}`"
            class="btn btn-secondary"
          >
            View Details
          </router-link>
          <button
            :disabled="product.inventory === 0"
            @click="addToCart(product.id)"
            class="btn btn-primary"
          >
            Add to Cart
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useProducts } from '@/composables/useProducts';
import { useCart } from '@/composables/useCart';
import type { Product } from '@/types/graphql';

const { products, loading, error } = useProducts();
const { addItem } = useCart();

const addToCart = (productId: string) => {
  addItem(productId, 1);
  alert('Added to cart!');
};
</script>

<style scoped>
.product-grid {
  padding: 2rem;
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: 2rem;
}

.product-card {
  border: 1px solid #e0e0e0;
  border-radius: 8px;
  padding: 1.5rem;
  display: flex;
  flex-direction: column;
  gap: 1rem;
  transition: box-shadow 0.3s;
}

.product-card:hover {
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.1);
}

.product-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 1rem;
}

.product-header h3 {
  margin: 0;
  font-size: 1.25rem;
}

.rating {
  font-size: 0.875rem;
  color: #666;
  white-space: nowrap;
}

.description {
  color: #666;
  margin: 0;
  font-size: 0.9rem;
  flex-grow: 1;
}

.product-footer {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.price {
  font-size: 1.5rem;
  font-weight: bold;
  color: #00a854;
}

.inventory {
  font-size: 0.875rem;
  color: #666;
}

.inventory.out-of-stock {
  color: #d9534f;
  font-weight: bold;
}

.actions {
  display: flex;
  gap: 0.5rem;
}

.btn {
  flex: 1;
  padding: 0.75rem;
  border: none;
  border-radius: 4px;
  cursor: pointer;
  font-size: 0.875rem;
  transition: background-color 0.2s;
}

.btn-primary {
  background-color: #0050b3;
  color: white;
}

.btn-primary:hover:not(:disabled) {
  background-color: #0039a6;
}

.btn-primary:disabled {
  background-color: #ccc;
  cursor: not-allowed;
}

.btn-secondary {
  background-color: #f5f5f5;
  color: #333;
  text-decoration: none;
  display: flex;
  align-items: center;
  justify-content: center;
}

.btn-secondary:hover {
  background-color: #e6e6e6;
}

.loading,
.error {
  text-align: center;
  padding: 2rem;
  font-size: 1.125rem;
}

.error {
  color: #d9534f;
}
</style>
```text

### File: `frontend/src/components/ShoppingCart.vue`

Shopping cart component:

```vue
<!-- src/components/ShoppingCart.vue -->
<template>
  <div class="cart">
    <h2>Shopping Cart</h2>

    <div v-if="isEmpty" class="empty">
      <p>Your cart is empty</p>
      <router-link to="/" class="btn btn-primary">Continue Shopping</router-link>
    </div>

    <div v-else>
      <div class="cart-items">
        <div v-for="item in items" :key="item.productId" class="cart-item">
          <div class="item-info">
            <h4>{{ item.productName }}</h4>
            <p class="price">${{ (item.unitPrice * item.quantity).toFixed(2) }}</p>
          </div>

          <div class="item-quantity">
            <button @click="decrementQuantity(item.productId)">-</button>
            <input v-model.number="item.quantity" type="number" min="1" />
            <button @click="incrementQuantity(item.productId)">+</button>
          </div>

          <button
            @click="removeItem(item.productId)"
            class="btn-remove"
          >
            Remove
          </button>
        </div>
      </div>

      <div class="cart-summary">
        <div class="summary-line">
          <span>Subtotal:</span>
          <span>${{ subtotal.toFixed(2) }}</span>
        </div>
        <div class="summary-line">
          <span>Tax (8%):</span>
          <span>${{ tax.toFixed(2) }}</span>
        </div>
        <div class="summary-line">
          <span>Shipping:</span>
          <span>${{ shipping.toFixed(2) }}</span>
        </div>
        <div class="summary-line total">
          <span>Total:</span>
          <span>${{ total.toFixed(2) }}</span>
        </div>

        <button
          @click="checkout"
          :disabled="isEmpty || isCheckingOut"
          class="btn btn-primary btn-checkout"
        >
          {{ isCheckingOut ? 'Processing...' : 'Checkout' }}
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue';
import { useCart } from '@/composables/useCart';

const { items, removeItem, updateQuantity } = useCart();
const isCheckingOut = ref(false);

const isEmpty = computed(() => items.value.length === 0);

const subtotal = computed(() =>
  items.value.reduce((sum, item) => sum + item.unitPrice * item.quantity, 0)
);

const tax = computed(() => subtotal.value * 0.08);
const shipping = computed(() => (subtotal.value > 100 ? 0 : 10));
const total = computed(() => subtotal.value + tax.value + shipping.value);

const incrementQuantity = (productId: string) => {
  const item = items.value.find((i) => i.productId === productId);
  if (item) {
    updateQuantity(productId, item.quantity + 1);
  }
};

const decrementQuantity = (productId: string) => {
  const item = items.value.find((i) => i.productId === productId);
  if (item && item.quantity > 1) {
    updateQuantity(productId, item.quantity - 1);
  }
};

const checkout = async () => {
  isCheckingOut.value = true;
  // TODO: implement checkout flow
  isCheckingOut.value = false;
};
</script>

<style scoped>
.cart {
  padding: 2rem;
  max-width: 900px;
  margin: 0 auto;
}

.empty {
  text-align: center;
  padding: 2rem;
  color: #666;
}

.cart-items {
  display: flex;
  flex-direction: column;
  gap: 1.5rem;
  margin-bottom: 2rem;
}

.cart-item {
  display: flex;
  align-items: center;
  gap: 1.5rem;
  padding: 1.5rem;
  border: 1px solid #e0e0e0;
  border-radius: 4px;
}

.item-info {
  flex: 1;
}

.item-info h4 {
  margin: 0;
  font-size: 1.125rem;
}

.item-info .price {
  margin: 0.5rem 0 0;
  color: #666;
}

.item-quantity {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.item-quantity input {
  width: 50px;
  padding: 0.5rem;
  text-align: center;
  border: 1px solid #e0e0e0;
  border-radius: 4px;
}

.item-quantity button {
  padding: 0.5rem 0.75rem;
  background-color: #f5f5f5;
  border: 1px solid #e0e0e0;
  border-radius: 4px;
  cursor: pointer;
}

.btn-remove {
  padding: 0.5rem 1rem;
  background-color: #f5f5f5;
  border: 1px solid #e0e0e0;
  border-radius: 4px;
  cursor: pointer;
  color: #d9534f;
}

.btn-remove:hover {
  background-color: #f0f0f0;
}

.cart-summary {
  padding: 2rem;
  border: 1px solid #e0e0e0;
  border-radius: 4px;
  background-color: #f9f9f9;
}

.summary-line {
  display: flex;
  justify-content: space-between;
  margin-bottom: 0.75rem;
  font-size: 0.95rem;
}

.summary-line.total {
  font-size: 1.25rem;
  font-weight: bold;
  padding-top: 0.75rem;
  border-top: 1px solid #e0e0e0;
  margin-bottom: 1.5rem;
}

.btn-checkout {
  width: 100%;
  padding: 1rem;
  background-color: #0050b3;
  color: white;
  border: none;
  border-radius: 4px;
  font-size: 1rem;
  cursor: pointer;
}

.btn-checkout:hover:not(:disabled) {
  background-color: #0039a6;
}

.btn-checkout:disabled {
  background-color: #ccc;
  cursor: not-allowed;
}
</style>
```text

### File: `frontend/src/composables/useCart.ts`

Cart state management:

```typescript
// src/composables/useCart.ts
import { ref, computed, watch } from 'vue';

interface CartItemDetail {
  productId: string;
  productName: string;
  quantity: number;
  unitPrice: number;
}

const STORAGE_KEY = 'ecommerce-cart';

const cartItems = ref<CartItemDetail[]>(loadCart());

function loadCart(): CartItemDetail[] {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    return stored ? JSON.parse(stored) : [];
  } catch {
    return [];
  }
}

function saveCart() {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(cartItems.value));
}

export function useCart() {
  const items = computed(() => cartItems.value);

  const addItem = (productId: string, quantity: number = 1) => {
    const existing = cartItems.value.find((i) => i.productId === productId);
    if (existing) {
      existing.quantity += quantity;
    } else {
      // TODO: fetch product details
      cartItems.value.push({
        productId,
        productName: 'Product', // placeholder
        quantity,
        unitPrice: 0,
      });
    }
    saveCart();
  };

  const removeItem = (productId: string) => {
    const index = cartItems.value.findIndex((i) => i.productId === productId);
    if (index > -1) {
      cartItems.value.splice(index, 1);
      saveCart();
    }
  };

  const updateQuantity = (productId: string, quantity: number) => {
    const item = cartItems.value.find((i) => i.productId === productId);
    if (item) {
      item.quantity = Math.max(1, quantity);
      saveCart();
    }
  };

  const clear = () => {
    cartItems.value = [];
    saveCart();
  };

  return {
    items,
    addItem,
    removeItem,
    updateQuantity,
    clear,
  };
}
```text

### File: `frontend/src/main.ts`

Vue application setup:

```typescript
// src/main.ts
import { createApp } from 'vue';
import { createRouter, createWebHistory } from 'vue-router';
import { provideApolloClient } from '@vue/apollo-composable';
import App from './App.vue';
import { apolloClient } from './apollo';

// Pages
import ProductsPage from './pages/Products.vue';
import ProductDetailPage from './pages/ProductDetail.vue';
import CartPage from './pages/Cart.vue';
import OrdersPage from './pages/Orders.vue';

const routes = [
  { path: '/', component: ProductsPage },
  { path: '/products/:id', component: ProductDetailPage },
  { path: '/cart', component: CartPage },
  { path: '/orders', component: OrdersPage },
];

const router = createRouter({
  history: createWebHistory(),
  routes,
});

const app = createApp(App);

provideApolloClient(apolloClient);
app.use(router);
app.mount('#app');
```text

### File: `frontend/package.json`

```json
{
  "name": "ecommerce-frontend",
  "version": "1.0.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "preview": "vite preview"
  },
  "dependencies": {
    "vue": "^3.3.0",
    "vue-router": "^4.2.0",
    "@apollo/client": "^3.8.0",
    "@vue/apollo-composable": "^4.1.0",
    "graphql": "^16.8.0"
  },
  "devDependencies": {
    "@vitejs/plugin-vue": "^4.4.0",
    "vite": "^4.5.0",
    "typescript": "^5.0.0",
    "vue-tsc": "^1.8.0"
  }
}
```text

---

## Part 6: Project Structure

```text
ecommerce-project/
├── schema-authoring/                 # TypeScript schema definition
│   ├── src/
│   │   ├── schema.ts                 # Main schema definition
│   │   ├── types/
│   │   │   ├── User.ts
│   │   │   ├── Product.ts
│   │   │   ├── Order.ts
│   │   │   ├── OrderItem.ts
│   │   │   └── Review.ts
│   │   ├── decorators.ts             # Decorator implementations
│   │   └── queries/                  # Query definitions
│   ├── scripts/
│   │   └── export-schema.js          # Export TypeScript → JSON
│   ├── tsconfig.json
│   └── package.json
│
├── backend/                          # Rust FraiseQL server
│   ├── schema.json                   # Exported schema
│   ├── schema.compiled.json          # Compiled schema (generated)
│   ├── fraiseql.toml                 # Configuration
│   ├── Dockerfile
│   └── src/
│       └── main.rs
│
├── database/                         # Database setup
│   ├── schema.sql                    # Table definitions
│   └── seed.sql                      # Sample data
│
├── frontend/                         # Vue 3 application
│   ├── src/
│   │   ├── components/
│   │   │   ├── ProductGrid.vue
│   │   │   ├── ProductDetail.vue
│   │   │   ├── ShoppingCart.vue
│   │   │   ├── OrderForm.vue
│   │   │   └── ReviewsList.vue
│   │   ├── composables/
│   │   │   ├── useProducts.ts
│   │   │   ├── useOrders.ts
│   │   │   ├── useCart.ts
│   │   │   └── useReviews.ts
│   │   ├── pages/
│   │   │   ├── Products.vue
│   │   │   ├── ProductDetail.vue
│   │   │   ├── Cart.vue
│   │   │   └── Orders.vue
│   │   ├── types/
│   │   │   └── graphql.ts
│   │   ├── apollo.ts
│   │   ├── main.ts
│   │   └── App.vue
│   ├── package.json
│   └── vite.config.ts
│
├── docker-compose.yml
└── README.md
```text

---

## Part 7: Running the Complete Stack

### Step 1: Set Up Development Environment

```bash
# Clone repository
git clone <repo> ecommerce-project
cd ecommerce-project

# Create environment files
cp backend/.env.example backend/.env
```text

### Step 2: Build Schema

```bash
cd schema-authoring
npm install
npm run build
npm run export

# Verify schema.json created
cat schema.json
```text

### Step 3: Compile Schema

```bash
cd ../backend

# Install FraiseQL CLI
cargo install fraiseql-cli

# Compile schema
fraiseql-cli compile ../schema-authoring/schema.json fraiseql.toml

# Verify schema.compiled.json created
ls -lh schema.compiled.json
```text

### Step 4: Start the Stack

```bash
cd ..

# Start all services (database, backend, frontend)
docker-compose up -d

# Wait for services to be healthy
docker-compose ps

# Check logs
docker-compose logs -f graphql-api
```text

### Step 5: Verify Services

```bash
# Check GraphQL endpoint
curl http://localhost:8080/graphql \
  -X POST \
  -H "Content-Type: application/json" \
  -d '{"query": "{ listProducts(limit: 5) { id name price } }"}'

# Check frontend
open http://localhost:5173
```text

---

## Part 8: Example Workflows

### Workflow 1: Product Search

**User action**: Search for "laptop"

**Frontend code**:

```typescript
// In ProductSearch.vue
const { performSearch, results } = useSearchProducts();

const handleSearch = (query: string) => {
  performSearch(query); // Triggers GraphQL query
};
```text

**GraphQL query**:

```graphql
query SearchProducts($query: String!, $limit: Int) {
  searchProducts(query: $query, limit: $limit) {
    id
    name
    description
    price
    rating
    reviewCount
  }
}
```text

**Rust execution** (FraiseQL):

1. Receives `searchProducts` query
2. Looks up compiled SQL template for `search_products` function
3. Executes: `SELECT ... FROM products WHERE name ILIKE '%laptop%' LIMIT 20`
4. Returns JSON to frontend
5. Apollo Client caches result

### Workflow 2: Create Order

**User action**: Click "Checkout" with 2 items in cart

**Frontend code**:

```typescript
const { createOrder } = useCreateOrder();

const handleCheckout = async () => {
  const cart = [
    { productId: 'prod-1', quantity: 1 },
    { productId: 'prod-2', quantity: 2 },
  ];

  const order = await createOrder(cart, '123 Main St');
  // Redirect to order confirmation
};
```text

**GraphQL mutation**:

```graphql
mutation CreateOrder($productIds: [ID]!, $quantities: [Int]!, $shippingAddress: String!) {
  createOrder(productIds: $productIds, quantities: $quantities, shippingAddress: $shippingAddress) {
    id
    total
    status
  }
}
```text

**SQL execution** (FraiseQL):

1. Receives `createOrder` mutation
2. Calls Postgres function `create_order_from_cart`
3. Function creates order, adds items, calculates total
4. Returns new order ID
5. Frontend updates cart state

### Workflow 3: Add Product Review

**User action**: Click "Leave Review" on product page

**Frontend code**:

```typescript
const { addReview } = useReviews();

const submitReview = async (productId: string, rating: number, title: string, content: string) => {
  const review = await addReview(productId, rating, title, content);
  // Show success message, refresh reviews
};
```text

**GraphQL mutation**:

```graphql
mutation AddReview($productId: ID!, $rating: Int!, $title: String!, $content: String!) {
  addReview(productId: $productId, rating: $rating, title: $title, content: $content) {
    id
    rating
    title
    content
    createdAt
  }
}
```text

**SQL execution**:

1. INSERT review record
2. Database trigger updates `updated_at` on products
3. Cached product aggregates invalidated
4. Frontend refetches product details with new rating

---

## Part 9: Production Deployment

### Option 1: Docker Compose (Single Server)

```bash
# Build and push images to registry
docker build -t myregistry/ecommerce-api:1.0.0 ./backend
docker build -t myregistry/ecommerce-frontend:1.0.0 ./frontend
docker push myregistry/ecommerce-api:1.0.0
docker push myregistry/ecommerce-frontend:1.0.0

# SSH to production server
ssh prod-server

# Update docker-compose.yml with production images
# Set production environment variables

docker-compose pull
docker-compose up -d

# Verify
curl https://api.ecommerce.com/health
```text

### Option 2: Kubernetes

Create `k8s/deployment.yaml`:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: graphql-api
spec:
  replicas: 3
  selector:
    matchLabels:
      app: graphql-api
  template:
    metadata:
      labels:
        app: graphql-api
    spec:
      containers:
      - name: api
        image: myregistry/ecommerce-api:1.0.0
        ports:
        - containerPort: 8080
        env:
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: app-secrets
              key: database-url
        - name: JWT_SECRET
          valueFrom:
            secretKeyRef:
              name: app-secrets
              key: jwt-secret
        livenessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 10
          periodSeconds: 10
```text

Deploy:

```bash
kubectl apply -f k8s/deployment.yaml
kubectl set image deployment/graphql-api api=myregistry/ecommerce-api:1.0.1
```text

### Environment Variables (Production)

```bash
# .env.production
DATABASE_URL=postgresql://user:pass@prod-db.example.com/ecommerce
JWT_SECRET=<generate-strong-secret>
GRAPHQL_HOST=0.0.0.0
GRAPHQL_PORT=8080
RUST_LOG=warn
ENABLE_INTROSPECTION=false
```text

---

## Part 10: Troubleshooting

### GraphQL Server Won't Start

**Error**: `connection refused to database`

**Solution**:

```bash
# Check database is running
docker-compose ps postgres

# Verify connection string
echo $DATABASE_URL

# Check PostgreSQL logs
docker-compose logs postgres

# Manually test connection
psql $DATABASE_URL
```text

### Schema Compilation Fails

**Error**: `Invalid schema: type Product has no fields`

**Solution**:

```bash
# Verify schema.json is valid JSON
cat schema.json | jq .

# Check export script output
npm run export --verbose

# Verify decorators are applied
npm run build && node -e "require('reflect-metadata'); console.log(require('./dist/schema.js'))"
```text

### Apollo Client Can't Connect

**Error**: `Network error: Failed to fetch`

**Solution**:

```bash
# Check CORS headers
curl -i http://localhost:8080/graphql

# Verify API URL in frontend config
cat frontend/.env

# Check browser console for network errors
# Development tools → Network → GraphQL requests

# Enable debug logging
VITE_DEBUG=true npm run dev
```text

### Product Not Showing in Cart

**Error**: `Cart shows "Product" instead of actual name`

**Solution**:

```typescript
// Update useCart.ts to fetch product details
const addItem = async (productId: string, quantity: number = 1) => {
  // Fetch product name from GraphQL
  const product = await apolloClient.query({
    query: GET_PRODUCT_QUERY,
    variables: { id: productId },
  });

  cartItems.value.push({
    productId,
    productName: product.data.getProduct.name,
    quantity,
    unitPrice: product.data.getProduct.price,
  });
};
```text

### Database Migration Issues

**Error**: `relation "orders" does not exist`

**Solution**:

```bash
# Run schema migration manually
docker exec ecommerce-db psql -U fraiseql -d ecommerce -f /docker-entrypoint-initdb.d/01-schema.sql

# Verify tables
docker exec ecommerce-db psql -U fraiseql -d ecommerce -c "\dt"

# Check logs
docker-compose logs postgres
```text

### Performance Issues

**Symptom**: Queries take > 1 second

**Solution**:

```sql
-- Check query execution plans
EXPLAIN ANALYZE
SELECT * FROM products
WHERE name ILIKE '%laptop%'
LIMIT 20;

-- Add missing indexes
CREATE INDEX idx_products_name_trgm ON products USING gin(name gin_trgm_ops);

-- Check cache settings in fraiseql.toml
[fraiseql.performance]
enable_query_cache = true
query_cache_ttl_secs = 3600
enable_apq = true  # Automatic Persisted Queries
```text

---

## Key Takeaways

1. **TypeScript is for authoring, not runtime** - Define your API shape with decorators, export to JSON, compile with Rust
2. **FraiseQL generates optimized SQL** - No n+1 queries, no runtime parsing, pre-compiled templates
3. **Layered independence** - Backend and frontend are completely decoupled, communicate via standard GraphQL
4. **Type safety end-to-end** - TypeScript types → GraphQL schema → Vue components (via Apollo)
5. **Configuration management** - Security settings flow from `fraiseql.toml` through compilation to runtime

This architecture demonstrates FraiseQL's core strength: **authoring simplicity** meets **compiled efficiency** with **frontend flexibility**.

---

## Next Steps

- **Add authentication**: Implement JWT validation in FraiseQL server
- **Add pagination**: Implement cursor-based pagination for large result sets
- **Add filtering**: Add dynamic filter support to queries
- **Add subscriptions**: Implement WebSocket subscriptions for real-time updates
- **Deploy to production**: Follow production deployment section
- **Monitor performance**: Set up observability with logs and metrics

See the main [FraiseQL documentation](../README.md) for more advanced topics.

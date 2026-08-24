/**
 * FraiseQL Field-Level Metadata Example
 *
 * This example demonstrates field-level access control, deprecation, and documentation:
 * - `requiresScope`: JWT scope required to access this field
 * - `deprecated`: marks the field deprecated, surfacing as `isDeprecated` /
 *   `deprecationReason` in introspection so generated clients can warn (#1025)
 * - `description`: Field documentation for GraphQL schema
 *
 * Field metadata allows fine-grained control over who can access which fields,
 * API versioning through deprecation markers, and rich schema documentation.
 */

import * as fraiseql from "../src/index";

// ============================================================================
// EXAMPLE 1: Field-Level Access Control (PII Protection)
// ============================================================================

// Register fields with access control
fraiseql.registerTypeFields(
  "User",
  [
    { name: "id", type: "ID", nullable: false },
    {
      name: "email",
      type: "Email",
      nullable: false,
      description: "User email address",
    },
    {
      name: "name",
      type: "String",
      nullable: false,
      description: "User full name",
    },
    {
      name: "salary",
      type: "Decimal",
      nullable: false,
      requiresScope: "read:User.salary",
      description: "Annual salary (requires HR scope to read)",
    },
    {
      name: "ssn",
      type: "String",
      nullable: false,
      // Exactly one scope. The compiled schema and the runtime field filter each
      // represent a single required scope, so a list is refused rather than truncated
      // — truncating it would grant the field to callers holding only the first.
      requiresScope: "pii:read",
      description: "Social security number (requires the PII scope)",
    },
  ],
  "User profile with sensitive fields protected by JWT scopes"
);

// ============================================================================
// EXAMPLE 2: API Versioning with Deprecation
// ============================================================================

fraiseql.registerTypeFields("Product", [
  { name: "id", type: "ID", nullable: false },
  {
    name: "name",
    type: "String",
    nullable: false,
    description: "Product name",
  },
  {
    name: "oldPrice",
    type: "Decimal",
    nullable: true,
    description: "Old pricing field (DEPRECATED)",
  },
  {
    name: "newPrice",
    type: "Decimal",
    nullable: false,
    description: "Current list price",
  },
  {
    name: "oldCategory",
    type: "String",
    nullable: true,
    description: "Single category (DEPRECATED - use categories instead)",
  },
  {
    name: "category",
    type: "String",
    nullable: false,
    description: "Primary product category",
  },
]);

// ============================================================================
// EXAMPLE 3: Rich Field Documentation
// ============================================================================

fraiseql.registerTypeFields("Order", [
  {
    name: "id",
    type: "ID",
    nullable: false,
    description: "Unique order identifier (globally unique)",
  },
  {
    name: "customerId",
    type: "ID",
    nullable: false,
    description: "Customer who placed the order",
  },
  {
    name: "items",
    type: "OrderItem",
    nullable: false,
    description: "Items in this order. Access requires order:read scope.",
  },
  {
    name: "subtotal",
    type: "Decimal",
    nullable: false,
    description: "Subtotal before tax and discounts",
  },
  {
    name: "discount",
    type: "Decimal",
    nullable: false,
    description:
      "Discount amount applied (cents). Requires orders:view_discounts scope.",
    requiresScope: "orders:view_discounts",
  },
  {
    name: "tax",
    type: "Decimal",
    nullable: false,
    description: "Sales tax calculated at order time",
  },
  {
    name: "total",
    type: "Decimal",
    nullable: false,
    description: "Final total (subtotal + tax - discount)",
  },
  {
    name: "internalNotes",
    type: "String",
    nullable: true,
    requiresScope: "orders:internal_notes",
    description:
      "Internal notes for customer service team only. Requires orders:internal_notes scope.",
  },
  {
    name: "customerNotes",
    type: "String",
    nullable: true,
    description: "Public notes from customer (e.g., delivery instructions)",
  },
]);

// ============================================================================
// EXAMPLE 4: Using field() Helper Function
// ============================================================================

// The field() function is a helper to create metadata objects
const accessControlledField = fraiseql.field({
  requiresScope: "read:sensitive",
  description: "This field requires special access permission",
});

// You can use it to build field definitions programmatically
fraiseql.registerTypeFields("Document", [
  { name: "id", type: "ID", nullable: false },
  {
    name: "publicTitle",
    type: "String",
    nullable: false,
    ...fraiseql.field({
      description: "Title visible to all users",
    }),
  },
  {
    name: "classifiedContent",
    type: "String",
    nullable: true,
    ...accessControlledField,
  },
  {
    name: "legacyId",
    type: "String",
    nullable: true,
    ...fraiseql.field({
      description: "Old identifier format",
      deprecated: "Use `id` instead",
    }),
  },
]);

// ============================================================================
// QUERIES
// ============================================================================

fraiseql.registerQuery(
  "getUser",
  "User",
  false,
  false,
  [{ name: "id", type: "ID", nullable: false }],
  "Get user by ID - sensitive fields (salary, SSN) require appropriate JWT scopes"
);

fraiseql.registerQuery(
  "getProduct",
  "Product",
  false,
  false,
  [{ name: "id", type: "ID", nullable: false }],
  "Get product"
);

fraiseql.registerQuery(
  "getOrder",
  "Order",
  false,
  false,
  [{ name: "id", type: "ID", nullable: false }],
  "Get order details - discount and internal_notes fields require special scopes"
);

// ============================================================================
// INPUT TYPES WITH FIELD DESCRIPTIONS
// ============================================================================

const CreateUserInput = fraiseql.input("CreateUserInput", [
  {
    name: "email",
    type: "Email",
    nullable: false,
    description: "User email address (must be unique)",
  },
  {
    name: "name",
    type: "String",
    nullable: false,
    description: "User full name",
  },
  {
    name: "role",
    type: "String",
    nullable: false,
    default: "user",
    description: 'User role: "admin", "moderator", or "user"',
  },
]);

// ============================================================================
// MUTATIONS
// ============================================================================

fraiseql.registerMutation(
  "createUser",
  "User",
  false,
  false,
  [{ name: "input", type: "CreateUserInput", nullable: false }],
  "Create a new user with optional role assignment"
);

// ============================================================================
// EXPORT SCHEMA
// ============================================================================

if (require.main === module) {
  fraiseql.exportSchema("schema.json");
  console.log("✅ Schema exported to schema.json");
  console.log("  ");
  console.log("  Field Metadata Examples:");
  console.log("    User:");
  console.log('      - salary: requires "read:User.salary" scope');
  console.log('      - ssn: requires ["pii:read", "hr:view_sensitive"] scopes');
  console.log("    ");
  console.log("    Product:");
  console.log("    ");
  console.log("    Order:");
  console.log('      - discount: requires "orders:view_discounts" scope');
  console.log('      - internalNotes: requires "orders:internal_notes" scope');
}

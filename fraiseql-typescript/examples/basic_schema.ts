/**
 * Example FraiseQL schema definition in TypeScript.
 *
 * This example demonstrates:
 * - Type definitions with @Type()
 * - Query definitions with @Query()
 * - Mutation definitions with @Mutation()
 * - Schema export to JSON
 *
 * Usage:
 *   npx tsx examples/basic_schema.ts
 *   # Creates schema.json that can be compiled with: fraiseql-cli compile schema.json
 */

import * as fraiseql from "../src/index";

// ============================================================================
// Type Definitions
// ============================================================================

/**
 * User type representing a user in the system.
 */
@fraiseql.type()
class User {
  id!: number;
  name!: string;
  email!: string;
  createdAt!: string;
  isActive!: boolean;
}

/**
 * Post type representing a blog post.
 */
@fraiseql.type()
class Post {
  id!: number;
  title!: string;
  content!: string;
  authorId!: number;
  published!: boolean;
  createdAt!: string;
}

// ============================================================================
// Manual Field Registration
// ============================================================================

// Since TypeScript doesn't preserve field type information at runtime by default,
// we manually register the fields for each type.

fraiseql.registerTypeFields("User", [
  { name: "id", type: "Int", nullable: false },
  { name: "name", type: "String", nullable: false },
  { name: "email", type: "String", nullable: false },
  { name: "createdAt", type: "String", nullable: false },
  { name: "isActive", type: "Boolean", nullable: false },
]);

fraiseql.registerTypeFields("Post", [
  { name: "id", type: "Int", nullable: false },
  { name: "title", type: "String", nullable: false },
  { name: "content", type: "String", nullable: false },
  { name: "authorId", type: "Int", nullable: false },
  { name: "published", type: "Boolean", nullable: false },
  { name: "createdAt", type: "String", nullable: false },
]);

// ============================================================================
// Query Definitions
// ============================================================================

/**
 * Get list of users with pagination.
 *
 * Note: In a full implementation, this would use @fraiseql.query({ sqlSource: "v_user" })
 * but TypeScript decorators are only valid on classes/methods. Instead, we use
 * registerQuery() below for manual registration.
 */
function users(_limit: number = 10, _offset: number = 0, _isActive?: boolean): User[] {
  // Function body not executed - only for type/metadata
  throw new Error("Not implemented");
}

fraiseql.registerQuery(
  "users",
  "User",
  true, // returns list
  false, // not nullable
  [
    { name: "limit", type: "Int", nullable: false, default: 10 },
    { name: "offset", type: "Int", nullable: false, default: 0 },
    { name: "isActive", type: "Boolean", nullable: true },
  ],
  "Get list of users with pagination",
  { sql_source: "v_user", auto_params: { limit: true, offset: true, where: true } }
);

/**
 * Get a single user by ID.
 */
function user(_id: number): User | null {
  // Function body not executed
  throw new Error("Not implemented");
}

fraiseql.registerQuery(
  "user",
  "User",
  false, // single item
  true, // nullable
  [{ name: "id", type: "Int", nullable: false }],
  "Get a single user by ID",
  { sql_source: "v_user" }
);

/**
 * Get list of posts with filtering.
 */
function posts(_authorId?: number, _published: boolean = true): Post[] {
  // Function body not executed
  throw new Error("Not implemented");
}

fraiseql.registerQuery(
  "posts",
  "Post",
  true, // returns list
  false, // not nullable
  [
    { name: "authorId", type: "Int", nullable: true },
    { name: "published", type: "Boolean", nullable: false, default: true },
  ],
  "Get list of posts with filtering",
  { sql_source: "v_post", auto_params: { limit: true, offset: true, where: true } }
);

// ============================================================================
// Mutation Definitions
// ============================================================================

/**
 * Create a new user.
 */
function createUser(_name: string, _email: string): User {
  // Function body not executed
  throw new Error("Not implemented");
}

fraiseql.registerMutation(
  "createUser",
  "User",
  false, // single item
  false, // not nullable
  [
    { name: "name", type: "String", nullable: false },
    { name: "email", type: "String", nullable: false },
  ],
  "Create a new user",
  { sql_source: "fn_create_user", operation: "CREATE" }
);

/**
 * Update an existing user.
 */
function updateUser(_id: number, _name?: string, _email?: string): User {
  // Function body not executed
  throw new Error("Not implemented");
}

fraiseql.registerMutation(
  "updateUser",
  "User",
  false, // single item
  false, // not nullable
  [
    { name: "id", type: "Int", nullable: false },
    { name: "name", type: "String", nullable: true },
    { name: "email", type: "String", nullable: true },
  ],
  "Update an existing user",
  { sql_source: "fn_update_user", operation: "UPDATE" }
);

/**
 * Delete a user.
 */
function deleteUser(_id: number): User {
  // Function body not executed
  throw new Error("Not implemented");
}

fraiseql.registerMutation(
  "deleteUser",
  "User",
  false, // single item
  false, // not nullable
  [{ name: "id", type: "Int", nullable: false }],
  "Delete a user",
  { sql_source: "fn_delete_user", operation: "DELETE" }
);

/**
 * Create a new blog post.
 */
function createPost(_title: string, _content: string, _authorId: number): Post {
  // Function body not executed
  throw new Error("Not implemented");
}

fraiseql.registerMutation(
  "createPost",
  "Post",
  false, // single item
  false, // not nullable
  [
    { name: "title", type: "String", nullable: false },
    { name: "content", type: "String", nullable: false },
    { name: "authorId", type: "Int", nullable: false },
  ],
  "Create a new blog post",
  { sql_source: "fn_create_post", operation: "CREATE" }
);

// ============================================================================
// Export Schema
// ============================================================================

// Export schema to JSON when run as main module
if (require.main === module) {
  fraiseql.exportSchema("schema.json");

  console.log("\n✅ Schema exported successfully!");
  console.log("   Next steps:");
  console.log("   1. Compile schema: fraiseql-cli compile schema.json");
  console.log("   2. Start server: fraiseql-server --schema schema.compiled.json");
}

// Also export for use as a module
export { User, Post };

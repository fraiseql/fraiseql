/**
 * A minimal FraiseQL schema authored with the TypeScript SDK.
 *
 * Start here. It shows the whole authoring surface in one file: declare a type's
 * fields, declare the queries and mutations over them, export, compile.
 *
 * **Why there are no decorators here.** TypeScript erases types at runtime, so a
 * `@Type()` decorator can see a class's *name* and nothing else — not its fields, not
 * their GraphQL types, not their nullability. The SDK used to register placeholder
 * types from decorators and emit them with empty `fields`; it now refuses instead
 * (#733). And a decorator cannot go on a bare `function` at all — that is not valid
 * TypeScript, whatever the syntax looks like in a Python example. So the authoring
 * surface is the explicit `registerTypeFields` / `registerQuery` / `registerMutation`
 * functions, and the `interface`s below are ordinary TypeScript for your own code.
 *
 * Usage:
 *   npx tsx examples/basic_schema.ts
 *   fraiseql-cli compile schema.json
 */

import {
  exportSchema,
  registerMutation,
  registerQuery,
  registerTypeFields,
} from "../src/index";

// ============================================================================
// Types
// ============================================================================

/** A user of the system. */
export interface User {
  id: number;
  name: string;
  email: string;
  createdAt: string;
  isActive: boolean;
}

/** A blog post, authored by a user. */
export interface Post {
  id: number;
  title: string;
  content: string;
  authorId: number;
  published: boolean;
  createdAt: string;
}

// `sqlSource` is the view each type is read from. FraiseQL's convention is singular:
// `v_user` serves the `users` list field just as it serves the `user` single field.
registerTypeFields(
  "User",
  [
    { name: "id", type: "Int", nullable: false },
    { name: "name", type: "String", nullable: false },
    { name: "email", type: "String", nullable: false },
    { name: "createdAt", type: "String", nullable: false },
    { name: "isActive", type: "Boolean", nullable: false },
  ],
  "A user of the system",
  { sqlSource: "v_user" }
);

registerTypeFields(
  "Post",
  [
    { name: "id", type: "Int", nullable: false },
    { name: "title", type: "String", nullable: false },
    { name: "content", type: "String", nullable: false },
    { name: "authorId", type: "Int", nullable: false },
    { name: "published", type: "Boolean", nullable: false },
    { name: "createdAt", type: "String", nullable: false },
  ],
  "A blog post",
  { sqlSource: "v_post" }
);

// ============================================================================
// Queries
// ============================================================================

// Arguments are declared, not inferred. The booleans after the return type are
// `returnsList` and `nullable`, in that order.
registerQuery(
  "users",
  "User",
  true,
  false,
  [{ name: "isActive", type: "Boolean", nullable: true }],
  "List users, optionally filtered by active state",
  { sql_source: "v_user" }
);

registerQuery(
  "user",
  "User",
  false,
  true,
  [{ name: "id", type: "Int", nullable: false }],
  "Fetch one user by id, or null",
  { sql_source: "v_user" }
);

registerQuery(
  "posts",
  "Post",
  true,
  false,
  [
    { name: "authorId", type: "Int", nullable: true },
    { name: "published", type: "Boolean", nullable: false, default: true },
  ],
  "List posts, optionally filtered by author",
  { sql_source: "v_post" }
);

// ============================================================================
// Mutations
// ============================================================================

// `operation` is the DML verb the compiler lowers to: `insert`, `update` or `delete`.
// `sql_source` is the function that performs it, not a view.
registerMutation(
  "createUser",
  "User",
  false,
  false,
  [
    { name: "name", type: "String", nullable: false },
    { name: "email", type: "String", nullable: false },
  ],
  "Create a user",
  { sql_source: "fn_create_user", operation: "insert", invalidates_views: ["v_user"] }
);

registerMutation(
  "updateUser",
  "User",
  false,
  false,
  [
    { name: "id", type: "Int", nullable: false },
    { name: "name", type: "String", nullable: true },
    { name: "email", type: "String", nullable: true },
  ],
  "Update a user; omitted arguments are left untouched",
  { sql_source: "fn_update_user", operation: "update", invalidates_views: ["v_user"] }
);

registerMutation(
  "createPost",
  "Post",
  false,
  false,
  [
    { name: "title", type: "String", nullable: false },
    { name: "content", type: "String", nullable: false },
    { name: "authorId", type: "Int", nullable: false },
  ],
  "Create a post",
  { sql_source: "fn_create_post", operation: "insert", invalidates_views: ["v_post"] }
);

// ============================================================================
// Export
// ============================================================================

exportSchema("schema.json");
console.log("Next: fraiseql-cli compile schema.json");

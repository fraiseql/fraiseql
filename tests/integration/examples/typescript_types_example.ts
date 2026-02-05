/**
 * Example: TypeScript SDK generating minimal types.json for TOML-based workflow
 *
 * This example shows how to use the TypeScript FraiseQL SDK to:
 * 1. Define GraphQL types with @Type decorator
 * 2. Export minimal types.json (types only, no queries/mutations)
 * 3. Combine with fraiseql.toml for complete schema compilation
 *
 * Usage:
 *   npx ts-node typescript_types_example.ts
 *   # Generates: types.json
 *
 * Then compile with:
 *   fraiseql compile fraiseql.toml --types types.json
 *   # Generates: schema.compiled.json
 */

import * as fraiseql from "fraiseql";

/**
 * User type - represents a user in the system
 */
@fraiseql.Type({ description: "User in the system" })
class User {
  id: string;
  name: string;
  email: string;
  createdAt: string;
}

/**
 * Post type - represents a blog post
 */
@fraiseql.Type({ description: "Blog post" })
class Post {
  id: string;
  title: string;
  content: string;
  authorId: string;
  createdAt: string;
}

/**
 * Comment type - represents a comment on a post
 */
@fraiseql.Type({ description: "Comment on a post" })
class Comment {
  id: string;
  text: string;
  postId: string;
  authorId: string;
  createdAt: string;
}

async function main() {
  // Register all types
  fraiseql.registerTypeFields("User", [
    { name: "id", type: "ID", nullable: false },
    { name: "name", type: "String", nullable: false },
    { name: "email", type: "String", nullable: false },
    { name: "createdAt", type: "DateTime", nullable: false },
  ]);

  fraiseql.registerTypeFields("Post", [
    { name: "id", type: "ID", nullable: false },
    { name: "title", type: "String", nullable: false },
    { name: "content", type: "String", nullable: false },
    { name: "authorId", type: "ID", nullable: false },
    { name: "createdAt", type: "DateTime", nullable: false },
  ]);

  fraiseql.registerTypeFields("Comment", [
    { name: "id", type: "ID", nullable: false },
    { name: "text", type: "String", nullable: false },
    { name: "postId", type: "ID", nullable: false },
    { name: "authorId", type: "ID", nullable: false },
    { name: "createdAt", type: "DateTime", nullable: false },
  ]);

  // Export minimal types.json (types only, no queries/mutations/federation/security)
  fraiseql.exportTypes("types.json", { pretty: true });

  console.log("✅ Generated types.json");
  console.log("   Types: 3 (User, Post, Comment)");
  console.log("\n🎯 Next steps:");
  console.log("   1. fraiseql compile fraiseql.toml --types types.json");
  console.log("   2. This merges types.json with fraiseql.toml configuration");
  console.log("   3. Result: schema.compiled.json with types + all config");
}

main().catch(console.error);

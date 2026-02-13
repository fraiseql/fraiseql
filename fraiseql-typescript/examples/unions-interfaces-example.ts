/**
 * FraiseQL Unions and Interfaces Example
 *
 * Interfaces: Define shared field contracts across types
 * Unions: Define polymorphic types (multiple alternatives)
 *
 * This example shows:
 * - Defining interfaces with shared fields
 * - Types implementing interfaces
 * - Creating union types for polymorphic queries
 * - Querying union types
 */

import * as fraiseql from "../src/index";

// ============================================================================
// INTERFACES - Shared field definitions
// ============================================================================

// Node interface: Standard for entities with global IDs
const Node = fraiseql.interface_("Node", [
  { name: "id", type: "ID", nullable: false },
  { name: "createdAt", type: "DateTime", nullable: false },
  { name: "updatedAt", type: "DateTime", nullable: false },
]);

// Publishable interface: For content that can be published
const Publishable = fraiseql.interface_("Publishable", [
  { name: "published", type: "Boolean", nullable: false },
  { name: "publishedAt", type: "DateTime", nullable: true },
]);

// ============================================================================
// TYPES IMPLEMENTING INTERFACES
// ============================================================================

// User type implements Node
@fraiseql.Type()
class User {
  id!: string;
  email!: string;
  name!: string;
  createdAt!: string;
  updatedAt!: string;
}

fraiseql.registerTypeFields("User", [
  { name: "id", type: "ID", nullable: false },
  { name: "email", type: "Email", nullable: false },
  { name: "name", type: "String", nullable: false },
  { name: "createdAt", type: "DateTime", nullable: false },
  { name: "updatedAt", type: "DateTime", nullable: false },
]);

// Post type implements Node and Publishable
@fraiseql.Type()
class Post {
  id!: string;
  title!: string;
  content!: string;
  authorId!: string;
  published!: boolean;
  publishedAt!: string | null;
  createdAt!: string;
  updatedAt!: string;
}

fraiseql.registerTypeFields("Post", [
  { name: "id", type: "ID", nullable: false },
  { name: "title", type: "String", nullable: false },
  { name: "content", type: "String", nullable: false },
  { name: "authorId", type: "ID", nullable: false },
  { name: "published", type: "Boolean", nullable: false },
  { name: "publishedAt", type: "DateTime", nullable: true },
  { name: "createdAt", type: "DateTime", nullable: false },
  { name: "updatedAt", type: "DateTime", nullable: false },
]);

// Comment type implements Node and Publishable
@fraiseql.Type()
class Comment {
  id!: string;
  content!: string;
  authorId!: string;
  postId!: string;
  published!: boolean;
  publishedAt!: string | null;
  createdAt!: string;
  updatedAt!: string;
}

fraiseql.registerTypeFields("Comment", [
  { name: "id", type: "ID", nullable: false },
  { name: "content", type: "String", nullable: false },
  { name: "authorId", type: "ID", nullable: false },
  { name: "postId", type: "ID", nullable: false },
  { name: "published", type: "Boolean", nullable: false },
  { name: "publishedAt", type: "DateTime", nullable: true },
  { name: "createdAt", type: "DateTime", nullable: false },
  { name: "updatedAt", type: "DateTime", nullable: false },
]);

// ============================================================================
// UNION TYPES - Polymorphic return types
// ============================================================================

// SearchResult: User, Post, or Comment
const SearchResult = fraiseql.union("SearchResult", ["User", "Post", "Comment"], {
  description: "Result of searching across content",
});

// PublishedContent: Post or Comment
const PublishedContent = fraiseql.union("PublishedContent", ["Post", "Comment"], {
  description: "Content that has been published",
});

// NodeType: Any entity with a Node interface
const NodeType = fraiseql.union("NodeType", ["User", "Post", "Comment"], {
  description: "Any entity with a unique ID",
});

// ============================================================================
// QUERIES - Returning unions and implementing interfaces
// ============================================================================

@fraiseql.Query({ sqlSource: "v_user" })
function getUser(id: string): User {
  pass;
}

fraiseql.registerQuery(
  "getUser",
  "User",
  false,
  false,
  [{ name: "id", type: "ID", nullable: false }],
  "Get user by ID"
);

@fraiseql.Query({ sqlSource: "v_post" })
function getPost(id: string): Post {
  pass;
}

fraiseql.registerQuery(
  "getPost",
  "Post",
  false,
  false,
  [{ name: "id", type: "ID", nullable: false }],
  "Get post by ID"
);

// Search across multiple types - returns union
@fraiseql.Query()
function search(query: string, limit: number = 10): unknown[] {
  pass;
}

fraiseql.registerQuery(
  "search",
  "SearchResult",
  true,
  false,
  [
    { name: "query", type: "String", nullable: false },
    { name: "limit", type: "Int", nullable: false, default: 10 },
  ],
  "Search users, posts, and comments"
);

// Get published content - returns union
@fraiseql.Query()
function getPublishedContent(limit: number = 10): unknown[] {
  pass;
}

fraiseql.registerQuery(
  "getPublishedContent",
  "PublishedContent",
  true,
  false,
  [{ name: "limit", type: "Int", nullable: false, default: 10 }],
  "Get all published posts and comments"
);

// Get node by ID - returns union
@fraiseql.Query()
function getNode(id: string): unknown {
  pass;
}

fraiseql.registerQuery(
  "getNode",
  "NodeType",
  false,
  false,
  [{ name: "id", type: "ID", nullable: false }],
  "Get any node by ID (user, post, or comment)"
);

// ============================================================================
// MUTATIONS
// ============================================================================

@fraiseql.Mutation({ sqlSource: "fn_create_post", operation: "CREATE" })
function createPost(title: string, content: string, authorId: string): Post {
  pass;
}

fraiseql.registerMutation(
  "createPost",
  "Post",
  false,
  false,
  [
    { name: "title", type: "String", nullable: false },
    { name: "content", type: "String", nullable: false },
    { name: "authorId", type: "ID", nullable: false },
  ],
  "Create a new post"
);

@fraiseql.Mutation({ sqlSource: "fn_publish_post", operation: "UPDATE" })
function publishPost(id: string): Post {
  pass;
}

fraiseql.registerMutation(
  "publishPost",
  "Post",
  false,
  false,
  [{ name: "id", type: "ID", nullable: false }],
  "Publish a post"
);

// ============================================================================
// Export Schema
// ============================================================================

if (require.main === module) {
  fraiseql.exportSchema("schema.json");
  console.log("✅ Schema exported to schema.json");
  console.log("  Interfaces: Node, Publishable");
  console.log("  Unions: SearchResult, PublishedContent, NodeType");
  console.log("  Types: User, Post, Comment");
}

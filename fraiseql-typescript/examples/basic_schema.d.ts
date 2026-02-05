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
/**
 * User type representing a user in the system.
 */
declare class User {
  id: number;
  name: string;
  email: string;
  createdAt: string;
  isActive: boolean;
}
/**
 * Post type representing a blog post.
 */
declare class Post {
  id: number;
  title: string;
  content: string;
  authorId: number;
  published: boolean;
  createdAt: string;
}
export { User, Post };
//# sourceMappingURL=basic_schema.d.ts.map

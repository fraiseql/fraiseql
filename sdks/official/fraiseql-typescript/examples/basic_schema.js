"use strict";
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
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __decorate = (this && this.__decorate) || function (decorators, target, key, desc) {
    var c = arguments.length, r = c < 3 ? target : desc === null ? desc = Object.getOwnPropertyDescriptor(target, key) : desc, d;
    if (typeof Reflect === "object" && typeof Reflect.decorate === "function") r = Reflect.decorate(decorators, target, key, desc);
    else for (var i = decorators.length - 1; i >= 0; i--) if (d = decorators[i]) r = (c < 3 ? d(r) : c > 3 ? d(target, key, r) : d(target, key)) || r;
    return c > 3 && r && Object.defineProperty(target, key, r), r;
};
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
exports.Post = exports.User = void 0;
const fraiseql = __importStar(require("../src/index"));
// ============================================================================
// Type Definitions
// ============================================================================
/**
 * User type representing a user in the system.
 */
let User = class User {
    id;
    name;
    email;
    createdAt;
    isActive;
};
exports.User = User;
exports.User = User = __decorate([
    fraiseql.type()
], User);
/**
 * Post type representing a blog post.
 */
let Post = class Post {
    id;
    title;
    content;
    authorId;
    published;
    createdAt;
};
exports.Post = Post;
exports.Post = Post = __decorate([
    fraiseql.type()
], Post);
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
 */
function users(limit = 10, offset = 0, isActive) {
    // Function body not executed - only for type/metadata
    throw new Error("Not implemented");
}
fraiseql.registerQuery("users", "User", true, // returns list
false, // not nullable
[
    { name: "limit", type: "Int", nullable: false, default: 10 },
    { name: "offset", type: "Int", nullable: false, default: 0 },
    { name: "isActive", type: "Boolean", nullable: true },
], "Get list of users with pagination", { sql_source: "v_user", auto_params: { limit: true, offset: true, where: true } });
/**
 * Get a single user by ID.
 */
function user(id) {
    // Function body not executed
    throw new Error("Not implemented");
}
fraiseql.registerQuery("user", "User", false, // single item
true, // nullable
[{ name: "id", type: "Int", nullable: false }], "Get a single user by ID", { sql_source: "v_user" });
/**
 * Get list of posts with filtering.
 */
function posts(authorId, published = true) {
    // Function body not executed
    throw new Error("Not implemented");
}
fraiseql.registerQuery("posts", "Post", true, // returns list
false, // not nullable
[
    { name: "authorId", type: "Int", nullable: true },
    { name: "published", type: "Boolean", nullable: false, default: true },
], "Get list of posts with filtering", { sql_source: "v_post", auto_params: { limit: true, offset: true, where: true } });
// ============================================================================
// Mutation Definitions
// ============================================================================
/**
 * Create a new user.
 */
function createUser(name, email) {
    // Function body not executed
    throw new Error("Not implemented");
}
fraiseql.registerMutation("createUser", "User", false, // single item
false, // not nullable
[
    { name: "name", type: "String", nullable: false },
    { name: "email", type: "String", nullable: false },
], "Create a new user", { sql_source: "fn_create_user", operation: "CREATE" });
/**
 * Update an existing user.
 */
function updateUser(id, name, email) {
    // Function body not executed
    throw new Error("Not implemented");
}
fraiseql.registerMutation("updateUser", "User", false, // single item
false, // not nullable
[
    { name: "id", type: "Int", nullable: false },
    { name: "name", type: "String", nullable: true },
    { name: "email", type: "String", nullable: true },
], "Update an existing user", { sql_source: "fn_update_user", operation: "UPDATE" });
/**
 * Delete a user.
 */
function deleteUser(id) {
    // Function body not executed
    throw new Error("Not implemented");
}
fraiseql.registerMutation("deleteUser", "User", false, // single item
false, // not nullable
[{ name: "id", type: "Int", nullable: false }], "Delete a user", { sql_source: "fn_delete_user", operation: "DELETE" });
/**
 * Create a new blog post.
 */
function createPost(title, content, authorId) {
    // Function body not executed
    throw new Error("Not implemented");
}
fraiseql.registerMutation("createPost", "Post", false, // single item
false, // not nullable
[
    { name: "title", type: "String", nullable: false },
    { name: "content", type: "String", nullable: false },
    { name: "authorId", type: "Int", nullable: false },
], "Create a new blog post", { sql_source: "fn_create_post", operation: "CREATE" });
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

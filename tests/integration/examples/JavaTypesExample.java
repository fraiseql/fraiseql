/**
 * Example: Java SDK generating minimal types.json for TOML-based workflow
 *
 * This example shows how to use the Java FraiseQL SDK to:
 * 1. Define GraphQL types with @GraphQLType annotation
 * 2. Export minimal types.json (types only, no queries/mutations)
 * 3. Combine with fraiseql.toml for complete schema compilation
 *
 * Usage:
 *   javac -cp fraiseql-java.jar JavaTypesExample.java
 *   java -cp fraiseql-java.jar:. JavaTypesExample
 *   # Generates: types.json
 *
 * Then compile with:
 *   fraiseql compile fraiseql.toml --types types.json
 *   # Generates: schema.compiled.json
 */

import com.fraiseql.core.FraiseQL;
import com.fraiseql.core.GraphQLType;
import com.fraiseql.core.GraphQLField;
import java.io.IOException;

/**
 * User type - represents a user in the system
 */
@GraphQLType(name = "User", description = "User in the system")
class User {
    @GraphQLField(type = "ID")
    public String id;

    @GraphQLField(type = "String")
    public String name;

    @GraphQLField(type = "String")
    public String email;

    @GraphQLField(type = "DateTime")
    public String createdAt;
}

/**
 * Post type - represents a blog post
 */
@GraphQLType(name = "Post", description = "Blog post")
class Post {
    @GraphQLField(type = "ID")
    public String id;

    @GraphQLField(type = "String")
    public String title;

    @GraphQLField(type = "String")
    public String content;

    @GraphQLField(type = "ID")
    public String authorId;

    @GraphQLField(type = "DateTime")
    public String createdAt;
}

/**
 * Comment type - represents a comment on a post
 */
@GraphQLType(name = "Comment", description = "Comment on a post")
class Comment {
    @GraphQLField(type = "ID")
    public String id;

    @GraphQLField(type = "String")
    public String text;

    @GraphQLField(type = "ID")
    public String postId;

    @GraphQLField(type = "ID")
    public String authorId;

    @GraphQLField(type = "DateTime")
    public String createdAt;
}

public class JavaTypesExample {
    public static void main(String[] args) throws IOException {
        // Register all types
        FraiseQL.registerTypes(User.class, Post.class, Comment.class);

        // Export minimal types.json (types only, no queries/mutations/federation/security)
        FraiseQL.exportTypes("types.json", true);

        System.out.println("✅ Generated types.json");
        System.out.println("   Types: 3 (User, Post, Comment)");
        System.out.println("\n🎯 Next steps:");
        System.out.println("   1. fraiseql compile fraiseql.toml --types types.json");
        System.out.println("   2. This merges types.json with fraiseql.toml configuration");
        System.out.println("   3. Result: schema.compiled.json with types + all config");
    }
}

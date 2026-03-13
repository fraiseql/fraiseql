package com.fraiseql.core;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.List;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Tests for GraphQL union type support in FraiseQL Java.
 * Unions are abstract types that can be one of several specified types.
 */
@DisplayName("GraphQL Unions")
public class UnionTest {

    private SchemaRegistry registry;

    @BeforeEach
    void setUp() {
        registry = SchemaRegistry.getInstance();
        registry.clear();
    }

    // =========================================================================
    // UNION REGISTRATION TESTS
    // =========================================================================

    @Test
    @DisplayName("Register union with member types")
    void testRegisterUnionWithMemberTypes() {
        List<String> memberTypes = new ArrayList<>();
        memberTypes.add("User");
        memberTypes.add("Post");
        memberTypes.add("Comment");

        registry.registerUnion("SearchResult", memberTypes, "Search result union");

        var unionInfo = registry.getUnion("SearchResult");
        assertTrue(unionInfo.isPresent());
        assertEquals("SearchResult", unionInfo.get().name);
        assertEquals(3, unionInfo.get().memberTypes.size());
        assertEquals("Search result union", unionInfo.get().description);
    }

    @Test
    @DisplayName("Register union without description")
    void testRegisterUnionWithoutDescription() {
        List<String> memberTypes = new ArrayList<>();
        memberTypes.add("User");
        memberTypes.add("Bot");

        registry.registerUnion("Actor", memberTypes, null);

        var unionInfo = registry.getUnion("Actor");
        assertTrue(unionInfo.isPresent());
        assertEquals("Actor", unionInfo.get().name);
        assertEquals(2, unionInfo.get().memberTypes.size());
    }

    @Test
    @DisplayName("Register union with two members")
    void testRegisterUnionWithTwoMembers() {
        List<String> memberTypes = new ArrayList<>();
        memberTypes.add("ImageNode");
        memberTypes.add("DocumentNode");

        registry.registerUnion("MediaNode", memberTypes, "Media content union");

        var unionInfo = registry.getUnion("MediaNode");
        assertTrue(unionInfo.isPresent());
        assertEquals(2, unionInfo.get().memberTypes.size());
    }

    @Test
    @DisplayName("Register multiple unions")
    void testRegisterMultipleUnions() {
        List<String> searchMembers = new ArrayList<>();
        searchMembers.add("User");
        searchMembers.add("Post");

        List<String> contentMembers = new ArrayList<>();
        contentMembers.add("Article");
        contentMembers.add("Video");

        registry.registerUnion("SearchResult", searchMembers, null);
        registry.registerUnion("Content", contentMembers, null);

        assertEquals(2, registry.getAllUnions().size());
        assertTrue(registry.getUnion("SearchResult").isPresent());
        assertTrue(registry.getUnion("Content").isPresent());
    }

    // =========================================================================
    // UNION MEMBER TESTS
    // =========================================================================

    @Test
    @DisplayName("Union with single member")
    void testUnionWithSingleMember() {
        List<String> memberTypes = new ArrayList<>();
        memberTypes.add("User");

        registry.registerUnion("SingleUnion", memberTypes, null);

        var unionInfo = registry.getUnion("SingleUnion");
        assertTrue(unionInfo.isPresent());
        assertEquals(1, unionInfo.get().memberTypes.size());
        assertTrue(unionInfo.get().memberTypes.contains("User"));
    }

    @Test
    @DisplayName("Union with many members")
    void testUnionWithManyMembers() {
        List<String> memberTypes = new ArrayList<>();
        for (int i = 1; i <= 5; i++) {
            memberTypes.add("Type" + i);
        }

        registry.registerUnion("ComplexUnion", memberTypes, null);

        var unionInfo = registry.getUnion("ComplexUnion");
        assertTrue(unionInfo.isPresent());
        assertEquals(5, unionInfo.get().memberTypes.size());
    }

    @Test
    @DisplayName("Union member types are preserved in order")
    void testUnionMemberTypesPreservedInOrder() {
        List<String> memberTypes = new ArrayList<>();
        memberTypes.add("A");
        memberTypes.add("B");
        memberTypes.add("C");

        registry.registerUnion("Ordered", memberTypes, null);

        var unionInfo = registry.getUnion("Ordered");
        assertTrue(unionInfo.isPresent());

        var members = unionInfo.get().memberTypes;
        assertEquals("A", members.get(0));
        assertEquals("B", members.get(1));
        assertEquals("C", members.get(2));
    }

    // =========================================================================
    // UNION USAGE PATTERNS
    // =========================================================================

    @Test
    @DisplayName("Pattern: Search result union")
    void testSearchResultUnionPattern() {
        List<String> searchMembers = new ArrayList<>();
        searchMembers.add("User");
        searchMembers.add("Post");
        searchMembers.add("Comment");

        registry.registerUnion("SearchResult", searchMembers, "Result of a search query");

        var unionInfo = registry.getUnion("SearchResult");
        assertTrue(unionInfo.isPresent());
        assertEquals(3, unionInfo.get().memberTypes.size());
        assertTrue(unionInfo.get().memberTypes.contains("User"));
        assertTrue(unionInfo.get().memberTypes.contains("Post"));
        assertTrue(unionInfo.get().memberTypes.contains("Comment"));
    }

    @Test
    @DisplayName("Pattern: Error result union")
    void testErrorResultUnionPattern() {
        List<String> errorMembers = new ArrayList<>();
        errorMembers.add("ValidationError");
        errorMembers.add("NotFoundError");
        errorMembers.add("PermissionError");

        registry.registerUnion("ErrorResult", errorMembers, "Possible error types");

        var unionInfo = registry.getUnion("ErrorResult");
        assertTrue(unionInfo.isPresent());
        assertEquals(3, unionInfo.get().memberTypes.size());
    }

    @Test
    @DisplayName("Pattern: Content node union")
    void testContentNodeUnionPattern() {
        List<String> contentMembers = new ArrayList<>();
        contentMembers.add("Article");
        contentMembers.add("Video");
        contentMembers.add("Podcast");
        contentMembers.add("Gallery");

        registry.registerUnion("ContentNode", contentMembers, "Any type of publishable content");

        var unionInfo = registry.getUnion("ContentNode");
        assertTrue(unionInfo.isPresent());
        assertEquals(4, unionInfo.get().memberTypes.size());
    }

    // =========================================================================
    // UNION IN QUERIES
    // =========================================================================

    @Test
    @DisplayName("Query returns union type")
    void testQueryReturnsUnionType() {
        // Register union
        List<String> memberTypes = new ArrayList<>();
        memberTypes.add("User");
        memberTypes.add("Post");
        registry.registerUnion("SearchResult", memberTypes, null);

        // Query can return union
        FraiseQL.query("search")
            .returnType("SearchResult")
            .returnsArray(true)
            .arg("query", "String")
            .register();

        var query = registry.getQuery("search");
        assertTrue(query.isPresent());
        assertEquals("[SearchResult]", query.get().returnType);
        assertEquals(1, query.get().arguments.size());
    }

    // =========================================================================
    // UNION WITH TYPES
    // =========================================================================

    @Test
    @DisplayName("Union references existing types")
    void testUnionReferencesExistingTypes() {
        // Register types first
        FraiseQL.registerTypes(User.class, Post.class);

        // Register union using those type names
        List<String> memberTypes = new ArrayList<>();
        memberTypes.add("User");
        memberTypes.add("Post");
        registry.registerUnion("Content", memberTypes, null);

        // Verify union is registered
        var unionInfo = registry.getUnion("Content");
        assertTrue(unionInfo.isPresent());

        // Verify referenced types exist
        assertTrue(registry.getType("User").isPresent());
        assertTrue(registry.getType("Post").isPresent());
    }

    // =========================================================================
    // CLEAR UNIONS TEST
    // =========================================================================

    @Test
    @DisplayName("Clear removes registered unions")
    void testClearRemovesUnions() {
        List<String> memberTypes = new ArrayList<>();
        memberTypes.add("Type1");
        registry.registerUnion("Test", memberTypes, null);

        assertTrue(registry.getUnion("Test").isPresent());

        registry.clear();

        assertFalse(registry.getUnion("Test").isPresent());
        assertEquals(0, registry.getAllUnions().size());
    }

    // =========================================================================
    // MUTATION RETURNS UNION
    // =========================================================================

    @Test
    @DisplayName("Mutation returns union type")
    void testMutationReturnsUnionType() {
        // Register union
        List<String> resultMembers = new ArrayList<>();
        resultMembers.add("SuccessResult");
        resultMembers.add("ErrorResult");
        registry.registerUnion("CreateResult", resultMembers, null);

        // Mutation returns union
        FraiseQL.mutation("createUser")
            .returnType("CreateResult")
            .arg("name", "String")
            .register();

        var mutation = registry.getMutation("createUser");
        assertTrue(mutation.isPresent());
        assertEquals("CreateResult", mutation.get().returnType);
    }

    // =========================================================================
    // TEST FIXTURES
    // =========================================================================

    @GraphQLType
    public static class User {
        @GraphQLField
        public String id;

        @GraphQLField
        public String name;
    }

    @GraphQLType
    public static class Post {
        @GraphQLField
        public String id;

        @GraphQLField
        public String title;
    }

    @GraphQLType
    public static class Comment {
        @GraphQLField
        public String id;

        @GraphQLField
        public String text;
    }

    @GraphQLUnion(members = {User.class, Post.class})
    public abstract static class SearchResult {
    }

    @GraphQLType
    public static class SuccessResult {
        @GraphQLField
        public String message;
    }

    @GraphQLType
    public static class ErrorResult {
        @GraphQLField
        public String error;
    }
}

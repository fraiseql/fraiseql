package com.fraiseql.core;

import com.fasterxml.jackson.databind.ObjectMapper;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * A class annotated with {@code @GraphQLUnion} registers a union through
 * {@link FraiseQL#registerUnion(Class)}: its name, its members' GraphQL names and its
 * description reach {@code schema.json}. A class the annotation cannot describe is refused,
 * and nothing is registered.
 */
@DisplayName("Union from an annotated class")
class UnionAnnotationTest {

    private static final ObjectMapper MAPPER = new ObjectMapper();

    @BeforeEach
    void setUp() {
        FraiseQL.clear();
    }

    @Test
    @DisplayName("an annotated union reaches schema.json with its members' GraphQL names")
    void anAnnotatedUnionIsExported() throws Exception {
        FraiseQL.registerUnion(Actor.class);
        FraiseQL.registerUnion(Hit.class);

        assertEquals(
            MAPPER.readTree(
                "[{\"name\":\"Actor\",\"member_types\":[\"Person\",\"Bot\"],"
                    + "\"description\":\"Who acted\"},"
                    + "{\"name\":\"SearchResult\",\"member_types\":[\"Person\"]}]"),
            FraiseQL.exportSchemaAsJson().get("unions"));
    }

    private static String refusal(Class<?> unionClass) {
        IllegalArgumentException refused =
            assertThrows(IllegalArgumentException.class, () -> FraiseQL.registerUnion(unionClass));
        assertTrue(FraiseQL.getRegistry().getUnion(unionClass.getSimpleName()).isEmpty());
        return refused.getMessage();
    }

    @Test
    @DisplayName("a class without @GraphQLUnion is refused")
    void anUnannotatedClassIsRefused() {
        assertEquals(
            "class " + NotAUnion.class.getName() + " is not annotated with @GraphQLUnion",
            refusal(NotAUnion.class));
    }

    @Test
    @DisplayName("a union with no members is refused")
    void aUnionWithNoMembersIsRefused() {
        assertEquals("Union Empty: @GraphQLUnion(members) is empty; a union needs a member",
            refusal(Empty.class));
    }

    @Test
    @DisplayName("a member that is not a @GraphQLType is refused, naming it")
    void aMemberThatIsNotATypeIsRefused() {
        assertEquals(
            "Union Stray: member " + String.class.getName()
                + " is not annotated with @GraphQLType; a union's members are object types",
            refusal(Stray.class));
    }

    @GraphQLType(name = "Person", sqlSource = "v_person")
    public static class Human {
        @GraphQLField(type = "ID")
        public String id;
    }

    @GraphQLType(sqlSource = "v_bot")
    public static class Bot {
        @GraphQLField(type = "ID")
        public String id;
    }

    @GraphQLUnion(members = {Human.class, Bot.class}, description = "Who acted")
    public abstract static class Actor {
    }

    @GraphQLUnion(name = "SearchResult", members = {Human.class})
    public abstract static class Hit {
    }

    @GraphQLUnion
    public abstract static class Empty {
    }

    @GraphQLUnion(members = {Human.class, String.class})
    public abstract static class Stray {
    }

    public abstract static class NotAUnion {
    }
}

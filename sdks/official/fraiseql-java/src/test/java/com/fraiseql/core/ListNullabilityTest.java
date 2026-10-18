package com.fraiseql.core;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.fasterxml.jackson.databind.JsonNode;
import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.Set;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.Test;

/**
 * A list position is non-null unless the author marks it nullable in Java's own way (#1586,
 * #1608): a primitive or an unannotated reference element is {@code !}; an element whose type
 * carries a type-use annotation named {@code Nullable}, or is an {@code Optional}, is not. The
 * same at every level of a nested list, and for an array as for a collection.
 */
class ListNullabilityTest {

    /** A type-use {@code @Nullable}, as JSpecify's: matched by its simple name. */
    @Target(ElementType.TYPE_USE)
    @Retention(RetentionPolicy.RUNTIME)
    @interface Nullable {}

    @GraphQLType(name = "Series")
    static class Series {
        @GraphQLField public int[] counts;
        @GraphQLField public String[] labels;
        @GraphQLField public @Nullable Double[] sparse;
        @GraphQLField public List<String> names;
        @GraphQLField public Set<Integer> codes;
        @GraphQLField public List<@Nullable Double> readings;
        @GraphQLField public List<Optional<Integer>> maybe;
        @GraphQLField public List<List<@Nullable Double>> grid;
        @GraphQLField public List<@Nullable List<Double>> rows;
    }

    @GraphQLType(name = "Raw")
    static class Raw {
        @SuppressWarnings("rawtypes")
        @GraphQLField public List untyped;
    }

    @AfterEach
    void clear() {
        FraiseQL.clear();
    }

    @Test
    void each_list_level_is_non_null_unless_marked_nullable() {
        Map<String, TypeConverter.GraphQLFieldInfo> fields = TypeConverter.extractFields(Series.class);
        Map<String, String> expected = Map.of(
            "counts", "[Int!]",
            "labels", "[String!]",
            "sparse", "[Float]",
            "names", "[String!]",
            "codes", "[Int!]",
            "readings", "[Float]",
            "maybe", "[Int]",
            "grid", "[[Float]!]",
            "rows", "[[Float!]]");
        expected.forEach((name, type) -> assertEquals(type, fields.get(name).type, name));
    }

    @Test
    void a_collection_without_an_element_type_is_refused() {
        RuntimeException refusal =
            assertThrows(RuntimeException.class, () -> TypeConverter.extractFields(Raw.class));
        assertTrue(refusal.getMessage().contains("Raw.untyped")
            && refusal.getMessage().contains("element type"), refusal.getMessage());
    }

    @Test
    void a_list_argument_keeps_its_list_type() {
        FraiseQL.query("seriesByIds")
            .returnType("Series")
            .returnsArray(true)
            .sqlSource("v_series")
            .arg("ids", "[ID!]!")
            .arg("tags", "[String]")
            .register();

        JsonNode schema = FraiseQL.exportSchemaAsJson();
        JsonNode args = null;
        for (JsonNode query : schema.get("queries")) {
            if ("seriesByIds".equals(query.get("name").asText())) {
                args = query.get("arguments");
            }
        }
        assertEquals("[ID!]", args.get(0).get("type").asText());
        assertEquals(false, args.get(0).get("nullable").asBoolean());
        assertEquals("[String]", args.get(1).get("type").asText());
        assertEquals(true, args.get(1).get("nullable").asBoolean());
    }
}

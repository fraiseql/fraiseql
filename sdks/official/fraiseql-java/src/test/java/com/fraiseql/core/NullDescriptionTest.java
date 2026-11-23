package com.fraiseql.core;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.node.ObjectNode;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import java.util.List;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.*;

/**
 * A description is optional on every registration: {@code null} exports as no description,
 * never as a crash of the whole export.
 */
@DisplayName("A null description exports as none")
public class NullDescriptionTest {

    private SchemaRegistry registry;

    @BeforeEach
    void setUp() {
        registry = SchemaRegistry.getInstance();
        registry.clear();
    }

    /** The entry named {@code name} in the exported {@code section}, after a full export. */
    private JsonNode exported(String section, String name) {
        ObjectNode schema = SchemaFormatter.formatSchema(registry);
        for (JsonNode entry : schema.get(section)) {
            if (name.equals(entry.get("name").asText())) {
                return entry;
            }
        }
        return fail("`" + name + "` is not in `" + section + "`: " + schema);
    }

    private static Map<String, TypeConverter.GraphQLFieldInfo> idField() {
        return Map.of("id", new TypeConverter.GraphQLFieldInfo("id", "ID", false, null));
    }

    @Test
    void aQueryWithANullDescriptionExports() {
        registry.registerQuery("things", "Thing", Map.of(), null);
        assertFalse(exported("queries", "things").has("description"));
    }

    @Test
    void aMutationWithANullDescriptionExports() {
        registry.registerMutation("makeThing", "Thing", Map.of(), null);
        assertFalse(exported("mutations", "makeThing").has("description"));
    }

    @Test
    void aSubscriptionWithANullDescriptionExports() {
        registry.registerSubscription("thingEvents", "Thing", Map.of(), null);
        assertFalse(exported("subscriptions", "thingEvents").has("description"));
    }

    @Test
    void anInterfaceWithANullDescriptionExports() {
        registry.registerInterface("Node", idField(), null);
        assertFalse(exported("interfaces", "Node").has("description"));
    }

    @Test
    void anInputTypeWithANullDescriptionExports() {
        registry.registerInputType("ThingInput", idField(), null);
        assertFalse(exported("input_types", "ThingInput").has("description"));
    }

    @Test
    void aFieldWithANullDescriptionExports() {
        registry.registerInterface("Node", idField(), "Anything with an id");
        JsonNode field = exported("interfaces", "Node").get("fields").get(0);
        assertFalse(field.has("description"), field::toString);
    }

    @Test
    void aTypeBuiltWithANullDescriptionKeepsNone() {
        var type = new SchemaRegistry.GraphQLTypeInfo(
            "Thing", Object.class, idField(), null, false, false, null, "thing");
        assertEquals("", type.description);
    }

    @Test
    void aUnionWithANullDescriptionKeepsNone() {
        registry.registerUnion("Found", List.of("Thing"), null);
        assertEquals("", registry.getUnion("Found").orElseThrow().description);
    }
}

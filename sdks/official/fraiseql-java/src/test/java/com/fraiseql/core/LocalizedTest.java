package com.fraiseql.core;

import com.fasterxml.jackson.databind.JsonNode;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

/**
 * {@code localized} authoring (#1527): a String field, input field or mutation argument
 * stored as a locale map. These tests follow the declaration to the exported JSON, and
 * refuse it on anything but a String, as the Python and TypeScript SDKs do.
 */
@DisplayName("Localized fields and arguments")
public class LocalizedTest {

    @GraphQLType
    public static class Product {
        @GraphQLField
        public String id;

        @GraphQLField(nullable = true, localized = true)
        public String name;

        @GraphQLField
        public String sku;
    }

    @GraphQLType
    public static class CreateProductInput {
        @GraphQLField(localized = true)
        public String name;
    }

    @GraphQLType
    public static class Priced {
        @GraphQLField(localized = true)
        public double price;
    }

    @BeforeEach
    void setUp() {
        FraiseQL.clear();
    }

    private static JsonNode named(JsonNode array, String name) {
        for (JsonNode item : array) {
            if (name.equals(item.get("name").asText())) {
                return item;
            }
        }
        fail(name + " is absent from " + array);
        return null;
    }

    @Test
    @DisplayName("a localized field and input field are exported as localized")
    void localizedFieldIsExported() {
        FraiseQL.registerType(Product.class);
        FraiseQL.getRegistry().registerInputType("CreateProductInput", CreateProductInput.class, "");

        JsonNode schema = SchemaFormatter.formatSchema(SchemaRegistry.getInstance());
        JsonNode fields = named(schema.get("types"), "Product").get("fields");
        assertTrue(named(fields, "name").path("localized").asBoolean(false), fields.toString());
        assertFalse(named(fields, "sku").has("localized"), "an unlocalized field carries no key");
        JsonNode inputs = named(schema.get("input_types"), "CreateProductInput").get("fields");
        assertTrue(named(inputs, "name").path("localized").asBoolean(false), inputs.toString());
    }

    @Test
    @DisplayName("a localized mutation argument is exported as a localized String")
    void localizedArgumentIsExported() {
        FraiseQL.mutation("createProduct")
            .returnType("Product")
            .sqlSource("fn_create_product")
            .arg("sku", "String!")
            .localizedArg("name", true)
            .localizedArg("tagline", false)
            .register();

        JsonNode schema = SchemaFormatter.formatSchema(SchemaRegistry.getInstance());
        JsonNode args = named(schema.get("mutations"), "createProduct").get("arguments");
        JsonNode name = named(args, "name");
        assertEquals("String", name.get("type").asText());
        assertTrue(name.get("nullable").asBoolean());
        assertTrue(name.path("localized").asBoolean(false), args.toString());
        JsonNode tagline = named(args, "tagline");
        assertFalse(tagline.get("nullable").asBoolean());
        assertTrue(tagline.path("localized").asBoolean(false), args.toString());
        assertFalse(named(args, "sku").has("localized"), "an unlocalized argument carries no key");
    }

    @Test
    @DisplayName("localized is refused on a field that is not a String")
    void localizedIsRefusedOnANonString() {
        FraiseQL.registerType(Priced.class);
        IllegalStateException refused = assertThrows(IllegalStateException.class,
            () -> SchemaFormatter.formatSchema(SchemaRegistry.getInstance()));
        assertTrue(refused.getMessage().contains("price")
            && refused.getMessage().contains("only a String can be localized"),
            refused.getMessage());
    }
}

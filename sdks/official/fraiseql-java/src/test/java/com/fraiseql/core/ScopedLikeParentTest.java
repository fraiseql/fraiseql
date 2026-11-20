package com.fraiseql.core;

import com.fasterxml.jackson.databind.JsonNode;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

/**
 * {@code scopedLikeParent} authoring: a field that embeds a scoped type is exported with
 * {@code scoped_like_parent: true}, only where it was declared, and an input field cannot
 * declare it.
 */
@DisplayName("Field scoped like its parent")
public class ScopedLikeParentTest {

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

    private static JsonNode field(JsonNode schema, String section, String type, String name) {
        return named(named(schema.get(section), type).get("fields"), name);
    }

    @Test
    @DisplayName("the declaration is exported on the field that makes it, and nowhere else")
    void scopedLikeParentIsExportedOnlyWhereDeclared() {
        FraiseQL.registerType(OrderLine.class);
        FraiseQL.registerType(Order.class);

        JsonNode schema = SchemaFormatter.formatSchema(SchemaRegistry.getInstance());
        JsonNode lines = field(schema, "types", "Order", "lines").get("scoped_like_parent");
        assertNotNull(lines, "the declaration is exported");
        assertTrue(lines.isBoolean() && lines.asBoolean());
        assertFalse(field(schema, "types", "Order", "firstLine").has("scoped_like_parent"));
        assertFalse(field(schema, "types", "Order", "id").has("scoped_like_parent"));
        assertFalse(field(schema, "types", "OrderLine", "id").has("scoped_like_parent"));
    }

    @Test
    @DisplayName("an input field cannot declare it")
    void anInputFieldCannotDeclareIt() {
        IllegalArgumentException refused = assertThrows(IllegalArgumentException.class,
            () -> FraiseQL.getRegistry().registerInputType("OrderInput", OrderInput.class, ""));
        assertTrue(refused.getMessage().contains("scoped_like_parent"), refused.getMessage());
        assertTrue(refused.getMessage().contains("line"), refused.getMessage());
        assertTrue(FraiseQL.getRegistry().getInputType("OrderInput").isEmpty());
    }

    @Test
    @DisplayName("the input types crud generates do not carry it")
    void generatedInputTypesDoNotCarryIt() {
        FraiseQL.registerType(OrderLine.class);
        FraiseQL.registerType(Shipment.class);

        JsonNode schema = SchemaFormatter.formatSchema(SchemaRegistry.getInstance());
        assertTrue(field(schema, "types", "Shipment", "line").get("scoped_like_parent").asBoolean());
        assertFalse(
            field(schema, "input_types", "CreateShipmentInput", "line").has("scoped_like_parent"));
        assertFalse(
            field(schema, "input_types", "UpdateShipmentInput", "line").has("scoped_like_parent"));
    }

    /** The scoped type the others embed. */
    @GraphQLType(name = "OrderLine", sqlSource = "v_order_line")
    public static class OrderLine {
        @GraphQLField(type = "ID")
        public String id;
    }

    /** Embeds {@code OrderLine} twice, declaring only one of the two scoped alike. */
    @GraphQLType(name = "Order", sqlSource = "v_order")
    public static class Order {
        @GraphQLField(type = "ID")
        public String id;

        @GraphQLField(type = "[OrderLine]", scopedLikeParent = true)
        public java.util.List<OrderLine> lines;

        @GraphQLField(type = "OrderLine")
        public OrderLine firstLine;
    }

    /** An input class making the declaration, which is refused. */
    @GraphQLType(name = "OrderInput")
    public static class OrderInput {
        @GraphQLField(type = "ID", scopedLikeParent = true)
        public String line;
    }

    /** A crud type, whose generated input types are built from its fields. */
    @GraphQLType(name = "Shipment", sqlSource = "v_shipment", crud = true)
    public static class Shipment {
        @GraphQLField(type = "ID")
        public String id;

        @GraphQLField(type = "OrderLine", scopedLikeParent = true)
        public OrderLine line;
    }
}

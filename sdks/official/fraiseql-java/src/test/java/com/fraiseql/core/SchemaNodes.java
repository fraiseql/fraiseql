package com.fraiseql.core;

import com.fasterxml.jackson.databind.JsonNode;

/**
 * Lookup helpers for the exported schema document.
 *
 * <p>Every top-level section is an <b>array</b> of objects carrying their own {@code name},
 * because that is the shape {@code fraiseql compile} reads. The exporter used to emit
 * objects keyed by name, so these tests indexed with {@code get("types").get("User")} and
 * passed against a document the compiler rejected outright (#851). Looking an element up
 * by its {@code name} keeps the tests readable without re-introducing that assumption.
 */
final class SchemaNodes {

    private SchemaNodes() {
    }

    /** The element of {@code section} whose {@code name} matches, or null. */
    static JsonNode byName(JsonNode schema, String section, String name) {
        JsonNode array = schema.get(section);
        if (array == null || !array.isArray()) {
            return null;
        }
        for (JsonNode element : array) {
            JsonNode elementName = element.get("name");
            if (elementName != null && name.equals(elementName.asText())) {
                return element;
            }
        }
        return null;
    }

    /** The field of {@code typeNode} whose {@code name} matches, or null. */
    static JsonNode field(JsonNode typeNode, String fieldName) {
        return byName(typeNode, "fields", fieldName);
    }

    /** The argument of an operation node whose {@code name} matches, or null. */
    static JsonNode argument(JsonNode operationNode, String argumentName) {
        return byName(operationNode, "arguments", argumentName);
    }

    /** Whether {@code section} contains an element named {@code name}. */
    static boolean has(JsonNode schema, String section, String name) {
        return byName(schema, section, name) != null;
    }
}

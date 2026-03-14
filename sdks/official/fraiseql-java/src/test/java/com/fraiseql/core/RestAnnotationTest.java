package com.fraiseql.core;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

/** Tests that REST annotation support is correctly emitted in schema JSON output. */
class RestAnnotationTest {

    private static final ObjectMapper MAPPER = new ObjectMapper();

    @BeforeEach
    void resetRegistry() {
        SchemaRegistry.getInstance().reset();
    }

    @Test
    void queryWithRestAnnotationEmitsRestBlock() throws Exception {
        new FraiseQL.QueryBuilder("getUser")
            .returnType("User")
            .sqlSource("v_user")
            .rest("/users/{id}", "GET")
            .register();

        String json = SchemaFormatter.toJson(SchemaRegistry.getInstance());
        JsonNode root = MAPPER.readTree(json);
        JsonNode restNode = root.path("queries").get(0).path("rest");

        assertFalse(restNode.isMissingNode(), "rest block should be present");
        assertEquals("/users/{id}", restNode.path("path").asText());
        assertEquals("GET", restNode.path("method").asText());
    }

    @Test
    void mutationWithRestAnnotationEmitsRestBlock() throws Exception {
        new FraiseQL.MutationBuilder("createUser")
            .returnType("User")
            .sqlSource("fn_create_user")
            .operation("CREATE")
            .rest("/users", "POST")
            .register();

        String json = SchemaFormatter.toJson(SchemaRegistry.getInstance());
        JsonNode root = MAPPER.readTree(json);
        JsonNode restNode = root.path("mutations").get(0).path("rest");

        assertFalse(restNode.isMissingNode(), "rest block should be present");
        assertEquals("/users", restNode.path("path").asText());
        assertEquals("POST", restNode.path("method").asText());
    }

    @Test
    void queryWithoutRestAnnotationOmitsRestBlock() throws Exception {
        new FraiseQL.QueryBuilder("getUsers")
            .returnType("User")
            .returnsList(true)
            .sqlSource("v_users")
            .register();

        String json = SchemaFormatter.toJson(SchemaRegistry.getInstance());
        JsonNode root = MAPPER.readTree(json);
        JsonNode restNode = root.path("queries").get(0).path("rest");

        assertTrue(restNode.isMissingNode(), "rest block should be absent when not set");
    }
}

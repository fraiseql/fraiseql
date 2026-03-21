package com.fraiseql.core;

import com.fasterxml.jackson.databind.JsonNode;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

class RestAnnotationTest {

    @BeforeEach
    void setUp() {
        FraiseQL.clear();
    }

    @Test
    void queryRestPathAndMethod() {
        FraiseQL.query("users")
            .returnType("User")
            .returnsArray(true)
            .sqlSource("v_user")
            .restPath("/api/users")
            .restMethod("GET")
            .register();

        JsonNode schema = FraiseQL.exportSchemaAsJson();
        JsonNode rest = schema.get("queries").get("users").get("rest");
        assertNotNull(rest);
        assertEquals("/api/users", rest.get("path").asText());
        assertEquals("GET", rest.get("method").asText());
    }

    @Test
    void queryRestDefaultsToGet() {
        FraiseQL.query("users")
            .returnType("User")
            .returnsArray(true)
            .sqlSource("v_user")
            .restPath("/api/users")
            .register();

        JsonNode schema = FraiseQL.exportSchemaAsJson();
        JsonNode rest = schema.get("queries").get("users").get("rest");
        assertNotNull(rest);
        assertEquals("GET", rest.get("method").asText());
    }

    @Test
    void queryWithoutRestOmitsBlock() {
        FraiseQL.query("users")
            .returnType("User")
            .returnsArray(true)
            .sqlSource("v_user")
            .register();

        JsonNode schema = FraiseQL.exportSchemaAsJson();
        JsonNode rest = schema.get("queries").get("users").get("rest");
        assertNull(rest);
    }

    @Test
    void mutationRestPathAndMethod() {
        FraiseQL.mutation("createUser")
            .returnType("User")
            .sqlSource("fn_create_user")
            .operation("insert")
            .restPath("/api/users")
            .restMethod("POST")
            .register();

        JsonNode schema = FraiseQL.exportSchemaAsJson();
        JsonNode rest = schema.get("mutations").get("createUser").get("rest");
        assertNotNull(rest);
        assertEquals("/api/users", rest.get("path").asText());
        assertEquals("POST", rest.get("method").asText());
    }

    @Test
    void mutationRestDefaultsToPost() {
        FraiseQL.mutation("createUser")
            .returnType("User")
            .sqlSource("fn_create_user")
            .operation("insert")
            .restPath("/api/users")
            .register();

        JsonNode schema = FraiseQL.exportSchemaAsJson();
        JsonNode rest = schema.get("mutations").get("createUser").get("rest");
        assertNotNull(rest);
        assertEquals("POST", rest.get("method").asText());
    }

    @Test
    void mutationRestDeleteMethod() {
        FraiseQL.mutation("deleteUser")
            .returnType("User")
            .sqlSource("fn_delete_user")
            .operation("delete")
            .restPath("/api/users/{id}")
            .restMethod("DELETE")
            .register();

        JsonNode schema = FraiseQL.exportSchemaAsJson();
        JsonNode rest = schema.get("mutations").get("deleteUser").get("rest");
        assertNotNull(rest);
        assertEquals("/api/users/{id}", rest.get("path").asText());
        assertEquals("DELETE", rest.get("method").asText());
    }

    @Test
    void mutationWithoutRestOmitsBlock() {
        FraiseQL.mutation("createUser")
            .returnType("User")
            .sqlSource("fn_create_user")
            .operation("insert")
            .register();

        JsonNode schema = FraiseQL.exportSchemaAsJson();
        JsonNode rest = schema.get("mutations").get("createUser").get("rest");
        assertNull(rest);
    }

    @Test
    void restMethodCaseInsensitive() {
        FraiseQL.query("users")
            .returnType("User")
            .returnsArray(true)
            .sqlSource("v_user")
            .restPath("/api/users")
            .restMethod("get")
            .register();

        JsonNode schema = FraiseQL.exportSchemaAsJson();
        JsonNode rest = schema.get("queries").get("users").get("rest");
        assertEquals("GET", rest.get("method").asText());
    }
}

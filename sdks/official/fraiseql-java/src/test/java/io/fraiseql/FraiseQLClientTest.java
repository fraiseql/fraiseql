package io.fraiseql;

import okhttp3.mockwebserver.MockResponse;
import okhttp3.mockwebserver.MockWebServer;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

import java.io.IOException;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Unit tests for {@link FraiseQLClient} against a local {@link MockWebServer}.
 */
class FraiseQLClientTest {

    private MockWebServer server;
    private String baseUrl;

    @BeforeEach
    void setUp() throws IOException {
        server = new MockWebServer();
        server.start();
        baseUrl = server.url("/graphql").toString();
    }

    @AfterEach
    void tearDown() throws IOException {
        server.shutdown();
    }

    // -------------------------------------------------------------------------
    // Helper
    // -------------------------------------------------------------------------

    private FraiseQLClient client() {
        return FraiseQLClient.builder(baseUrl)
                .timeout(java.time.Duration.ofSeconds(5))
                .build();
    }

    // -------------------------------------------------------------------------
    // Tests
    // -------------------------------------------------------------------------

    @Test
    void queryReturnsDataOnSuccess() throws Exception {
        server.enqueue(new MockResponse()
                .setResponseCode(200)
                .setHeader("Content-Type", "application/json")
                .setBody("{\"data\":{\"id\":\"42\",\"name\":\"Alice\"}}"));

        try (FraiseQLClient client = client()) {
            @SuppressWarnings("unchecked")
            Map<String, Object> result = client.query("{ user { id name } }", Map.class);

            assertNotNull(result);
            assertEquals("42", result.get("id"));
            assertEquals("Alice", result.get("name"));
        }
    }

    @Test
    void throwsGraphQLExceptionWhenErrorsPresent() {
        server.enqueue(new MockResponse()
                .setResponseCode(200)
                .setHeader("Content-Type", "application/json")
                .setBody("{\"errors\":[{\"message\":\"Not found\"}],\"data\":null}"));

        try (FraiseQLClient client = client()) {
            GraphQLException ex = assertThrows(GraphQLException.class,
                    () -> client.query("{ missing }", Map.class));

            assertEquals(1, ex.getErrors().size());
            assertEquals("Not found", ex.getErrors().get(0).getMessage());
        }
    }

    @Test
    void treatsNullErrorsAsSuccess() {
        // Cross-SDK invariant: absent/null "errors" key must NOT throw
        server.enqueue(new MockResponse()
                .setResponseCode(200)
                .setHeader("Content-Type", "application/json")
                .setBody("{\"data\":{\"ok\":true}}"));

        try (FraiseQLClient client = client()) {
            @SuppressWarnings("unchecked")
            Map<String, Object> result = client.query("mutation { doThing }", Map.class);
            assertNotNull(result);
            assertEquals(Boolean.TRUE, result.get("ok"));
        }
    }

    @Test
    void throwsAuthenticationExceptionOn401() {
        server.enqueue(new MockResponse().setResponseCode(401));

        try (FraiseQLClient client = client()) {
            AuthenticationException ex = assertThrows(AuthenticationException.class,
                    () -> client.query("{ secret }", Map.class));

            assertEquals(401, ex.getStatusCode());
        }
    }

    @Test
    void throwsNetworkExceptionOnConnectionFailure() throws IOException {
        // Shut the server down so connection is refused
        server.shutdown();

        FraiseQLClient client = FraiseQLClient.builder("http://127.0.0.1:1")
                .timeout(java.time.Duration.ofSeconds(2))
                .build();

        assertThrows(NetworkException.class, () -> client.query("{ ping }", Map.class));
    }
}

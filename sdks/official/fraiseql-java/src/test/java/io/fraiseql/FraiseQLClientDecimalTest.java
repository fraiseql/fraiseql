package io.fraiseql;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import okhttp3.mockwebserver.MockResponse;
import okhttp3.mockwebserver.MockWebServer;
import okhttp3.mockwebserver.RecordedRequest;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.CsvSource;

import java.io.IOException;
import java.math.BigDecimal;
import java.time.Duration;
import java.util.List;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * A {@code Decimal} travels as the string of its exact digits.
 *
 * <p>The server takes a {@code Decimal} argument as a string such as {@code "1.50"}, or an
 * exact integer, and refuses a number with a fraction or an exponent. The client writes a
 * {@link BigDecimal} variable as {@link BigDecimal#toPlainString()} in a JSON string, and
 * reads a response's string into a {@link BigDecimal} with its scale.
 */
class FraiseQLClientDecimalTest {

    private static final ObjectMapper JSON = new ObjectMapper();

    private MockWebServer server;

    /** A response type with a {@code Decimal} field. */
    public static class Quote {
        public BigDecimal amount;
    }

    @BeforeEach
    void setUp() throws IOException {
        server = new MockWebServer();
        server.start();
    }

    @AfterEach
    void tearDown() throws IOException {
        server.shutdown();
    }

    private FraiseQLClient client() {
        return FraiseQLClient.builder(server.url("/graphql").toString())
                .timeout(Duration.ofSeconds(5))
                .build();
    }

    private void answer(String data) {
        server.enqueue(new MockResponse()
                .setResponseCode(200)
                .setHeader("Content-Type", "application/json")
                .setBody("{\"data\":" + data + "}"));
    }

    /** The {@code variables} object of the request the client sent, as JSON. */
    private JsonNode sentVariables(Map<String, Object> variables) throws Exception {
        answer("{}");
        try (FraiseQLClient client = client()) {
            client.query("query($a: Decimal) { echo(a: $a) }", variables, Map.class);
        }
        RecordedRequest request = server.takeRequest();
        return JSON.readTree(request.getBody().readUtf8()).get("variables");
    }

    @ParameterizedTest
    @CsvSource({
        "1.50, 1.50",
        "100.00, 100.00",
        "12345678901234567890.12, 12345678901234567890.12",
        "123456789012345678901234567890, 123456789012345678901234567890",
        "-0.00, 0.00",
        "1E+3, 1000",
        "1.5E-3, 0.0015",
    })
    void aBigDecimalVariableIsSentAsAStringOfItsPlainDigits(String written, String wire)
            throws Exception {
        JsonNode sent = sentVariables(Map.of("a", new BigDecimal(written)));
        assertTrue(sent.get("a").isTextual(), "a Decimal must travel as a JSON string: " + sent);
        assertEquals(wire, sent.get("a").textValue());
        assertEquals(0, new BigDecimal(written).compareTo(new BigDecimal(sent.get("a").textValue())));
    }

    @Test
    void aBigDecimalNestedInAnInputObjectIsSentAsAString() throws Exception {
        JsonNode sent = sentVariables(Map.of(
                "a", Map.of("amounts", List.of(new BigDecimal("1.50"), new BigDecimal("2.0")))));
        assertEquals("[\"1.50\",\"2.0\"]", sent.get("a").get("amounts").toString());
    }

    @Test
    void otherNumbersAreStillNumbers() throws Exception {
        JsonNode sent = sentVariables(Map.of("i", 42, "f", 0.5));
        assertTrue(sent.get("i").isIntegralNumber());
        assertTrue(sent.get("f").isFloatingPointNumber());
    }

    @ParameterizedTest
    @CsvSource({
        "1.50, 2",
        "100.00, 2",
        "12345678901234567890.12, 2",
        "123456789012345678901234567890, 0",
    })
    void aDecimalResponseIsReadWithItsDigitsAndScale(String digits, int scale) throws Exception {
        answer("{\"amount\":\"" + digits + "\"}");
        Quote quote;
        try (FraiseQLClient client = client()) {
            quote = client.query("{ quote { amount } }", Quote.class);
        }
        assertEquals(digits, quote.amount.toPlainString());
        assertEquals(scale, quote.amount.scale());
    }
}

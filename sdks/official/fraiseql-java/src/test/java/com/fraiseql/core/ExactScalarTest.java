package com.fraiseql.core;

import com.fasterxml.jackson.databind.JsonNode;
import java.math.BigDecimal;
import java.math.BigInteger;
import java.util.List;
import java.util.UUID;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

/**
 * A {@code BigDecimal} or {@code BigInteger} is the engine's exact {@code Decimal}: as
 * {@code Float} it loses digits. A {@code UUID} is the engine's {@code UUID}: as
 * {@code String} it loses its type. An {@code id} typed as a UUID is {@code ID}.
 */
@DisplayName("Decimal and UUID are their exact scalars")
public class ExactScalarTest {

    @BeforeEach
    void setUp() {
        FraiseQL.clear();
    }

    @Test
    @DisplayName("BigDecimal and BigInteger map to Decimal, UUID to UUID")
    void nativeTypesMapToTheExactScalars() {
        assertEquals("Decimal", TypeConverter.javaToGraphQL(BigDecimal.class));
        assertEquals("Decimal", TypeConverter.javaToGraphQL(BigInteger.class));
        assertEquals("[Decimal!]", TypeConverter.javaToGraphQL(BigDecimal[].class));
        assertEquals("UUID", TypeConverter.javaToGraphQL(UUID.class));
        assertEquals("Float", TypeConverter.javaToGraphQL(double.class));
    }

    @Test
    @DisplayName("a type's fields export the exact scalars, and a UUID id is ID")
    void fieldsExportTheExactScalars() {
        FraiseQL.registerType(Quote.class);

        JsonNode schema = SchemaFormatter.formatSchema(SchemaRegistry.getInstance());
        JsonNode quote = schema.get("types").get(0);
        assertEquals("ID", SchemaNodes.field(quote, "id").get("type").asText());
        assertEquals("Decimal", SchemaNodes.field(quote, "amount").get("type").asText());
        assertEquals("[Decimal!]", SchemaNodes.field(quote, "deltas").get("type").asText());
        assertEquals("UUID", SchemaNodes.field(quote, "issuerId").get("type").asText());
    }

    @GraphQLType(name = "Quote", sqlSource = "v_quote")
    public static class Quote {
        @GraphQLField
        public UUID id;

        @GraphQLField
        public BigDecimal amount;

        @GraphQLField
        public List<BigDecimal> deltas;

        @GraphQLField
        public UUID issuerId;
    }
}

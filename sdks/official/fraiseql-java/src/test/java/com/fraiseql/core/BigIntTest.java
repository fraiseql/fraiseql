package com.fraiseql.core;

import com.fasterxml.jackson.databind.JsonNode;
import java.util.List;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

/**
 * A {@code long} is a signed 64-bit integer, the engine's {@code BigInt}. Published as
 * {@code Int}, which is 32 bits, a value past that range is refused at read time.
 */
@DisplayName("64-bit integers are BigInt")
public class BigIntTest {

    @BeforeEach
    void setUp() {
        FraiseQL.clear();
    }

    @Test
    @DisplayName("long and Long map to BigInt; the narrower integers stay Int")
    void longIsBigInt() {
        assertEquals("BigInt", TypeConverter.javaToGraphQL(long.class));
        assertEquals("BigInt", TypeConverter.javaToGraphQL(Long.class));
        assertEquals("[BigInt!]", TypeConverter.javaToGraphQL(long[].class));
        assertEquals("Int", TypeConverter.javaToGraphQL(int.class));
        assertEquals("Int", TypeConverter.javaToGraphQL(Integer.class));
        assertEquals("Int", TypeConverter.javaToGraphQL(short.class));
    }

    @Test
    @DisplayName("a type's long fields are exported as BigInt, a list's items included")
    void longFieldsAreExportedAsBigInt() {
        FraiseQL.registerType(Ledger.class);

        JsonNode schema = SchemaFormatter.formatSchema(SchemaRegistry.getInstance());
        JsonNode ledger = schema.get("types").get(0);
        assertEquals("BigInt", SchemaNodes.field(ledger, "amount").get("type").asText());
        assertEquals("BigInt", SchemaNodes.field(ledger, "balance").get("type").asText());
        assertEquals("[BigInt!]", SchemaNodes.field(ledger, "transferIds").get("type").asText());
        assertEquals("[BigInt]", SchemaNodes.field(ledger, "holds").get("type").asText());
    }

    @Test
    @DisplayName("BigInt is a known scalar name")
    void bigIntIsAKnownScalar() {
        assertEquals("BigInt", Scalars.BIG_INT);
        assertTrue(Scalars.SCALAR_NAMES.contains("BigInt"));
    }

    /** A type-use {@code @Nullable}, as JSpecify's: matched by its simple name. */
    @java.lang.annotation.Target(java.lang.annotation.ElementType.TYPE_USE)
    @java.lang.annotation.Retention(java.lang.annotation.RetentionPolicy.RUNTIME)
    @interface Nullable {}

    @GraphQLType(name = "Ledger", sqlSource = "v_ledger")
    public static class Ledger {
        @GraphQLField
        public long amount;

        @GraphQLField(nullable = true)
        public Long balance;

        @GraphQLField
        public List<Long> transferIds;

        @GraphQLField
        public List<@Nullable Long> holds;
    }
}

package com.fraiseql.core;

import com.fasterxml.jackson.databind.JsonNode;
import java.time.LocalDateTime;
import java.util.List;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

/**
 * A {@code java.time.LocalDateTime} has no offset: it is the engine's {@code LocalDateTime},
 * a PostgreSQL {@code timestamp}. Published as {@code DateTime}, its value read as an instant
 * it never was. The instants ({@code Instant}, {@code OffsetDateTime},
 * {@code java.sql.Timestamp}, ...) stay {@code DateTime}.
 */
@DisplayName("LocalDateTime is LocalDateTime")
public class LocalDateTimeTest {

    @BeforeEach
    void setUp() {
        FraiseQL.clear();
    }

    @Test
    @DisplayName("LocalDateTime maps to LocalDateTime; the instants stay DateTime")
    void localDateTimeIsLocalDateTime() {
        assertEquals("LocalDateTime", TypeConverter.javaToGraphQL(LocalDateTime.class));
        assertEquals("[LocalDateTime!]", TypeConverter.javaToGraphQL(LocalDateTime[].class));
        assertEquals("DateTime", TypeConverter.javaToGraphQL(java.time.Instant.class));
        assertEquals("DateTime", TypeConverter.javaToGraphQL(java.sql.Timestamp.class));
    }

    @Test
    @DisplayName("a type's LocalDateTime fields are exported as LocalDateTime")
    void localDateTimeFieldsAreExported() {
        FraiseQL.registerType(Shift.class);

        JsonNode schema = SchemaFormatter.formatSchema(SchemaRegistry.getInstance());
        JsonNode shift = schema.get("types").get(0);
        assertEquals("LocalDateTime", SchemaNodes.field(shift, "startsAt").get("type").asText());
        assertEquals("[LocalDateTime!]", SchemaNodes.field(shift, "breaksAt").get("type").asText());
        assertEquals("DateTime", SchemaNodes.field(shift, "loggedAt").get("type").asText());
    }

    @Test
    @DisplayName("LocalDateTime is a known scalar name")
    void localDateTimeIsAKnownScalar() {
        assertEquals("LocalDateTime", Scalars.LOCAL_DATE_TIME);
        assertTrue(Scalars.SCALAR_NAMES.contains("LocalDateTime"));
    }

    @GraphQLType(name = "Shift", sqlSource = "v_shift")
    public static class Shift {
        @GraphQLField
        public LocalDateTime startsAt;

        @GraphQLField
        public List<LocalDateTime> breaksAt;

        @GraphQLField
        public java.time.Instant loggedAt;
    }
}

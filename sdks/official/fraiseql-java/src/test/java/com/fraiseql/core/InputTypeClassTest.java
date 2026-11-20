package com.fraiseql.core;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Registering an input type from a class: the class's fields are read only when it is
 * annotated, so a class that is not, or that declares no field, is refused rather than
 * registered as an input type with no fields.
 */
@DisplayName("Input type from a class")
public class InputTypeClassTest {

    @BeforeEach
    void setUp() {
        FraiseQL.clear();
    }

    @Test
    @DisplayName("a class without @GraphQLType is refused, and nothing is registered")
    void anUnannotatedClassIsRefused() {
        IllegalArgumentException refusal = assertThrows(IllegalArgumentException.class,
            () -> FraiseQL.getRegistry().registerInputType("OrderInput", Unannotated.class, ""));
        assertEquals(
            "Input type OrderInput: class " + Unannotated.class.getName()
                + " is not annotated with @GraphQLType, so its fields cannot be read; "
                + "annotate it with @GraphQLType",
            refusal.getMessage());
        assertTrue(FraiseQL.getRegistry().getInputType("OrderInput").isEmpty());
    }

    @Test
    @DisplayName("a class carrying only @GraphQLInput is refused the same way")
    void aGraphQLInputOnlyClassIsRefused() {
        IllegalArgumentException refusal = assertThrows(IllegalArgumentException.class,
            () -> FraiseQL.getRegistry().registerInputType("OrderInput", InputOnly.class, ""));
        assertTrue(refusal.getMessage().contains(InputOnly.class.getName()), refusal.getMessage());
        assertTrue(refusal.getMessage().contains("@GraphQLType"), refusal.getMessage());
        assertTrue(FraiseQL.getRegistry().getInputType("OrderInput").isEmpty());
    }

    @Test
    @DisplayName("an annotated class declaring no field is refused, and nothing is registered")
    void anAnnotatedClassWithoutFieldsIsRefused() {
        IllegalArgumentException refusal = assertThrows(IllegalArgumentException.class,
            () -> FraiseQL.getRegistry().registerInputType("OrderInput", Fieldless.class, ""));
        assertEquals(
            "Input type OrderInput: class " + Fieldless.class.getName()
                + " declares no @GraphQLField field; an input type needs at least one field",
            refusal.getMessage());
        assertTrue(FraiseQL.getRegistry().getInputType("OrderInput").isEmpty());
    }

    @Test
    @DisplayName("an annotated class registers with its fields")
    void anAnnotatedClassRegistersWithItsFields() {
        FraiseQL.getRegistry().registerInputType("OrderInput", Annotated.class, "An order");

        SchemaRegistry.InputTypeInfo registered =
            FraiseQL.getRegistry().getInputType("OrderInput").orElseThrow();
        assertEquals(java.util.List.of("id", "note"),
            java.util.List.copyOf(registered.fields.keySet()));
        assertEquals("An order", registered.description);
    }

    /** Carries {@code @GraphQLField} but no class annotation. */
    public static class Unannotated {
        @GraphQLField
        public String note;
    }

    /** Carries the input marker, which field extraction does not read. */
    @GraphQLInput
    public static class InputOnly {
        @GraphQLField
        public String note;
    }

    /** Annotated, with a member that is not a declared field. */
    @GraphQLType
    public static class Fieldless {
        public String note;
    }

    /** Annotated, with two declared fields. */
    @GraphQLType
    public static class Annotated {
        @GraphQLField(type = "ID")
        public String id;

        @GraphQLField(nullable = true)
        public String note;
    }
}

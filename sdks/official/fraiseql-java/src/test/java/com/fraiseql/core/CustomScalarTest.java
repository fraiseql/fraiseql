package com.fraiseql.core;

import static org.junit.jupiter.api.Assertions.*;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.DisplayName;

/**
 * Comprehensive test suite for custom scalar support.
 *
 * <p>Tests cover registration, validation, error handling, and integration with schema.
 */
@DisplayName("Custom Scalar Support")
class CustomScalarTest {

    /**
     * Test Email scalar implementation.
     */
    @Scalar
    static class Email extends CustomScalar {
        @Override
        public String getName() {
            return "Email";
        }

        @Override
        public Object serialize(Object value) {
            return String.valueOf(value);
        }

        @Override
        public Object parseValue(Object value) {
            String str = String.valueOf(value).trim();
            if (!str.contains("@")) {
                throw new IllegalArgumentException("Invalid email format: " + str);
            }
            return str;
        }

        @Override
        public Object parseLiteral(Object ast) {
            if (ast instanceof java.util.Map) {
                java.util.Map<String, Object> astMap = (java.util.Map<String, Object>) ast;
                if (astMap.containsKey("value")) {
                    return parseValue(astMap.get("value"));
                }
            }
            throw new IllegalArgumentException("Email literal must be string");
        }
    }

    /**
     * Test Phone scalar implementation.
     */
    @Scalar
    static class Phone extends CustomScalar {
        @Override
        public String getName() {
            return "Phone";
        }

        @Override
        public Object serialize(Object value) {
            return String.valueOf(value);
        }

        @Override
        public Object parseValue(Object value) {
            String str = String.valueOf(value).trim();
            if (!str.startsWith("+")) {
                throw new IllegalArgumentException("Phone must start with +");
            }
            String digits = str.substring(1);
            if (!digits.matches("^\\d+$")) {
                throw new IllegalArgumentException("Phone must contain only digits after +");
            }
            if (digits.length() < 10 || digits.length() > 14) {
                throw new IllegalArgumentException(
                    String.format("Phone must be 10-14 digits, got %d", digits.length()));
            }
            return str;
        }

        @Override
        public Object parseLiteral(Object ast) {
            if (ast instanceof java.util.Map) {
                java.util.Map<String, Object> astMap = (java.util.Map<String, Object>) ast;
                if (astMap.containsKey("value")) {
                    return parseValue(astMap.get("value"));
                }
            }
            throw new IllegalArgumentException("Phone literal must be string");
        }
    }

    @BeforeEach
    void setUp() {
        ScalarProcessor.clearAll();
    }

    // ========================================================================
    // Scalar Registration Tests
    // ========================================================================

    @Test
    @DisplayName("Registers scalar globally")
    void testScalarRegistersGlobally() {
        // Before registration
        assertTrue(ScalarRegistry.getInstance().getCustomScalars().isEmpty());

        // Register Email
        ScalarProcessor.register(Email.class);

        // After registration
        assertTrue(ScalarRegistry.getInstance().hasScalar("Email"));
        assertEquals(Email.class, ScalarRegistry.getInstance().getScalar("Email"));
    }

    @Test
    @DisplayName("Validates class has @Scalar annotation")
    void testValidatesScalarAnnotation() {
        class NotAnnotated extends CustomScalar {
            @Override
            public String getName() {
                return "NotAnnotated";
            }

            @Override
            public Object serialize(Object value) {
                return value;
            }

            @Override
            public Object parseValue(Object value) {
                return value;
            }

            @Override
            public Object parseLiteral(Object ast) {
                return ast;
            }
        }

        assertThrows(IllegalArgumentException.class, () -> ScalarProcessor.register(NotAnnotated.class));
    }

    @Test
    @DisplayName("Validates scalar name is non-empty")
    void testValidatesScalarName() {
        @Scalar
        class NoName extends CustomScalar {
            @Override
            public String getName() {
                return "";
            }

            @Override
            public Object serialize(Object value) {
                return value;
            }

            @Override
            public Object parseValue(Object value) {
                return value;
            }

            @Override
            public Object parseLiteral(Object ast) {
                return ast;
            }
        }

        assertThrows(IllegalArgumentException.class, () -> ScalarProcessor.register(NoName.class));
    }

    @Test
    @DisplayName("Prevents duplicate scalar names")
    void testPreventsDuplicateNames() {
        ScalarProcessor.register(Email.class);

        @Scalar
        class AnotherEmail extends CustomScalar {
            @Override
            public String getName() {
                return "Email";
            }

            @Override
            public Object serialize(Object value) {
                return value;
            }

            @Override
            public Object parseValue(Object value) {
                return value;
            }

            @Override
            public Object parseLiteral(Object ast) {
                return ast;
            }
        }

        assertThrows(IllegalArgumentException.class, () -> ScalarProcessor.register(AnotherEmail.class));
    }

    @Test
    @DisplayName("Registers multiple scalars")
    void testRegistersMultipleScalars() {
        ScalarProcessor.registerAll(Email.class, Phone.class);

        assertTrue(ScalarRegistry.getInstance().hasScalar("Email"));
        assertTrue(ScalarRegistry.getInstance().hasScalar("Phone"));
    }

    // ========================================================================
    // Validation Engine Tests
    // ========================================================================

    @Test
    @DisplayName("Validates parseValue successfully")
    void testValidateParseValueSuccess() {
        ScalarProcessor.register(Email.class);
        Object result = ScalarValidator.validate(Email.class, "user@example.com", "parseValue");
        assertEquals("user@example.com", result);
    }

    @Test
    @DisplayName("Rejects invalid values")
    void testValidateParseValueFailure() {
        ScalarProcessor.register(Email.class);
        assertThrows(
            ScalarValidationError.class,
            () -> ScalarValidator.validate(Email.class, "invalid-email", "parseValue"));
    }

    @Test
    @DisplayName("Serializes values")
    void testValidateSerialize() {
        ScalarProcessor.register(Email.class);
        Object result = ScalarValidator.validate(Email.class, "user@example.com", "serialize");
        assertEquals("user@example.com", result);
    }

    @Test
    @DisplayName("Parses literals")
    void testValidateParseLiteral() {
        ScalarProcessor.register(Email.class);
        java.util.Map<String, Object> ast = java.util.Collections.singletonMap("value", "user@example.com");
        Object result = ScalarValidator.validate(Email.class, ast, "parseLiteral");
        assertEquals("user@example.com", result);
    }

    @Test
    @DisplayName("Handles multiple scalar types")
    void testValidateMultipleScalars() {
        ScalarProcessor.registerAll(Email.class, Phone.class);

        // Email validation
        Object email = ScalarValidator.validate(Email.class, "test@test.com");
        assertEquals("test@test.com", email);

        // Phone validation
        Object phone = ScalarValidator.validate(Phone.class, "+12025551234");
        assertEquals("+12025551234", phone);

        // Both fail appropriately
        assertThrows(ScalarValidationError.class, () -> ScalarValidator.validate(Email.class, "notanemail"));
        assertThrows(ScalarValidationError.class, () -> ScalarValidator.validate(Phone.class, "invalid"));
    }

    @Test
    @DisplayName("Throws on invalid context")
    void testValidateInvalidContext() {
        ScalarProcessor.register(Email.class);
        assertThrows(
            IllegalArgumentException.class,
            () -> ScalarValidator.validate(Email.class, "test@test.com", "invalidContext"));
    }

    @Test
    @DisplayName("Uses default context parseValue")
    void testValidateDefaultContext() {
        ScalarProcessor.register(Email.class);
        Object result = ScalarValidator.validate(Email.class, "test@test.com");
        assertEquals("test@test.com", result);
    }

    // ========================================================================
    // Error Message Tests
    // ========================================================================

    @Test
    @DisplayName("Error includes scalar name")
    void testErrorIncludesScalarName() {
        ScalarProcessor.register(Email.class);
        ScalarValidationError error =
            assertThrows(
                ScalarValidationError.class,
                () -> ScalarValidator.validate(Email.class, "notanemail"));
        assertTrue(error.getMessage().contains("Email"));
    }

    @Test
    @DisplayName("Error includes context")
    void testErrorIncludesContext() {
        ScalarProcessor.register(Email.class);
        ScalarValidationError error =
            assertThrows(
                ScalarValidationError.class,
                () -> ScalarValidator.validate(Email.class, "notanemail", "parseValue"));
        assertTrue(error.getMessage().contains("parseValue"));
    }

    @Test
    @DisplayName("Error includes original message")
    void testErrorIncludesMessage() {
        ScalarProcessor.register(Email.class);
        ScalarValidationError error =
            assertThrows(
                ScalarValidationError.class,
                () -> ScalarValidator.validate(Email.class, "notanemail"));
        assertTrue(error.getMessage().contains("Invalid email format"));
    }

    @Test
    @DisplayName("Error getters work")
    void testErrorGetters() {
        ScalarProcessor.register(Email.class);
        ScalarValidationError error =
            assertThrows(
                ScalarValidationError.class,
                () -> ScalarValidator.validate(Email.class, "notanemail", "parseValue"));

        assertEquals("Email", error.getScalarName());
        assertEquals("parseValue", error.getContext());
    }

    // ========================================================================
    // Utility Function Tests
    // ========================================================================

    @Test
    @DisplayName("Gets all custom scalars")
    void testGetAllCustomScalars() {
        ScalarProcessor.registerAll(Email.class, Phone.class);

        java.util.Map<String, Class<? extends CustomScalar>> scalars =
            ScalarValidator.getAllCustomScalars();
        assertTrue(scalars.containsKey("Email"));
        assertTrue(scalars.containsKey("Phone"));
        assertEquals(Email.class, scalars.get("Email"));
        assertEquals(Phone.class, scalars.get("Phone"));
    }

    @Test
    @DisplayName("Returns empty when no scalars registered")
    void testGetAllCustomScalarsEmpty() {
        java.util.Map<String, Class<? extends CustomScalar>> scalars =
            ScalarValidator.getAllCustomScalars();
        assertTrue(scalars.isEmpty());
    }

    // ========================================================================
    // Email Scalar Specific Tests
    // ========================================================================

    @Test
    @DisplayName("Email validates correct addresses")
    void testEmailValidatesCorrect() {
        Email email = new Email();
        Object result = email.parseValue("test@example.com");
        assertEquals("test@example.com", result);
    }

    @Test
    @DisplayName("Email rejects without @")
    void testEmailRejectsNoAt() {
        Email email = new Email();
        assertThrows(IllegalArgumentException.class, () -> email.parseValue("notanemail"));
    }

    @Test
    @DisplayName("Email serializes")
    void testEmailSerializes() {
        Email email = new Email();
        Object result = email.serialize("test@example.com");
        assertEquals("test@example.com", result);
    }

    // ========================================================================
    // Phone Scalar Specific Tests
    // ========================================================================

    @Test
    @DisplayName("Phone validates E.164 format")
    void testPhoneValidatesE164() {
        Phone phone = new Phone();
        Object result = phone.parseValue("+12025551234");
        assertEquals("+12025551234", result);
    }

    @Test
    @DisplayName("Phone requires + prefix")
    void testPhoneRequiresPlus() {
        Phone phone = new Phone();
        assertThrows(IllegalArgumentException.class, () -> phone.parseValue("2025551234"));
    }

    @Test
    @DisplayName("Phone validates digit count")
    void testPhoneValidatesDigitCount() {
        Phone phone = new Phone();
        assertThrows(IllegalArgumentException.class, () -> phone.parseValue("+123"));
    }

    @Test
    @DisplayName("Phone serializes")
    void testPhoneSerializes() {
        Phone phone = new Phone();
        Object result = phone.serialize("+12025551234");
        assertEquals("+12025551234", result);
    }

    // ========================================================================
    // Annotation Tests
    // ========================================================================

    @Test
    @DisplayName("@Scalar annotation is present on Email")
    void testScalarAnnotationPresent() {
        assertTrue(Email.class.isAnnotationPresent(Scalar.class));
    }

    @Test
    @DisplayName("CustomScalar toString works")
    void testToString() {
        Email email = new Email();
        assertEquals("CustomScalar(Email)", email.toString());
    }
}

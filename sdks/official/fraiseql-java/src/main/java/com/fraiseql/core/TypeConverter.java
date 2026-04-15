package com.fraiseql.core;

import java.lang.reflect.Field;
import java.time.LocalDate;
import java.time.LocalDateTime;
import java.util.*;

/**
 * Converts Java types to GraphQL types.
 * Supports all common Java types, nullable types via Optional, and custom types.
 */
public class TypeConverter {

    /**
     * Converts a Java type to a GraphQL type string.
     *
     * @param javaType the Java type to convert
     * @return the GraphQL type string (e.g., "Int", "String", "Boolean")
     */
    public static String javaToGraphQL(Class<?> javaType) {
        // Handle primitive types
        if (javaType == int.class || javaType == Integer.class) {
            return "Int";
        }
        if (javaType == long.class || javaType == Long.class) {
            return "Int";  // GraphQL doesn't distinguish long vs int
        }
        if (javaType == short.class || javaType == Short.class
            || javaType == byte.class || javaType == Byte.class) {
            return "Int";
        }
        if (javaType == float.class || javaType == Float.class ||
            javaType == double.class || javaType == Double.class) {
            return "Float";
        }
        if (javaType == java.math.BigDecimal.class || javaType == java.math.BigInteger.class) {
            return "Float";
        }
        if (javaType == boolean.class || javaType == Boolean.class) {
            return "Boolean";
        }
        if (javaType == String.class) {
            return "String";
        }
        if (javaType == java.util.UUID.class) {
            return "String";
        }

        // Handle temporal types
        if (javaType == LocalDate.class || javaType == LocalDateTime.class ||
            javaType == java.util.Date.class || javaType == java.sql.Date.class
            || javaType == java.sql.Timestamp.class) {
            return "String";  // Date types are represented as strings in GraphQL
        }

        // Handle collections
        if (Collection.class.isAssignableFrom(javaType)) {
            return "[String]";  // Generic fallback for unparameterized collections
        }
        if (javaType.isArray()) {
            return "[" + javaToGraphQL(javaType.getComponentType()) + "]";
        }

        // Handle Optional
        if (javaType == Optional.class) {
            return "String";  // Default fallback for bare Optional
        }

        // Custom types (classes with @GraphQLType annotation)
        if (javaType.isAnnotationPresent(GraphQLType.class)) {
            GraphQLType annotation = javaType.getAnnotation(GraphQLType.class);
            if (!annotation.name().isEmpty()) {
                return annotation.name();
            }
            return javaType.getSimpleName();
        }

        // Default: use class name
        return javaType.getSimpleName();
    }

    /**
     * Extracts field information from a class annotated with @GraphQLType.
     *
     * @param type the class to analyze
     * @return a map of field names to GraphQL type information
     */
    public static Map<String, GraphQLFieldInfo> extractFields(Class<?> type) {
        Map<String, GraphQLFieldInfo> fields = new LinkedHashMap<>();

        boolean isGraphQLType = type.isAnnotationPresent(GraphQLType.class);
        boolean isFactTable = type.isAnnotationPresent(GraphQLFactTable.class);
        if (!isGraphQLType && !isFactTable) {
            return fields;
        }

        for (Field field : type.getDeclaredFields()) {
            // Accept @GraphQLField, @Measure, or @Dimension as valid field markers
            boolean hasGraphQLField = field.isAnnotationPresent(GraphQLField.class);
            boolean hasMeasure = field.isAnnotationPresent(Measure.class);
            boolean hasDimension = field.isAnnotationPresent(Dimension.class);
            if (!hasGraphQLField && !hasMeasure && !hasDimension) {
                continue;
            }

            // For @Measure / @Dimension without @GraphQLField, synthesise a basic field info
            if (!hasGraphQLField) {
                String fieldName = field.getName();
                String graphQLType = javaToGraphQL(field.getType());
                String description = hasMeasure
                    ? field.getAnnotation(Measure.class).description()
                    : field.getAnnotation(Dimension.class).description();
                fields.put(fieldName, new GraphQLFieldInfo(fieldName, graphQLType, true, description));
                continue;
            }

            GraphQLField annotation = field.getAnnotation(GraphQLField.class);
            String fieldName = annotation.name().isEmpty() ? field.getName() : annotation.name();
            boolean isList = field.getType().isArray()
                || Collection.class.isAssignableFrom(field.getType());
            String graphQLType = annotation.type().isEmpty() ?
                javaToGraphQL(field.getType()) : annotation.type();
            boolean nullable = annotation.nullable();

            // Extract scope information.
            // The sentinel "\u0000" (NUL) is the default for requiresScope meaning "not set".
            // An explicitly empty "" means the user provided an empty scope → validate and reject.
            // Any other non-sentinel value is a scope to validate normally.
            String rawScope = annotation.requiresScope();
            String[] rawScopes = annotation.requiresScopes();

            boolean scopeIsUnset = rawScope.equals("\u0000");
            String requiresScope = scopeIsUnset ? null : rawScope;

            // Sentinel default for requiresScopes is {"\u0000"} (single NUL element = not set).
            // An explicitly empty {} means the user provided an empty array → reject.
            boolean scopesIsUnset = rawScopes.length == 1 && "\u0000".equals(rawScopes[0]);
            String[] requiresScopes;
            if (scopesIsUnset) {
                requiresScopes = null;
            } else if (rawScopes.length == 0) {
                // Explicitly provided empty array
                throw new RuntimeException(
                    String.format("Field %s.%s has empty requiresScopes array", type.getSimpleName(), fieldName)
                );
            } else {
                requiresScopes = rawScopes;
            }

            // Reject empty string scope (explicit "" means user forgot to provide a value)
            if (!scopeIsUnset && requiresScope != null && requiresScope.isEmpty()) {
                throw new RuntimeException(
                    String.format("Field %s.%s has empty scope", type.getSimpleName(), fieldName)
                );
            }
            // Validate scopes if present
            if (requiresScope != null && !requiresScope.isEmpty()) {
                validateScope(requiresScope, type.getSimpleName(), fieldName);
            }
            if (requiresScopes != null) {
                for (String scope : requiresScopes) {
                    validateScope(scope, type.getSimpleName(), fieldName);
                }
            }

            // Ensure at most one of requiresScope or requiresScopes is set
            if (requiresScope != null && requiresScopes != null) {
                throw new RuntimeException(
                    String.format("Field %s.%s cannot have both requiresScope and requiresScopes",
                        type.getSimpleName(), fieldName)
                );
            }

            GraphQLFieldInfo fieldInfo = new GraphQLFieldInfo(
                fieldName,
                graphQLType,
                nullable,
                annotation.description(),
                requiresScope,
                requiresScopes,
                annotation.computed()
            );
            fieldInfo.isList = isList;

            // Set deprecation flag if deprecated reason is provided
            if (!annotation.deprecated().isEmpty()) {
                fieldInfo.isDeprecated = true;
            }

            fields.put(fieldName, fieldInfo);
        }

        return fields;
    }

    /**
     * Validates scope format. Accepted patterns:
     * - Global wildcard: {@code *}
     * - Bare role/permission name (alphanumeric only, no underscores): {@code admin}, {@code auditor}
     * - Structured scope {@code action:resource}: {@code read:user.email}, {@code read:User.*}
     *   - action: alphanumeric + underscore
     *   - resource: alphanumeric + underscore + dot, optionally ending in {@code .*}; or just {@code *}
     */
    private static void validateScope(String scope, String typeName, String fieldName) {
        if (scope == null || scope.isEmpty()) {
            throw new RuntimeException(
                String.format("Field %s.%s has empty scope", typeName, fieldName)
            );
        }

        // Global wildcard is always valid
        if ("*".equals(scope)) {
            return;
        }

        // If there's no colon, allow simple bare role/permission names (letters and digits only)
        if (!scope.contains(":")) {
            if (!scope.matches("[a-zA-Z][a-zA-Z0-9]*")) {
                throw new RuntimeException(
                    String.format("Field %s.%s has invalid scope '%s' (missing colon)", typeName, fieldName, scope)
                );
            }
            return;
        }

        String[] parts = scope.split(":", 2);
        if (parts.length != 2) {
            throw new RuntimeException(
                String.format("Field %s.%s has invalid scope '%s'", typeName, fieldName, scope)
            );
        }

        String action = parts[0];
        String resource = parts[1];

        // Validate action: alphanumeric + underscore
        if (!action.matches("[a-zA-Z_][a-zA-Z0-9_]*")) {
            throw new RuntimeException(
                String.format("Field %s.%s has invalid action in scope '%s' (must be alphanumeric + underscore)",
                    typeName, fieldName, scope)
            );
        }

        // Validate resource: alphanumeric + underscore + dot, optionally ending in .*, or just *
        if (!resource.matches("[a-zA-Z_][a-zA-Z0-9_.]*(\\.[*])?|[*]")) {
            throw new RuntimeException(
                String.format("Field %s.%s has invalid resource in scope '%s' (must be alphanumeric + underscore + dot, or *)",
                    typeName, fieldName, scope)
            );
        }
    }

    /**
     * Information about a GraphQL field.
     */
    public static class GraphQLFieldInfo {
        public final String name;
        public final String type;
        public final boolean nullable;
        public final String description;
        public final String requiresScope;
        public final String[] requiresScopes;
        public final boolean computed;
        public boolean isDeprecated;
        public boolean isList;

        public GraphQLFieldInfo(String name, String type, boolean nullable, String description) {
            this(name, type, nullable, description, null, null, false);
        }

        /**
         * Convenience constructor for input-type fields where the name is provided
         * by the map key rather than stored in the object.
         *
         * @param type        GraphQL type string (e.g., "String", "Int")
         * @param nullable    whether this field can be null
         * @param deprecated  whether this field is deprecated
         * @param description human-readable description
         */
        public GraphQLFieldInfo(String type, boolean nullable, boolean deprecated, String description) {
            this("", type, nullable, description, null, null);
            this.isDeprecated = deprecated;
        }

        public GraphQLFieldInfo(String name, String type, boolean nullable, String description,
                               String requiresScope, String[] requiresScopes, boolean computed) {
            this.name = name;
            this.type = type;
            this.nullable = nullable;
            this.description = description;
            this.requiresScope = requiresScope;
            this.requiresScopes = requiresScopes;
            this.computed = computed;
            this.isDeprecated = false;
        }

        /**
         * Return the GraphQL type string including the non-null marker ({@code !}) for
         * non-nullable fields.
         * For list fields the type is already wrapped in brackets (e.g. {@code "[String]"}),
         * so the result is e.g. {@code "[String]!"} or {@code "[String]"}.
         */
        public String getGraphQLType() {
            return nullable ? type : type + "!";
        }

        public String getRequiresScope() {
            return requiresScope;
        }

        public String[] getRequiresScopes() {
            return requiresScopes;
        }

        @Override
        public String toString() {
            return String.format("{name='%s', type='%s', nullable=%b}", name, type, nullable);
        }
    }
}

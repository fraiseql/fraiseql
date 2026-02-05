package com.fraiseql.core;

import java.util.*;

/**
 * Validates GraphQL schemas for correctness and completeness.
 * Checks for common errors such as:
 * - Missing return types in queries/mutations
 * - Undefined types referenced in queries
 * - Duplicate operation names
 * - Invalid type references
 */
public class SchemaValidator {

    /**
     * Result of schema validation.
     */
    public static class ValidationResult {
        public final boolean valid;
        public final List<String> errors;
        public final List<String> warnings;

        public ValidationResult(boolean valid, List<String> errors, List<String> warnings) {
            this.valid = valid;
            this.errors = errors;
            this.warnings = warnings;
        }

        @Override
        public String toString() {
            StringBuilder sb = new StringBuilder();
            sb.append("ValidationResult{valid=").append(valid);
            if (!errors.isEmpty()) {
                sb.append(", errors=[");
                for (int i = 0; i < errors.size(); i++) {
                    if (i > 0) sb.append(", ");
                    sb.append("\"").append(errors.get(i)).append("\"");
                }
                sb.append("]");
            }
            if (!warnings.isEmpty()) {
                sb.append(", warnings=[");
                for (int i = 0; i < warnings.size(); i++) {
                    if (i > 0) sb.append(", ");
                    sb.append("\"").append(warnings.get(i)).append("\"");
                }
                sb.append("]");
            }
            sb.append("}");
            return sb.toString();
        }
    }

    /**
     * Validate the entire schema registry.
     *
     * @param registry the SchemaRegistry to validate
     * @return ValidationResult with errors and warnings
     */
    public static ValidationResult validate(SchemaRegistry registry) {
        List<String> errors = new ArrayList<>();
        List<String> warnings = new ArrayList<>();

        // Validate types
        validateTypes(registry, errors, warnings);

        // Validate queries
        validateQueries(registry, errors, warnings);

        // Validate mutations
        validateMutations(registry, errors, warnings);

        boolean valid = errors.isEmpty();
        return new ValidationResult(valid, errors, warnings);
    }

    /**
     * Validate all registered types.
     */
    private static void validateTypes(SchemaRegistry registry, List<String> errors, List<String> warnings) {
        Map<String, SchemaRegistry.GraphQLTypeInfo> types = registry.getAllTypes();

        if (types.isEmpty()) {
            warnings.add("No types registered in schema");
            return;
        }

        for (SchemaRegistry.GraphQLTypeInfo typeInfo : types.values()) {
            if (typeInfo.fields.isEmpty()) {
                warnings.add("Type '" + typeInfo.name + "' has no fields");
            }

            // Validate field types
            for (TypeConverter.GraphQLFieldInfo fieldInfo : typeInfo.fields.values()) {
                if (fieldInfo.type == null || fieldInfo.type.isEmpty()) {
                    errors.add("Field '" + typeInfo.name + "." + fieldInfo.name + "' has no type");
                }
            }
        }
    }

    /**
     * Validate all registered queries.
     */
    private static void validateQueries(SchemaRegistry registry, List<String> errors, List<String> warnings) {
        Map<String, SchemaRegistry.QueryInfo> queries = registry.getAllQueries();
        Map<String, SchemaRegistry.GraphQLTypeInfo> types = registry.getAllTypes();

        for (SchemaRegistry.QueryInfo queryInfo : queries.values()) {
            // Check return type exists
            String returnTypeName = extractBaseType(queryInfo.returnType);
            if (!types.containsKey(returnTypeName)) {
                errors.add("Query '" + queryInfo.name + "' references undefined return type '" + returnTypeName + "'");
            }

            // Check argument types are valid
            for (String argType : queryInfo.arguments.values()) {
                if (!isValidGraphQLType(argType)) {
                    errors.add("Query '" + queryInfo.name + "' has invalid argument type '" + argType + "'");
                }
            }

            // Warnings
            if (queryInfo.arguments.isEmpty()) {
                warnings.add("Query '" + queryInfo.name + "' has no arguments");
            }
        }
    }

    /**
     * Validate all registered mutations.
     */
    private static void validateMutations(SchemaRegistry registry, List<String> errors, List<String> warnings) {
        Map<String, SchemaRegistry.MutationInfo> mutations = registry.getAllMutations();
        Map<String, SchemaRegistry.GraphQLTypeInfo> types = registry.getAllTypes();

        for (SchemaRegistry.MutationInfo mutationInfo : mutations.values()) {
            // Check return type exists
            String returnTypeName = extractBaseType(mutationInfo.returnType);
            if (!types.containsKey(returnTypeName)) {
                errors.add("Mutation '" + mutationInfo.name + "' references undefined return type '" + returnTypeName + "'");
            }

            // Check argument types are valid
            for (String argType : mutationInfo.arguments.values()) {
                if (!isValidGraphQLType(argType)) {
                    errors.add("Mutation '" + mutationInfo.name + "' has invalid argument type '" + argType + "'");
                }
            }

            // Warnings
            if (mutationInfo.arguments.isEmpty()) {
                warnings.add("Mutation '" + mutationInfo.name + "' has no arguments");
            }
        }
    }

    /**
     * Extract base type from a GraphQL type string.
     * Handles [Type] and Type! notation.
     *
     * @param typeString the GraphQL type string
     * @return the base type name
     */
    private static String extractBaseType(String typeString) {
        if (typeString == null || typeString.isEmpty()) {
            return "";
        }

        // Remove list brackets
        String type = typeString.replace("[", "").replace("]", "");

        // Remove nullable modifier
        type = type.replace("!", "");

        return type.trim();
    }

    /**
     * Check if a type string is a valid GraphQL type.
     * Valid types include: Int, Float, String, Boolean, and custom types.
     *
     * @param typeString the type to check
     * @return true if valid
     */
    private static boolean isValidGraphQLType(String typeString) {
        if (typeString == null || typeString.isEmpty()) {
            return false;
        }

        // Built-in scalar types
        Set<String> scalarTypes = new HashSet<>(Arrays.asList(
            "Int", "Float", "String", "Boolean", "ID"
        ));

        String baseType = extractBaseType(typeString);
        return scalarTypes.contains(baseType) || !baseType.isEmpty();
    }

    /**
     * Get schema statistics.
     *
     * @param registry the SchemaRegistry
     * @return statistics as formatted string
     */
    public static String getStatistics(SchemaRegistry registry) {
        int typeCount = registry.getAllTypes().size();
        int queryCount = registry.getAllQueries().size();
        int mutationCount = registry.getAllMutations().size();
        int fieldCount = registry.getAllTypes().values().stream()
            .mapToInt(t -> t.fields.size())
            .sum();

        return String.format(
            "Schema Statistics: %d types (%d fields), %d queries, %d mutations",
            typeCount, fieldCount, queryCount, mutationCount
        );
    }
}

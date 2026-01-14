package com.fraiseql.core;

import com.fasterxml.jackson.databind.ObjectMapper;
import java.io.File;
import java.io.IOException;
import java.util.*;

/**
 * Main FraiseQL API for schema definition.
 * Provides fluent builders for queries, mutations, and type registration.
 *
 * Example usage:
 * <pre>
 * FraiseQL.registerType(User.class);
 * FraiseQL.registerType(Post.class);
 *
 * FraiseQL.query("users")
 *     .returnType(User.class)
 *     .returnsArray(true)
 *     .arg("limit", "Int", 10)
 *     .register();
 *
 * FraiseQL.exportSchema("schema.json");
 * </pre>
 */
public class FraiseQL {
    private static final ObjectMapper mapper = new ObjectMapper();
    private static final SchemaRegistry registry = SchemaRegistry.getInstance();

    private FraiseQL() {
        // Prevent instantiation
    }

    /**
     * Register a GraphQL type.
     *
     * @param typeClass the class annotated with @GraphQLType
     */
    public static void registerType(Class<?> typeClass) {
        String typeName = typeClass.getSimpleName();
        if (typeClass.isAnnotationPresent(GraphQLType.class)) {
            GraphQLType annotation = typeClass.getAnnotation(GraphQLType.class);
            if (!annotation.name().isEmpty()) {
                typeName = annotation.name();
            }
        }
        registry.registerType(typeName, typeClass);
    }

    /**
     * Register multiple GraphQL types.
     *
     * @param typeClasses the classes annotated with @GraphQLType
     */
    public static void registerTypes(Class<?>... typeClasses) {
        for (Class<?> typeClass : typeClasses) {
            registerType(typeClass);
        }
    }

    /**
     * Create a query builder.
     *
     * @param queryName the query name
     * @return a QueryBuilder for this query
     */
    public static QueryBuilder query(String queryName) {
        return new QueryBuilder(queryName);
    }

    /**
     * Create a mutation builder.
     *
     * @param mutationName the mutation name
     * @return a MutationBuilder for this mutation
     */
    public static MutationBuilder mutation(String mutationName) {
        return new MutationBuilder(mutationName);
    }

    /**
     * Export the schema to a JSON file.
     * Generates schema.json compatible with fraiseql-cli compile.
     *
     * @param filePath the output file path
     * @throws IOException if writing to file fails
     */
    public static void exportSchema(String filePath) throws IOException {
        var schema = SchemaFormatter.formatSchema(registry);
        SchemaFormatter.writeToFile(schema, filePath);
    }

    /**
     * Get the schema registry.
     *
     * @return the SchemaRegistry instance
     */
    public static SchemaRegistry getRegistry() {
        return registry;
    }

    /**
     * Clear all registered types and operations.
     * Useful for testing.
     */
    public static void clear() {
        registry.clear();
    }

    /**
     * Builder for GraphQL queries.
     */
    public static class QueryBuilder {
        private final String name;
        private String returnType;
        private boolean returnsArray = false;
        private final Map<String, String> arguments = new LinkedHashMap<>();
        private String description = "";

        private QueryBuilder(String name) {
            this.name = name;
        }

        /**
         * Set the return type for this query.
         *
         * @param typeClass the return type class
         * @return this builder for chaining
         */
        public QueryBuilder returnType(Class<?> typeClass) {
            this.returnType = TypeConverter.javaToGraphQL(typeClass);
            return this;
        }

        /**
         * Set the return type using a GraphQL type name.
         *
         * @param typeName the GraphQL type name
         * @return this builder for chaining
         */
        public QueryBuilder returnType(String typeName) {
            this.returnType = typeName;
            return this;
        }

        /**
         * Set whether this query returns an array of the type.
         *
         * @param returnsArray true if returns array, false otherwise
         * @return this builder for chaining
         */
        public QueryBuilder returnsArray(boolean returnsArray) {
            this.returnsArray = returnsArray;
            return this;
        }

        /**
         * Add an argument to this query.
         *
         * @param argName the argument name
         * @param argType the argument GraphQL type
         * @return this builder for chaining
         */
        public QueryBuilder arg(String argName, String argType) {
            arguments.put(argName, argType);
            return this;
        }

        /**
         * Add an argument to this query with a default value (stored as string).
         *
         * @param argName the argument name
         * @param argType the argument GraphQL type
         * @param defaultValue the default value (can be null)
         * @return this builder for chaining
         */
        public QueryBuilder arg(String argName, String argType, Object defaultValue) {
            arguments.put(argName, argType);
            return this;
        }

        /**
         * Set the description for this query.
         *
         * @param description the query description
         * @return this builder for chaining
         */
        public QueryBuilder description(String description) {
            this.description = description;
            return this;
        }

        /**
         * Register this query in the schema.
         */
        public void register() {
            String finalReturnType = returnsArray ? "[" + returnType + "]" : returnType;
            registry.registerQuery(name, finalReturnType, arguments, description);
        }
    }

    /**
     * Builder for GraphQL mutations.
     */
    public static class MutationBuilder {
        private final String name;
        private String returnType;
        private boolean returnsArray = false;
        private final Map<String, String> arguments = new LinkedHashMap<>();
        private String description = "";

        private MutationBuilder(String name) {
            this.name = name;
        }

        /**
         * Set the return type for this mutation.
         *
         * @param typeClass the return type class
         * @return this builder for chaining
         */
        public MutationBuilder returnType(Class<?> typeClass) {
            this.returnType = TypeConverter.javaToGraphQL(typeClass);
            return this;
        }

        /**
         * Set the return type using a GraphQL type name.
         *
         * @param typeName the GraphQL type name
         * @return this builder for chaining
         */
        public MutationBuilder returnType(String typeName) {
            this.returnType = typeName;
            return this;
        }

        /**
         * Set whether this mutation returns an array of the type.
         *
         * @param returnsArray true if returns array, false otherwise
         * @return this builder for chaining
         */
        public MutationBuilder returnsArray(boolean returnsArray) {
            this.returnsArray = returnsArray;
            return this;
        }

        /**
         * Add an argument to this mutation.
         *
         * @param argName the argument name
         * @param argType the argument GraphQL type
         * @return this builder for chaining
         */
        public MutationBuilder arg(String argName, String argType) {
            arguments.put(argName, argType);
            return this;
        }

        /**
         * Add an argument to this mutation with a default value (stored as string).
         *
         * @param argName the argument name
         * @param argType the argument GraphQL type
         * @param defaultValue the default value (can be null)
         * @return this builder for chaining
         */
        public MutationBuilder arg(String argName, String argType, Object defaultValue) {
            arguments.put(argName, argType);
            return this;
        }

        /**
         * Set the description for this mutation.
         *
         * @param description the mutation description
         * @return this builder for chaining
         */
        public MutationBuilder description(String description) {
            this.description = description;
            return this;
        }

        /**
         * Register this mutation in the schema.
         */
        public void register() {
            String finalReturnType = returnsArray ? "[" + returnType + "]" : returnType;
            registry.registerMutation(name, finalReturnType, arguments, description);
        }
    }

}

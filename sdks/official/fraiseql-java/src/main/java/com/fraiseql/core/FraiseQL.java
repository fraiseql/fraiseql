package com.fraiseql.core;

import com.fasterxml.jackson.databind.JsonNode;
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

        // Auto-generate CRUD operations if crud annotation is set
        if (typeClass.isAnnotationPresent(GraphQLType.class)) {
            GraphQLType annotation = typeClass.getAnnotation(GraphQLType.class);
            String[] crud = annotation.crud();
            if (crud.length > 0) {
                var typeInfo = registry.getType(typeName);
                typeInfo.ifPresent(info ->
                    registry.generateCrudOperations(info.name, info.fields, crud));
            }
        }
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
     * Create a subscription builder.
     * Subscriptions in FraiseQL are compiled projections of database events.
     * They are sourced from LISTEN/NOTIFY or CDC, not resolver-based.
     *
     * @param subscriptionName the subscription name
     * @return a SubscriptionBuilder for this subscription
     */
    public static SubscriptionBuilder subscription(String subscriptionName) {
        return new SubscriptionBuilder(subscriptionName);
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
     * Export minimal types.json (types only, no queries/mutations).
     * For TOML-based workflow: Java generates types.json, fraiseql.toml provides config.
     *
     * @param filePath the output file path
     * @throws IOException if writing to file fails
     */
    public static void exportTypes(String filePath) throws IOException {
        var schema = SchemaFormatter.formatMinimalSchema(registry);
        SchemaFormatter.writeToFile(schema, filePath);
    }

    /**
     * Export minimal types.json with optional pretty-printing.
     *
     * @param filePath the output file path
     * @param pretty whether to pretty-print JSON
     * @throws IOException if writing to file fails
     */
    public static void exportTypes(String filePath, boolean pretty) throws IOException {
        var schema = SchemaFormatter.formatMinimalSchema(registry);
        SchemaFormatter.writeToFile(schema, filePath, pretty);
    }

    /**
     * Export the full schema as a JsonNode (for in-memory assertions in tests).
     *
     * @return the schema as a Jackson JsonNode
     */
    public static JsonNode exportSchemaAsJson() {
        return SchemaFormatter.formatSchema(registry);
    }

    /**
     * Register an error type (is_error = true) in the schema.
     * Error types are returned by mutations to signal domain errors.
     *
     * @param typeClass the class annotated with @GraphQLType
     */
    public static void registerErrorType(Class<?> typeClass) {
        String typeName = typeClass.getSimpleName();
        if (typeClass.isAnnotationPresent(GraphQLType.class)) {
            GraphQLType annotation = typeClass.getAnnotation(GraphQLType.class);
            if (!annotation.name().isEmpty()) {
                typeName = annotation.name();
            }
        }
        registry.registerErrorType(typeName, typeClass);
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
        private boolean relay = false;
        private String sqlSource = null;
        private Long cacheTtlSeconds = null;
        private Map<String, String> injectParams = null;
        private List<String> additionalViews = null;
        private String restPath = null;
        private String restMethod = null;

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
         * Mark this query as a Relay connection query.
         * Requires returnsArray(true). The compiler derives the cursor column from the
         * return type name (e.g. User -> pk_user) and generates Connection/Edge types.
         *
         * @param relay true to enable Relay connection wrapping
         * @return this builder for chaining
         */
        public QueryBuilder relay(boolean relay) {
            this.relay = relay;
            return this;
        }

        /**
         * Set the underlying SQL view or table for this query.
         *
         * @param sqlSource view or table name
         * @return this builder for chaining
         */
        public QueryBuilder sqlSource(String sqlSource) {
            this.sqlSource = sqlSource;
            return this;
        }

        /**
         * Set the cache TTL for query results.
         *
         * @param seconds TTL in seconds (0 = no cache)
         * @return this builder for chaining
         */
        public QueryBuilder cacheTtlSeconds(long seconds) {
            this.cacheTtlSeconds = seconds;
            return this;
        }

        /**
         * Inject server-side parameters derived from the JWT.
         * Map keys are parameter names; values are {@code "jwt:<claim>"} expressions.
         *
         * @param params inject mapping
         * @return this builder for chaining
         */
        public QueryBuilder inject(Map<String, String> params) {
            this.injectParams = new LinkedHashMap<>(params);
            return this;
        }

        /**
         * Declare additional views invalidated when this query's cache should be cleared.
         *
         * @param views list of view names
         * @return this builder for chaining
         */
        public QueryBuilder additionalViews(List<String> views) {
            this.additionalViews = new ArrayList<>(views);
            return this;
        }

        /**
         * Set the REST endpoint path for this query.
         *
         * @param path the REST path (e.g. "/api/users")
         * @return this builder for chaining
         */
        public QueryBuilder restPath(String path) {
            this.restPath = path;
            return this;
        }

        /**
         * Set the HTTP method for the REST endpoint.
         * Defaults to GET for queries. Must be one of: GET, POST, PUT, PATCH, DELETE.
         *
         * @param method the HTTP method
         * @return this builder for chaining
         */
        public QueryBuilder restMethod(String method) {
            this.restMethod = method;
            return this;
        }

        /**
         * Register this query in the schema.
         *
         * @throws IllegalStateException if relay(true) is set without returnsArray(true)
         */
        public void register() {
            if (relay && !returnsArray) {
                throw new IllegalStateException(
                    "Query '" + name + "': relay(true) requires returnsArray(true). " +
                    "Relay connections only apply to list queries."
                );
            }
            String finalReturnType = returnsArray ? "[" + returnType + "]" : returnType;
            if (sqlSource != null || cacheTtlSeconds != null || injectParams != null
                    || additionalViews != null || restPath != null) {
                String effectiveRestMethod = restMethod != null ? restMethod.toUpperCase(java.util.Locale.ROOT) : null;
                registry.registerQuery(name, finalReturnType, arguments, description, relay,
                    sqlSource, cacheTtlSeconds, injectParams, additionalViews,
                    restPath, effectiveRestMethod);
            } else {
                registry.registerQuery(name, finalReturnType, arguments, description, relay);
            }
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
        private String sqlSource = null;
        private String operation = null;
        private Map<String, String> injectParams = null;
        private List<String> invalidatesViews = null;
        private List<String> invalidatesFactTables = null;
        private String restPath = null;
        private String restMethod = null;

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
         * Set the underlying SQL function for this mutation.
         *
         * @param sqlSource function name
         * @return this builder for chaining
         */
        public MutationBuilder sqlSource(String sqlSource) {
            this.sqlSource = sqlSource;
            return this;
        }

        /**
         * Set the DML operation type (e.g. "insert", "update", "delete").
         *
         * @param operation operation name
         * @return this builder for chaining
         */
        public MutationBuilder operation(String operation) {
            this.operation = operation;
            return this;
        }

        /**
         * Inject server-side parameters derived from the JWT.
         *
         * @param params inject mapping (key → "jwt:&lt;claim&gt;")
         * @return this builder for chaining
         */
        public MutationBuilder inject(Map<String, String> params) {
            this.injectParams = new LinkedHashMap<>(params);
            return this;
        }

        /**
         * Declare cache views that should be invalidated when this mutation succeeds.
         *
         * @param views list of view names
         * @return this builder for chaining
         */
        public MutationBuilder invalidatesViews(List<String> views) {
            this.invalidatesViews = new ArrayList<>(views);
            return this;
        }

        /**
         * Declare fact tables whose cached aggregates should be invalidated.
         *
         * @param tables list of fact table names
         * @return this builder for chaining
         */
        public MutationBuilder invalidatesFactTables(List<String> tables) {
            this.invalidatesFactTables = new ArrayList<>(tables);
            return this;
        }

        /**
         * Set the REST endpoint path for this mutation.
         *
         * @param path the REST path (e.g. "/api/users")
         * @return this builder for chaining
         */
        public MutationBuilder restPath(String path) {
            this.restPath = path;
            return this;
        }

        /**
         * Set the HTTP method for the REST endpoint.
         * Defaults to POST for mutations. Must be one of: GET, POST, PUT, PATCH, DELETE.
         *
         * @param method the HTTP method
         * @return this builder for chaining
         */
        public MutationBuilder restMethod(String method) {
            this.restMethod = method;
            return this;
        }

        /**
         * Register this mutation in the schema.
         */
        public void register() {
            String finalReturnType = returnsArray ? "[" + returnType + "]" : returnType;
            if (sqlSource != null || operation != null || injectParams != null
                    || invalidatesViews != null || invalidatesFactTables != null || restPath != null) {
                String effectiveRestMethod = restMethod != null ? restMethod.toUpperCase(java.util.Locale.ROOT) : null;
                registry.registerMutation(name, finalReturnType, arguments, description,
                    sqlSource, operation, injectParams, invalidatesViews, invalidatesFactTables,
                    restPath, effectiveRestMethod);
            } else {
                registry.registerMutation(name, finalReturnType, arguments, description);
            }
        }
    }

    /**
     * Builder for GraphQL subscriptions.
     * Subscriptions in FraiseQL are compiled projections of database events.
     * They are sourced from LISTEN/NOTIFY or CDC, not resolver-based.
     */
    public static class SubscriptionBuilder {
        private final String name;
        private String entityType;
        private final Map<String, String> arguments = new LinkedHashMap<>();
        private String description = "";
        private String topic = null;
        private String operation = null;

        private SubscriptionBuilder(String name) {
            this.name = name;
        }

        /**
         * Set the entity type for this subscription.
         *
         * @param typeClass the entity type class
         * @return this builder for chaining
         */
        public SubscriptionBuilder entityType(Class<?> typeClass) {
            this.entityType = TypeConverter.javaToGraphQL(typeClass);
            return this;
        }

        /**
         * Set the entity type using a GraphQL type name.
         *
         * @param typeName the GraphQL type name
         * @return this builder for chaining
         */
        public SubscriptionBuilder entityType(String typeName) {
            this.entityType = typeName;
            return this;
        }

        /**
         * Add an argument to this subscription (for filtering events).
         *
         * @param argName the argument name
         * @param argType the argument GraphQL type
         * @return this builder for chaining
         */
        public SubscriptionBuilder arg(String argName, String argType) {
            arguments.put(argName, argType);
            return this;
        }

        /**
         * Set the description for this subscription.
         *
         * @param description the subscription description
         * @return this builder for chaining
         */
        public SubscriptionBuilder description(String description) {
            this.description = description;
            return this;
        }

        /**
         * Set the topic/channel name for this subscription.
         *
         * @param topic the LISTEN/NOTIFY channel or CDC topic
         * @return this builder for chaining
         */
        public SubscriptionBuilder topic(String topic) {
            this.topic = topic;
            return this;
        }

        /**
         * Set the operation filter for this subscription.
         *
         * @param operation the operation type (CREATE, UPDATE, DELETE)
         * @return this builder for chaining
         */
        public SubscriptionBuilder operation(String operation) {
            this.operation = operation;
            return this;
        }

        /**
         * Register this subscription in the schema.
         */
        public void register() {
            registry.registerSubscription(name, entityType, arguments, description, topic, operation);
        }
    }

}

package com.fraiseql.core;

import java.util.*;
import java.util.concurrent.ConcurrentHashMap;

/**
 * Thread-safe registry for GraphQL types, queries, mutations, and subscriptions.
 * Singleton pattern ensures single schema instance across application.
 * Used by FraiseQL to track all registered types and root operations.
 */
public class SchemaRegistry {
    private static final SchemaRegistry INSTANCE = new SchemaRegistry();

    private final Map<String, GraphQLTypeInfo> types;
    private final Map<String, QueryInfo> queries;
    private final Map<String, MutationInfo> mutations;
    private final Map<String, SubscriptionInfo> subscriptions;

    private SchemaRegistry() {
        this.types = new ConcurrentHashMap<>();
        this.queries = new ConcurrentHashMap<>();
        this.mutations = new ConcurrentHashMap<>();
        this.subscriptions = new ConcurrentHashMap<>();
    }

    /**
     * Get the singleton instance of SchemaRegistry.
     *
     * @return the SchemaRegistry instance
     */
    public static SchemaRegistry getInstance() {
        return INSTANCE;
    }

    /**
     * Register a GraphQL type in the schema.
     *
     * @param typeName the GraphQL type name
     * @param typeClass the Java class annotated with @GraphQLType
     * @throws IllegalArgumentException if type is not annotated with @GraphQLType
     */
    public void registerType(String typeName, Class<?> typeClass) {
        if (!typeClass.isAnnotationPresent(GraphQLType.class)) {
            throw new IllegalArgumentException("Class " + typeClass.getName() + " must be annotated with @GraphQLType");
        }

        GraphQLType annotation = typeClass.getAnnotation(GraphQLType.class);
        String name = typeName;
        if (annotation.name() != null && !annotation.name().isEmpty()) {
            name = annotation.name();
        }

        Map<String, TypeConverter.GraphQLFieldInfo> fields = TypeConverter.extractFields(typeClass);

        GraphQLTypeInfo typeInfo = new GraphQLTypeInfo(
            name,
            typeClass,
            fields,
            annotation.description()
        );

        types.put(name, typeInfo);
    }

    /**
     * Register a query in the schema.
     *
     * @param queryName the query name
     * @param returnType the return type name
     * @param arguments the query arguments
     * @param description optional description
     */
    public void registerQuery(String queryName, String returnType, Map<String, String> arguments, String description) {
        QueryInfo queryInfo = new QueryInfo(queryName, returnType, arguments, description);
        queries.put(queryName, queryInfo);
    }

    /**
     * Register a mutation in the schema.
     *
     * @param mutationName the mutation name
     * @param returnType the return type name
     * @param arguments the mutation arguments
     * @param description optional description
     */
    public void registerMutation(String mutationName, String returnType, Map<String, String> arguments, String description) {
        MutationInfo mutationInfo = new MutationInfo(mutationName, returnType, arguments, description);
        mutations.put(mutationName, mutationInfo);
    }

    /**
     * Register a subscription in the schema.
     * Subscriptions in FraiseQL are compiled projections of database events.
     * They are sourced from LISTEN/NOTIFY or CDC, not resolver-based.
     *
     * @param subscriptionName the subscription name
     * @param entityType the entity type being subscribed to
     * @param arguments the subscription arguments (filters)
     * @param description optional description
     */
    public void registerSubscription(String subscriptionName, String entityType, Map<String, String> arguments, String description) {
        SubscriptionInfo subscriptionInfo = new SubscriptionInfo(subscriptionName, entityType, arguments, description, null, null);
        subscriptions.put(subscriptionName, subscriptionInfo);
    }

    /**
     * Register a subscription in the schema with topic and operation.
     * Subscriptions in FraiseQL are compiled projections of database events.
     * They are sourced from LISTEN/NOTIFY or CDC, not resolver-based.
     *
     * @param subscriptionName the subscription name
     * @param entityType the entity type being subscribed to
     * @param arguments the subscription arguments (filters)
     * @param description optional description
     * @param topic optional topic/channel name for filtering events
     * @param operation optional operation filter (CREATE, UPDATE, DELETE)
     */
    public void registerSubscription(String subscriptionName, String entityType, Map<String, String> arguments,
                                     String description, String topic, String operation) {
        SubscriptionInfo subscriptionInfo = new SubscriptionInfo(subscriptionName, entityType, arguments, description, topic, operation);
        subscriptions.put(subscriptionName, subscriptionInfo);
    }

    /**
     * Get a registered type by name.
     *
     * @param typeName the type name
     * @return the GraphQLTypeInfo or empty Optional if not found
     */
    public Optional<GraphQLTypeInfo> getType(String typeName) {
        return Optional.ofNullable(types.get(typeName));
    }

    /**
     * Get a registered query by name.
     *
     * @param queryName the query name
     * @return the QueryInfo or empty Optional if not found
     */
    public Optional<QueryInfo> getQuery(String queryName) {
        return Optional.ofNullable(queries.get(queryName));
    }

    /**
     * Get a registered mutation by name.
     *
     * @param mutationName the mutation name
     * @return the MutationInfo or empty Optional if not found
     */
    public Optional<MutationInfo> getMutation(String mutationName) {
        return Optional.ofNullable(mutations.get(mutationName));
    }

    /**
     * Get a registered subscription by name.
     *
     * @param subscriptionName the subscription name
     * @return the SubscriptionInfo or empty Optional if not found
     */
    public Optional<SubscriptionInfo> getSubscription(String subscriptionName) {
        return Optional.ofNullable(subscriptions.get(subscriptionName));
    }

    /**
     * Get all registered types.
     *
     * @return unmodifiable map of type name to GraphQLTypeInfo
     */
    public Map<String, GraphQLTypeInfo> getAllTypes() {
        return Collections.unmodifiableMap(types);
    }

    /**
     * Get all registered queries.
     *
     * @return unmodifiable map of query name to QueryInfo
     */
    public Map<String, QueryInfo> getAllQueries() {
        return Collections.unmodifiableMap(queries);
    }

    /**
     * Get all registered mutations.
     *
     * @return unmodifiable map of mutation name to MutationInfo
     */
    public Map<String, MutationInfo> getAllMutations() {
        return Collections.unmodifiableMap(mutations);
    }

    /**
     * Get all registered subscriptions.
     *
     * @return unmodifiable map of subscription name to SubscriptionInfo
     */
    public Map<String, SubscriptionInfo> getAllSubscriptions() {
        return Collections.unmodifiableMap(subscriptions);
    }

    /**
     * Clear all registered types, queries, mutations, and subscriptions.
     * Useful for testing.
     */
    public void clear() {
        types.clear();
        queries.clear();
        mutations.clear();
        subscriptions.clear();
    }

    /**
     * Information about a registered GraphQL type.
     */
    public static class GraphQLTypeInfo {
        public final String name;
        public final Class<?> javaClass;
        public final Map<String, TypeConverter.GraphQLFieldInfo> fields;
        public final String description;

        public GraphQLTypeInfo(String name, Class<?> javaClass, Map<String, TypeConverter.GraphQLFieldInfo> fields, String description) {
            this.name = name;
            this.javaClass = javaClass;
            this.fields = Collections.unmodifiableMap(new LinkedHashMap<>(fields));
            this.description = description;
        }

        @Override
        public String toString() {
            return "GraphQLTypeInfo{" +
                "name='" + name + '\'' +
                ", javaClass=" + javaClass.getSimpleName() +
                ", fields=" + fields.size() +
                '}';
        }
    }

    /**
     * Information about a registered GraphQL query.
     */
    public static class QueryInfo {
        public final String name;
        public final String returnType;
        public final Map<String, String> arguments;
        public final String description;

        public QueryInfo(String name, String returnType, Map<String, String> arguments, String description) {
            this.name = name;
            this.returnType = returnType;
            this.arguments = Collections.unmodifiableMap(new LinkedHashMap<>(arguments));
            this.description = description;
        }

        @Override
        public String toString() {
            return "QueryInfo{" +
                "name='" + name + '\'' +
                ", returnType='" + returnType + '\'' +
                ", arguments=" + arguments.size() +
                '}';
        }
    }

    /**
     * Information about a registered GraphQL mutation.
     */
    public static class MutationInfo {
        public final String name;
        public final String returnType;
        public final Map<String, String> arguments;
        public final String description;

        public MutationInfo(String name, String returnType, Map<String, String> arguments, String description) {
            this.name = name;
            this.returnType = returnType;
            this.arguments = Collections.unmodifiableMap(new LinkedHashMap<>(arguments));
            this.description = description;
        }

        @Override
        public String toString() {
            return "MutationInfo{" +
                "name='" + name + '\'' +
                ", returnType='" + returnType + '\'' +
                ", arguments=" + arguments.size() +
                '}';
        }
    }

    /**
     * Information about a registered GraphQL subscription.
     * Subscriptions in FraiseQL are compiled projections of database events.
     * They are sourced from LISTEN/NOTIFY or CDC, not resolver-based.
     */
    public static class SubscriptionInfo {
        public final String name;
        public final String entityType;
        public final Map<String, String> arguments;
        public final String description;
        public final String topic;
        public final String operation;

        public SubscriptionInfo(String name, String entityType, Map<String, String> arguments,
                                String description, String topic, String operation) {
            this.name = name;
            this.entityType = entityType;
            this.arguments = Collections.unmodifiableMap(new LinkedHashMap<>(arguments));
            this.description = description;
            this.topic = topic;
            this.operation = operation;
        }

        @Override
        public String toString() {
            return "SubscriptionInfo{" +
                "name='" + name + '\'' +
                ", entityType='" + entityType + '\'' +
                ", arguments=" + arguments.size() +
                (topic != null ? ", topic='" + topic + '\'' : "") +
                (operation != null ? ", operation='" + operation + '\'' : "") +
                '}';
        }
    }
}

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
    private final Map<String, EnumInfo> enums;
    private final Map<String, InterfaceInfo> interfaces;
    private final Map<String, UnionInfo> unions;
    private final Map<String, InputTypeInfo> inputTypes;

    private SchemaRegistry() {
        this.types = new ConcurrentHashMap<>();
        this.queries = new ConcurrentHashMap<>();
        this.mutations = new ConcurrentHashMap<>();
        this.subscriptions = new ConcurrentHashMap<>();
        this.enums = new ConcurrentHashMap<>();
        this.interfaces = new ConcurrentHashMap<>();
        this.unions = new ConcurrentHashMap<>();
        this.inputTypes = new ConcurrentHashMap<>();
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
     * Register an enum type in the schema.
     *
     * @param enumName the enum name
     * @param values the enum values (name -> value map)
     * @param description optional description
     */
    public void registerEnum(String enumName, Map<String, String> values, String description) {
        EnumInfo enumInfo = new EnumInfo(enumName, values, description);
        enums.put(enumName, enumInfo);
    }

    /**
     * Register an interface type in the schema.
     *
     * @param interfaceName the interface name
     * @param fields the interface fields
     * @param description optional description
     */
    public void registerInterface(String interfaceName, Map<String, TypeConverter.GraphQLFieldInfo> fields, String description) {
        InterfaceInfo interfaceInfo = new InterfaceInfo(interfaceName, fields, description);
        interfaces.put(interfaceName, interfaceInfo);
    }

    /**
     * Register a union type in the schema.
     *
     * @param unionName the union name
     * @param memberTypes the member type names
     * @param description optional description
     */
    public void registerUnion(String unionName, List<String> memberTypes, String description) {
        UnionInfo unionInfo = new UnionInfo(unionName, memberTypes, description);
        unions.put(unionName, unionInfo);
    }

    /**
     * Register an input type in the schema.
     *
     * @param inputName the input type name
     * @param fields the input fields
     * @param description optional description
     */
    public void registerInputType(String inputName, Map<String, TypeConverter.GraphQLFieldInfo> fields, String description) {
        InputTypeInfo inputInfo = new InputTypeInfo(inputName, fields, description);
        inputTypes.put(inputName, inputInfo);
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
     * Get an enum type by name.
     *
     * @param enumName the enum name
     * @return the EnumInfo or empty Optional if not found
     */
    public Optional<EnumInfo> getEnum(String enumName) {
        return Optional.ofNullable(enums.get(enumName));
    }

    /**
     * Get all registered enum types.
     *
     * @return unmodifiable map of enum name to EnumInfo
     */
    public Map<String, EnumInfo> getAllEnums() {
        return Collections.unmodifiableMap(enums);
    }

    /**
     * Get an interface type by name.
     *
     * @param interfaceName the interface name
     * @return the InterfaceInfo or empty Optional if not found
     */
    public Optional<InterfaceInfo> getInterface(String interfaceName) {
        return Optional.ofNullable(interfaces.get(interfaceName));
    }

    /**
     * Get all registered interface types.
     *
     * @return unmodifiable map of interface name to InterfaceInfo
     */
    public Map<String, InterfaceInfo> getAllInterfaces() {
        return Collections.unmodifiableMap(interfaces);
    }

    /**
     * Get a union type by name.
     *
     * @param unionName the union name
     * @return the UnionInfo or empty Optional if not found
     */
    public Optional<UnionInfo> getUnion(String unionName) {
        return Optional.ofNullable(unions.get(unionName));
    }

    /**
     * Get all registered union types.
     *
     * @return unmodifiable map of union name to UnionInfo
     */
    public Map<String, UnionInfo> getAllUnions() {
        return Collections.unmodifiableMap(unions);
    }

    /**
     * Get an input type by name.
     *
     * @param inputName the input type name
     * @return the InputTypeInfo or empty Optional if not found
     */
    public Optional<InputTypeInfo> getInputType(String inputName) {
        return Optional.ofNullable(inputTypes.get(inputName));
    }

    /**
     * Get all registered input types.
     *
     * @return unmodifiable map of input type name to InputTypeInfo
     */
    public Map<String, InputTypeInfo> getAllInputTypes() {
        return Collections.unmodifiableMap(inputTypes);
    }

    /**
     * Clear all registered types, queries, mutations, subscriptions, enums, interfaces, unions, and input types.
     * Useful for testing.
     */
    public void clear() {
        types.clear();
        queries.clear();
        mutations.clear();
        subscriptions.clear();
        enums.clear();
        interfaces.clear();
        unions.clear();
        inputTypes.clear();
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

    /**
     * Information about a registered GraphQL enum type.
     */
    public static class EnumInfo {
        public final String name;
        public final Map<String, String> values;
        public final String description;

        public EnumInfo(String name, Map<String, String> values, String description) {
            this.name = name;
            this.values = Collections.unmodifiableMap(new LinkedHashMap<>(values));
            this.description = description;
        }

        @Override
        public String toString() {
            return "EnumInfo{" +
                "name='" + name + '\'' +
                ", values=" + values.size() +
                '}';
        }
    }

    /**
     * Information about a registered GraphQL interface type.
     */
    public static class InterfaceInfo {
        public final String name;
        public final Map<String, TypeConverter.GraphQLFieldInfo> fields;
        public final String description;

        public InterfaceInfo(String name, Map<String, TypeConverter.GraphQLFieldInfo> fields, String description) {
            this.name = name;
            this.fields = Collections.unmodifiableMap(new LinkedHashMap<>(fields));
            this.description = description;
        }

        @Override
        public String toString() {
            return "InterfaceInfo{" +
                "name='" + name + '\'' +
                ", fields=" + fields.size() +
                '}';
        }
    }

    /**
     * Information about a registered GraphQL union type.
     */
    public static class UnionInfo {
        public final String name;
        public final List<String> memberTypes;
        public final String description;

        public UnionInfo(String name, List<String> memberTypes, String description) {
            this.name = name;
            this.memberTypes = Collections.unmodifiableList(new ArrayList<>(memberTypes));
            this.description = description;
        }

        @Override
        public String toString() {
            return "UnionInfo{" +
                "name='" + name + '\'' +
                ", memberTypes=" + memberTypes.size() +
                '}';
        }
    }

    /**
     * Information about a registered GraphQL input type.
     */
    public static class InputTypeInfo {
        public final String name;
        public final Map<String, TypeConverter.GraphQLFieldInfo> fields;
        public final String description;

        public InputTypeInfo(String name, Map<String, TypeConverter.GraphQLFieldInfo> fields, String description) {
            this.name = name;
            this.fields = Collections.unmodifiableMap(new LinkedHashMap<>(fields));
            this.description = description;
        }

        @Override
        public String toString() {
            return "InputTypeInfo{" +
                "name='" + name + '\'' +
                ", fields=" + fields.size() +
                '}';
        }
    }
}

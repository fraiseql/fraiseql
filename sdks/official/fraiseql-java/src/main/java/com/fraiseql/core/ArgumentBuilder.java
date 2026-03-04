package com.fraiseql.core;

import java.util.*;

/**
 * Builder for GraphQL query/mutation arguments with default values.
 * Supports typed arguments with optional defaults and descriptions.
 *
 * Example:
 * <pre>
 * ArgumentBuilder args = new ArgumentBuilder()
 *     .add("limit", "Int", 10, "Maximum items to return")
 *     .add("offset", "Int", 0, "Pagination offset")
 *     .add("filter", "String", null, "Optional search filter");
 *
 * FraiseQL.query("items")
 *     .returnType(Item.class)
 *     .withArguments(args.build())
 *     .register();
 * </pre>
 */
public class ArgumentBuilder {
    private final Map<String, ArgumentInfo> arguments = new LinkedHashMap<>();

    /**
     * Add an argument with type only.
     *
     * @param name the argument name
     * @param type the GraphQL type (Int, String, Boolean, Float, etc.)
     * @return this builder for chaining
     */
    public ArgumentBuilder add(String name, String type) {
        arguments.put(name, new ArgumentInfo(name, type, null, ""));
        return this;
    }

    /**
     * Add an argument with type and default value.
     *
     * @param name the argument name
     * @param type the GraphQL type
     * @param defaultValue the default value (null for optional without default)
     * @return this builder for chaining
     */
    public ArgumentBuilder add(String name, String type, Object defaultValue) {
        arguments.put(name, new ArgumentInfo(name, type, defaultValue, ""));
        return this;
    }

    /**
     * Add an argument with all parameters.
     *
     * @param name the argument name
     * @param type the GraphQL type
     * @param defaultValue the default value (null for no default)
     * @param description the argument description
     * @return this builder for chaining
     */
    public ArgumentBuilder add(String name, String type, Object defaultValue, String description) {
        arguments.put(name, new ArgumentInfo(name, type, defaultValue, description));
        return this;
    }

    /**
     * Get the built arguments map.
     * Suitable for use with query/mutation builders.
     *
     * @return unmodifiable map of argument name to type
     */
    public Map<String, String> build() {
        Map<String, String> result = new LinkedHashMap<>();
        for (ArgumentInfo arg : arguments.values()) {
            result.put(arg.name, arg.type);
        }
        return result;
    }

    /**
     * Get detailed argument information including defaults.
     *
     * @return map of argument name to full ArgumentInfo
     */
    public Map<String, ArgumentInfo> buildDetailed() {
        return new LinkedHashMap<>(arguments);
    }

    /**
     * Check if an argument has a default value.
     *
     * @param name the argument name
     * @return true if the argument has a default value
     */
    public boolean hasDefault(String name) {
        ArgumentInfo arg = arguments.get(name);
        return arg != null && arg.defaultValue != null;
    }

    /**
     * Get the default value for an argument.
     *
     * @param name the argument name
     * @return the default value or null if not set
     */
    public Object getDefault(String name) {
        ArgumentInfo arg = arguments.get(name);
        return arg != null ? arg.defaultValue : null;
    }

    /**
     * Get all arguments that have default values.
     *
     * @return list of ArgumentInfo with defaults
     */
    public List<ArgumentInfo> getArgumentsWithDefaults() {
        List<ArgumentInfo> result = new ArrayList<>();
        for (ArgumentInfo arg : arguments.values()) {
            if (arg.defaultValue != null) {
                result.add(arg);
            }
        }
        return result;
    }

    /**
     * Information about a GraphQL argument including default value.
     */
    public static class ArgumentInfo {
        public final String name;
        public final String type;
        public final Object defaultValue;
        public final String description;

        public ArgumentInfo(String name, String type, Object defaultValue, String description) {
            this.name = name;
            this.type = type;
            this.defaultValue = defaultValue;
            this.description = description;
        }

        /**
         * Check if this argument is optional (has a default or is nullable type).
         *
         * @return true if argument is optional
         */
        public boolean isOptional() {
            return defaultValue != null || type.endsWith("!");
        }

        @Override
        public String toString() {
            if (defaultValue != null) {
                return name + ": " + type + " = " + defaultValue;
            }
            return name + ": " + type;
        }
    }
}

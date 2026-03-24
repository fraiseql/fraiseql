package com.fraiseql.core;

/**
 * Three-state field wrapper for update mutation inputs.
 * Distinguishes between "not provided" (UNSET), "explicitly null", and "has value".
 *
 * <p>Usage:
 * <pre>
 * UpdateField&lt;String&gt; name = UpdateField.unset();   // field not sent
 * UpdateField&lt;String&gt; name = UpdateField.ofNull();   // explicitly set to null
 * UpdateField&lt;String&gt; name = UpdateField.of("Alice"); // set to a value
 * </pre>
 *
 * @param <T> the wrapped value type
 */
public sealed interface UpdateField<T> {

    /** Field was not provided in the request. */
    record Unset<T>() implements UpdateField<T> {}

    /** Field was explicitly set to null. */
    record Null<T>() implements UpdateField<T> {}

    /** Field was set to a concrete value. */
    record Value<T>(T value) implements UpdateField<T> {}

    /**
     * Create an UNSET sentinel (field not provided).
     *
     * @param <T> the value type
     * @return an Unset instance
     */
    static <T> UpdateField<T> unset() { return new Unset<>(); }

    /**
     * Create an explicit null value.
     *
     * @param <T> the value type
     * @return a Null instance
     */
    static <T> UpdateField<T> ofNull() { return new Null<>(); }

    /**
     * Wrap a concrete value.
     *
     * @param value the value to wrap
     * @param <T> the value type
     * @return a Value instance
     */
    static <T> UpdateField<T> of(T value) { return new Value<>(value); }

    /** Returns true if this field was not provided (UNSET). */
    default boolean isUnset() { return this instanceof Unset; }

    /** Returns true if this field was explicitly set to null. */
    default boolean isNull() { return this instanceof Null; }

    /** Returns true if this field holds a concrete value. */
    default boolean isValue() { return this instanceof Value; }
}

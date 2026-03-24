namespace FraiseQL.Models;

/// <summary>
/// Three-state field wrapper for update mutation inputs.
/// Distinguishes "not provided" (Unset) from "explicitly null" from "has value".
/// </summary>
/// <typeparam name="T">The underlying value type.</typeparam>
public readonly struct UpdateField<T>
{
    private readonly T _value;
    private readonly bool _isSet;
    private readonly bool _isNull;

    private UpdateField(T value, bool isSet, bool isNull)
    {
        _value = value;
        _isSet = isSet;
        _isNull = isNull;
    }

    /// <summary>Creates an unset field (not provided).</summary>
    /// <returns>An unset <see cref="UpdateField{T}"/>.</returns>
    public static UpdateField<T> Unset() => new(default!, false, false);

    /// <summary>Creates a field explicitly set to <c>null</c>.</summary>
    /// <returns>A null <see cref="UpdateField{T}"/>.</returns>
    public static UpdateField<T> Null() => new(default!, true, true);

    /// <summary>Creates a field set to the given value.</summary>
    /// <param name="value">The value to wrap.</param>
    /// <returns>A valued <see cref="UpdateField{T}"/>.</returns>
    public static UpdateField<T> Of(T value) => new(value, true, false);

    /// <summary>Gets whether this field was not provided (unset).</summary>
    public bool IsUnset => !_isSet;

    /// <summary>Gets whether this field was explicitly set to <c>null</c>.</summary>
    public bool IsNull => _isSet && _isNull;

    /// <summary>Gets whether this field has a concrete value.</summary>
    public bool IsValue => _isSet && !_isNull;

    /// <summary>Gets the value, or throws if not set to a value.</summary>
    /// <exception cref="InvalidOperationException">Thrown when the field is unset or null.</exception>
    public T Value => IsValue ? _value : throw new InvalidOperationException("Field is not set to a value");

    /// <summary>Gets the value if set, or returns the specified default.</summary>
    /// <param name="defaultValue">The default value to return if unset or null.</param>
    /// <returns>The value or the default.</returns>
    public T GetValueOrDefault(T defaultValue) => IsValue ? _value : defaultValue;
}

/// <summary>Static helper for creating <see cref="UpdateField{T}"/> instances.</summary>
public static class FraiseQLField
{
    /// <summary>Creates an unset field (not provided).</summary>
    /// <typeparam name="T">The underlying value type.</typeparam>
    /// <returns>An unset <see cref="UpdateField{T}"/>.</returns>
    public static UpdateField<T> Unset<T>() => UpdateField<T>.Unset();

    /// <summary>Creates a field explicitly set to <c>null</c>.</summary>
    /// <typeparam name="T">The underlying value type.</typeparam>
    /// <returns>A null <see cref="UpdateField{T}"/>.</returns>
    public static UpdateField<T> Null<T>() => UpdateField<T>.Null();

    /// <summary>Creates a field set to the given value.</summary>
    /// <typeparam name="T">The underlying value type.</typeparam>
    /// <param name="value">The value to wrap.</param>
    /// <returns>A valued <see cref="UpdateField{T}"/>.</returns>
    public static UpdateField<T> Of<T>(T value) => UpdateField<T>.Of(value);
}

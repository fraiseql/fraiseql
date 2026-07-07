using System.Reflection;
using FraiseQL.Attributes;

namespace FraiseQL.Registry;

/// <summary>
/// Maps C# property types to GraphQL scalar type names, respecting nullable annotations
/// and explicit <see cref="GraphQLFieldAttribute.Type"/> overrides.
/// </summary>
public static class TypeMapper
{
    /// <summary>
    /// Detects the GraphQL type name and nullability for a C# property.
    /// </summary>
    /// <param name="prop">The property to inspect.</param>
    /// <param name="attr">
    /// The <see cref="GraphQLFieldAttribute"/> applied to the property, or <see langword="null"/>
    /// if none is present.
    /// </param>
    /// <returns>
    /// A tuple of the GraphQL type name (e.g. <c>"String"</c>) and whether the field is nullable.
    /// </returns>
    public static (string GraphQLType, bool Nullable) Detect(
        PropertyInfo prop, GraphQLFieldAttribute? attr)
    {
        // Unwrap Nullable<T> for value types (e.g. int? → int)
        var underlyingType = Nullable.GetUnderlyingType(prop.PropertyType);
        var isNullableValueType = underlyingType != null;
        var baseType = underlyingType ?? prop.PropertyType;

        // Detect nullable reference types via NullabilityInfoContext (C# 8+ annotations)
        var nullabilityCtx = new NullabilityInfoContext();
        var nullabilityInfo = nullabilityCtx.Create(prop);
        var isNullableRef = nullabilityInfo.WriteState == NullabilityState.Nullable
                         || nullabilityInfo.ReadState == NullabilityState.Nullable;

        var isNullable = isNullableValueType || isNullableRef;

        // Explicit [GraphQLField(Type = "X")] always wins over auto-detection
        if (attr?.Type is { } explicitType)
            return (explicitType, attr.Nullable || isNullable);

        var graphqlType = MapBaseType(baseType);
        // attr.Nullable = true can force nullable; C# type system nullability always applies
        var nullable = isNullable || (attr?.Nullable ?? false);
        return (graphqlType, nullable);
    }

    /// <summary>
    /// Wire-transparent string representations of an identity: C# <c>string</c> maps to
    /// <c>"String"</c>, and the explicit <c>"UUID"</c> scalar both serialize as a JSON string,
    /// identical to GraphQL <c>ID</c>.
    /// </summary>
    private static readonly HashSet<string> StringShapedIdTypes = new(StringComparer.Ordinal)
    {
        "String",
        "UUID",
    };

    /// <summary>
    /// Enforces the entity-identity convention: a field named <c>id</c> is emitted as <c>ID</c>.
    /// </summary>
    /// <remarks>
    /// A global <c>id: ID!</c> is the identity contract the FraiseQL compiler enforces
    /// (Node / CascadeNode / federation <c>@key(fields: "id")</c> — see fraiseql-core ADR-0017).
    /// <c>string</c> and the explicit <c>UUID</c> scalar are wire-identical string representations
    /// of an identity, so an <c>id</c> typed either way is canonicalized to <c>ID</c> at authoring
    /// time — keeping the emitted <c>schema.json</c> honest instead of leaking <c>id: String</c>
    /// that the compiler would then reject. A numeric <c>id</c> (<c>"Int"</c>) is left unchanged
    /// (not wire-compatible with <c>ID</c>); the compiler flags it if the type opts into a
    /// Node-style interface, prompting a real <c>ID</c> identity.
    /// </remarks>
    /// <param name="fieldName">The GraphQL field name (camelCase).</param>
    /// <param name="graphqlType">The GraphQL type name detected for the field.</param>
    /// <returns><c>"ID"</c> when the field is a string-shaped <c>id</c>; otherwise the input type.</returns>
    public static string CanonicalizeIdType(string fieldName, string graphqlType) =>
        fieldName == "id" && StringShapedIdTypes.Contains(graphqlType)
            ? "ID"
            : graphqlType;

    /// <summary>Maps a non-nullable C# base type to its GraphQL scalar equivalent.</summary>
    /// <param name="baseType">The C# type to map.</param>
    /// <returns>The GraphQL scalar name. Defaults to <c>"String"</c> for unknown types.</returns>
    private static string MapBaseType(Type baseType)
    {
        if (baseType == typeof(int) || baseType == typeof(long) || baseType == typeof(short))
            return "Int";
        if (baseType == typeof(float) || baseType == typeof(double) || baseType == typeof(decimal))
            return "Float";
        if (baseType == typeof(bool))
            return "Boolean";
        if (baseType == typeof(Guid))
            return "ID";
        if (baseType == typeof(string))
            return "String";
        if (baseType == typeof(DateTime) || baseType == typeof(DateTimeOffset))
            return "String";

        // Fallback for unknown types
        return "String";
    }
}

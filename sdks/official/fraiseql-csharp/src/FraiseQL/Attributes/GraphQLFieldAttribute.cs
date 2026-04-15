namespace FraiseQL.Attributes;

/// <summary>
/// Marks a C# property as a GraphQL field for FraiseQL schema export.
/// Apply to properties on a class decorated with <see cref="GraphQLTypeAttribute"/>.
/// When <c>Type</c> is omitted, the GraphQL type is auto-detected from the C# property type.
/// </summary>
/// <example>
/// <code>
/// [GraphQLType(Name = "Author", SqlSource = "v_author")]
/// public class Author
/// {
///     [GraphQLField(Type = "ID", Nullable = false)]
///     public int Id { get; set; }
///
///     [GraphQLField(Nullable = true)]
///     public string? Bio { get; set; }
/// }
/// </code>
/// </example>
[AttributeUsage(AttributeTargets.Property, Inherited = false)]
public sealed class GraphQLFieldAttribute : Attribute
{
    /// <summary>
    /// Gets or sets the explicit GraphQL type name (e.g. <c>"ID"</c>, <c>"String"</c>, <c>"Int"</c>).
    /// When set, this overrides C# type auto-detection.
    /// When <see langword="null"/>, the type is inferred from the C# property type.
    /// </summary>
    public string? Type { get; set; }

    /// <summary>Gets or sets an optional human-readable field description.</summary>
    public string? Description { get; set; }

    /// <summary>
    /// Gets or sets whether this field is nullable in the GraphQL schema.
    /// For reference types, nullability is also inferred from C# nullable annotations (<c>T?</c>).
    /// </summary>
    public bool Nullable { get; set; } = false;

    /// <summary>Gets or sets an optional custom resolver name for computed fields.</summary>
    public string? Resolver { get; set; }

    /// <summary>Gets or sets a single required OAuth scope for field access.</summary>
    public string? Scope { get; set; }

    /// <summary>Gets or sets multiple required OAuth scopes (any one suffices) for field access.</summary>
    public string[]? Scopes { get; set; }

    /// <summary>
    /// Gets or sets whether this field is computed and should be excluded from CRUD input types.
    /// Computed fields are typically auto-generated (like slugs, timestamps, etc.)
    /// and should not be set directly by users in create/update operations.
    ///
    /// When <see langword="true"/>, the field will be excluded from:
    /// - <c>Create{TypeName}Input</c> types (all fields)
    /// - <c>Update{TypeName}Input</c> types (non-PK fields only)
    ///
    /// The field remains visible in query results.
    /// </summary>
    public bool Computed { get; set; } = false;
}

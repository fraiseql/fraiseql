namespace FraiseQL

open System

/// Marks a class or record as a GraphQL type for FraiseQL schema authoring.
/// Apply this attribute to any .NET type that should appear in schema.json.
///
/// Example:
/// <code>
/// [&lt;GraphQLType(Name = "Author", SqlSource = "v_author")&gt;]
/// type AuthorEntity() =
///     [&lt;GraphQLField(Nullable = false)&gt;]
///     member val Id: Guid = Guid.Empty with get, set
///     [&lt;GraphQLField(Nullable = false)&gt;]
///     member val Name: string = "" with get, set
/// </code>
[<AttributeUsage(AttributeTargets.Class ||| AttributeTargets.Struct, AllowMultiple = false, Inherited = false)>]
[<Sealed>]
type GraphQLTypeAttribute() =
    inherit Attribute()

    /// The GraphQL type name (PascalCase). Defaults to the .NET type name.
    member val Name: string = "" with get, set

    /// The SQL view or table backing this type. Required for schema compilation.
    member val SqlSource: string = "" with get, set

    /// Optional human-readable description exposed in GraphQL introspection.
    member val Description: string = "" with get, set

    /// When true, this type is a GraphQL input type (used as mutation arguments).
    member val IsInput: bool = false with get, set

    /// When true, this type participates in Relay-style cursor pagination.
    member val Relay: bool = false with get, set

    /// When true, this type models a mutation error response.
    member val IsError: bool = false with get, set

    /// When true, auto-generate CRUD queries and mutations for this type.
    member val Crud: bool = false with get, set

    /// When true, generated CRUD mutations include cascade support.
    member val Cascade: bool = false with get, set

/// Marks a property on a GraphQL type as a field to include in the schema.
/// Apply this attribute to properties on classes decorated with <see cref="GraphQLTypeAttribute"/>.
///
/// Example:
/// <code>
/// [&lt;GraphQLField(Nullable = false, Description = "Unique author identifier")&gt;]
/// member val Id: Guid = Guid.Empty with get, set
/// </code>
[<AttributeUsage(AttributeTargets.Property, AllowMultiple = false, Inherited = false)>]
[<Sealed>]
type GraphQLFieldAttribute() =
    inherit Attribute()

    /// Explicit GraphQL type override (e.g. "ID", "String"). When empty, the type
    /// is inferred automatically from the .NET property type via <c>TypeMapper</c>.
    member val Type: string = "" with get, set

    /// Whether this field may return null. Defaults to true (nullable).
    member val Nullable: bool = true with get, set

    /// Optional human-readable description exposed in GraphQL introspection.
    member val Description: string = "" with get, set

    /// Single scope/permission required to read this field.
    member val Scope: string = "" with get, set

    /// Multiple scopes/permissions (any one is sufficient) required to read this field.
    member val Scopes: string[] = [||] with get, set

    /// When true, marks the field as deprecated in GraphQL introspection.
    member val Deprecated: bool = false with get, set

    /// Human-readable reason for the deprecation, shown in introspection.
    member val DeprecationReason: string = "" with get, set

    /// When true, this field is server-computed and excluded from CRUD input types.
    ///
    /// Computed fields (e.g. auto-generated slugs, view aggregations) are never
    /// provided by the client, so they are omitted from Create{Type}Input and
    /// Update{Type}Input. They remain visible in query results.
    member val Computed: bool = false with get, set

    /// Vector width on a `Vector` / `BitVector` / `HalfVector` / `SparseVector` field:
    /// float components for the float kinds, bits for `BitVector`.
    ///
    /// The compiler refuses a vector field carrying no configuration, so this is what
    /// makes the four pgvector field types authorable. Zero — the default — means the
    /// field is not a vector field; a column with no dimensions is not a thing an author
    /// can mean.
    member val VectorDimensions: int = 0 with get, set

    /// The index this vector column is searched through: one of the `VectorIndex` values.
    /// Empty means the default, `hnsw`.
    member val VectorIndexType: string = "" with get, set

    /// The distance metric a search over this column orders by: one of the
    /// `VectorMetric` values. Empty means the default, `cosine`.
    member val VectorDistanceMetric: string = "" with get, set

    /// On a `Float` field, the vector field whose `nearest` search distance this field
    /// carries. Selecting it on a query that did not run that search is refused, not
    /// answered with null.
    member val VectorDistance: string = "" with get, set

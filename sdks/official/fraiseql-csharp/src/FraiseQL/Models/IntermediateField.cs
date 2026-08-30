using System.Text.Json.Serialization;

namespace FraiseQL.Models;

/// <summary>
/// Represents a single field on a GraphQL type in the intermediate schema format
/// consumed by <c>fraiseql compile</c>.
/// </summary>
/// <param name="Name">The GraphQL field name (camelCase).</param>
/// <param name="Type">The GraphQL scalar or type name.</param>
/// <param name="Nullable">Whether the field is nullable in the schema.</param>
/// <param name="Description">Optional description, omitted from JSON when <see langword="null"/>.</param>
/// <param name="Scope">Optional required scope, omitted from JSON when <see langword="null"/>.</param>
/// <param name="Scopes">Optional required scopes, omitted from JSON when <see langword="null"/>.</param>
/// <param name="Computed">
/// When <see langword="true"/>, the field is server-computed and excluded from CRUD input types.
/// Authoring-time only, and therefore never serialised (#1244): <c>CrudGenerator</c> reads it
/// to decide which fields to omit from the input types it generates, and that runs before
/// export. <c>IntermediateField</c> in the compiler has no <c>computed</c> member and denies
/// unknown fields, so emitting it made <c>fraiseql compile</c> refuse the whole document —
/// naming a key this SDK's own attribute documents. F# reached the same answer in
/// <c>Types.fs</c>; Python's is <c>registry.py::_build_field_def</c> (#927).
/// </param>
/// <param name="Vector">
/// pgvector configuration on a <c>Vector</c> / <c>BitVector</c> / <c>HalfVector</c> /
/// <c>SparseVector</c> field. The compiler refuses such a field without one, so dropping
/// it here would not be a silent loss — it would make the four pgvector field types
/// unauthorable in C#.
/// </param>
/// <param name="VectorDistance">
/// On a <c>Float</c> field, the vector field whose <c>nearest</c> search distance this
/// field carries. Selecting it on a query that did not run that search is refused, not
/// answered with null.
/// </param>
/// <param name="Deprecated">
/// Optional deprecation. When present the field surfaces as <c>isDeprecated</c> /
/// <c>deprecationReason</c> through introspection.
/// </param>
public record IntermediateField(
    [property: JsonPropertyName("name")]        string Name,
    [property: JsonPropertyName("type")]        string Type,
    [property: JsonPropertyName("nullable")]    bool Nullable,
    [property: JsonPropertyName("description")] string? Description = null,
    // The wire key is `requires_scope` — the key the compiler reads. It was `scope`,
    // which binds to nothing, so a field the author gated with [GraphQLField(Scope=...)]
    // compiled with no scope at all and was served to callers holding none (#807).
    [property: JsonPropertyName("requires_scope")] string? Scope = null,
    // Multiple required scopes have no representation in the compiled schema or the
    // runtime field filter, which check exactly one `requires_scope`. The property is
    // retained for source compatibility but is never serialised: emitting it produced a
    // key nothing reads. A singleton list is normalised onto Scope by SchemaRegistry.
    [property: JsonIgnore]                      IReadOnlyList<string>? Scopes = null,
    [property: JsonIgnore]                      bool? Computed = null,
    [property: JsonPropertyName("vector_config")]   VectorConfig? Vector = null,
    [property: JsonPropertyName("vector_distance")] string? VectorDistance = null,
    // `IntermediateField.deprecated` has been readable since #1025. There was no
    // attribute to put a reason in, so a C# author could not deprecate a field at all.
    [property: JsonPropertyName("deprecated")]      DeprecationInfo? Deprecated = null);

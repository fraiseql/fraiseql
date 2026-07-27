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
/// <param name="Resolver">Optional resolver name, omitted from JSON when <see langword="null"/>.</param>
/// <param name="Scope">Optional required scope, omitted from JSON when <see langword="null"/>.</param>
/// <param name="Scopes">Optional required scopes, omitted from JSON when <see langword="null"/>.</param>
/// <param name="Computed">
/// When <see langword="true"/>, the field is server-computed and excluded from CRUD input types.
/// Omitted from JSON when <see langword="null"/> (the default) to keep the schema compact.
/// </param>
public record IntermediateField(
    [property: JsonPropertyName("name")]        string Name,
    [property: JsonPropertyName("type")]        string Type,
    [property: JsonPropertyName("nullable")]    bool Nullable,
    [property: JsonPropertyName("description")] string? Description = null,
    [property: JsonPropertyName("resolver")]    string? Resolver = null,
    // The wire key is `requires_scope` — the key the compiler reads. It was `scope`,
    // which binds to nothing, so a field the author gated with [GraphQLField(Scope=...)]
    // compiled with no scope at all and was served to callers holding none (#807).
    [property: JsonPropertyName("requires_scope")] string? Scope = null,
    // Multiple required scopes have no representation in the compiled schema or the
    // runtime field filter, which check exactly one `requires_scope`. The property is
    // retained for source compatibility but is never serialised: emitting it produced a
    // key nothing reads. A singleton list is normalised onto Scope by SchemaRegistry.
    [property: JsonIgnore]                      IReadOnlyList<string>? Scopes = null,
    [property: JsonPropertyName("computed")]    bool? Computed = null);

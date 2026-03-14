using System.Text.Json.Serialization;

namespace FraiseQL.Models;

/// <summary>
/// Represents a GraphQL query in the intermediate schema format consumed by <c>fraiseql compile</c>.
/// All keys are snake_case in the JSON output. Optional fields are omitted when <see langword="null"/>.
/// </summary>
/// <param name="Name">The query name.</param>
/// <param name="ReturnType">The GraphQL return type name.</param>
/// <param name="ReturnsList">Whether the query returns a list.</param>
/// <param name="Nullable">Whether the query result may be <c>null</c>.</param>
/// <param name="SqlSource">The backing SQL view.</param>
/// <param name="Arguments">Ordered list of query arguments (always present, empty array if none).</param>
/// <param name="CacheTtlSeconds">Optional cache TTL in seconds, omitted from JSON when <see langword="null"/>.</param>
/// <param name="Description">Optional description, omitted from JSON when <see langword="null"/>.</param>
public record IntermediateQuery(
    [property: JsonPropertyName("name")]              string Name,
    [property: JsonPropertyName("return_type")]       string ReturnType,
    [property: JsonPropertyName("returns_list")]      bool ReturnsList,
    [property: JsonPropertyName("nullable")]          bool Nullable,
    [property: JsonPropertyName("sql_source")]        string SqlSource,
    [property: JsonPropertyName("arguments")]         IReadOnlyList<IntermediateArgument> Arguments,
    [property: JsonPropertyName("cache_ttl_seconds")] int? CacheTtlSeconds = null,
    [property: JsonPropertyName("description")]       string? Description = null,
    [property: JsonPropertyName("rest")]              RestAnnotation? Rest = null);

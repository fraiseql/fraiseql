using System.Text.Json.Serialization;

namespace FraiseQL.Models;

/// <summary>
/// Represents a GraphQL type in the intermediate schema format consumed by <c>fraiseql compile</c>.
/// </summary>
/// <param name="Name">The GraphQL type name.</param>
/// <param name="SqlSource">The backing SQL view or function.</param>
/// <param name="Description">Optional description, omitted from JSON when <see langword="null"/>.</param>
/// <param name="Fields">Ordered list of fields on this type.</param>
/// <param name="TenantScoped">Whether this type is tenant-scoped, omitted from JSON when <see langword="null"/>.</param>
public record IntermediateType(
    [property: JsonPropertyName("name")]           string Name,
    [property: JsonPropertyName("sql_source")]     string SqlSource,
    [property: JsonPropertyName("description")]    string? Description,
    [property: JsonPropertyName("fields")]         IReadOnlyList<IntermediateField> Fields,
    [property: JsonPropertyName("tenant_scoped")]  bool? TenantScoped = null);

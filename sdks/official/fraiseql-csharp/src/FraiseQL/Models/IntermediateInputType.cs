using System.Text.Json.Serialization;

namespace FraiseQL.Models;

/// <summary>
/// Represents a GraphQL input object type in the intermediate schema format
/// consumed by <c>fraiseql compile</c>.
/// </summary>
/// <param name="Name">The input type name (e.g. <c>"CreateUserInput"</c>).</param>
/// <param name="Fields">Ordered list of fields on this input type.</param>
/// <param name="Description">Optional description, omitted from JSON when <see langword="null"/>.</param>
public record IntermediateInputType(
    [property: JsonPropertyName("name")]        string Name,
    [property: JsonPropertyName("fields")]      IReadOnlyList<IntermediateInputField> Fields,
    [property: JsonPropertyName("description")] string? Description = null);

/// <summary>
/// Represents a single field on a GraphQL input type in the intermediate schema format.
/// </summary>
/// <param name="Name">The field name.</param>
/// <param name="Type">The GraphQL type name.</param>
/// <param name="Nullable">Whether the field accepts <c>null</c>.</param>
public record IntermediateInputField(
    [property: JsonPropertyName("name")]     string Name,
    [property: JsonPropertyName("type")]     string Type,
    [property: JsonPropertyName("nullable")] bool Nullable);

using System.Text.Json.Serialization;

namespace FraiseQL.Models;

/// <summary>
/// Represents a GraphQL enum type in the intermediate schema format consumed by
/// <c>fraiseql compile</c>.
/// </summary>
/// <param name="Name">The enum type name (e.g. <c>"OrderStatus"</c>).</param>
/// <param name="Values">Ordered enum members.</param>
/// <param name="Description">Optional description, omitted from JSON when <see langword="null"/>.</param>
public record IntermediateEnum(
    [property: JsonPropertyName("name")]        string Name,
    [property: JsonPropertyName("values")]      IReadOnlyList<IntermediateEnumValue> Values,
    [property: JsonPropertyName("description")] string? Description = null);

/// <summary>
/// A single member of a GraphQL enum.
/// </summary>
/// <param name="Name">The member name, as it appears in a GraphQL document.</param>
/// <param name="Value">What the member is stored as, a string or an integer, when it is not
/// its name (#1611); omitted from JSON when <see langword="null"/>.</param>
/// <param name="Description">Optional description, omitted from JSON when <see langword="null"/>.</param>
public record IntermediateEnumValue(
    [property: JsonPropertyName("name")]        string Name,
    [property: JsonPropertyName("value")]       object? Value = null,
    [property: JsonPropertyName("description")] string? Description = null);

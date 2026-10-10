using System.Text.Json.Serialization;

namespace FraiseQL.Models;

/// <summary>
/// Represents a single argument on a GraphQL query or mutation,
/// serialized to the <c>schema.json</c> intermediate format consumed by <c>fraiseql compile</c>.
/// </summary>
/// <param name="Name">The argument name.</param>
/// <param name="Type">The GraphQL type name (e.g. <c>"ID"</c>, <c>"String"</c>).</param>
/// <param name="Nullable">Whether the argument accepts <c>null</c>.</param>
/// <param name="Localized">
/// <see langword="true"/> on a String received as a locale map (#1527); omitted otherwise.
/// </param>
public record IntermediateArgument(
    [property: JsonPropertyName("name")]      string Name,
    [property: JsonPropertyName("type")]      string Type,
    [property: JsonPropertyName("nullable")]  bool Nullable,
    [property: JsonPropertyName("localized")] bool? Localized = null);

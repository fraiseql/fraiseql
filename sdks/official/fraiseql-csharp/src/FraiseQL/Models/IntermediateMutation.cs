using System.Text.Json.Serialization;

namespace FraiseQL.Models;

/// <summary>
/// Represents a GraphQL mutation in the intermediate schema format consumed by <c>fraiseql compile</c>.
/// All keys are snake_case in the JSON output. Optional fields are omitted when <see langword="null"/>.
/// </summary>
/// <param name="Name">The mutation name.</param>
/// <param name="ReturnType">The GraphQL return type name.</param>
/// <param name="SqlSource">The backing SQL function (e.g. <c>fn_create_author</c>).</param>
/// <param name="Operation">The operation kind: <c>"insert"</c>, <c>"update"</c>, <c>"delete"</c>, or <c>"upsert"</c>.</param>
/// <param name="Arguments">Ordered list of mutation arguments (always present, empty array if none).</param>
/// <param name="Description">Optional description, omitted from JSON when <see langword="null"/>.</param>
public record IntermediateMutation(
    [property: JsonPropertyName("name")]        string Name,
    [property: JsonPropertyName("return_type")] string ReturnType,
    [property: JsonPropertyName("sql_source")]  string SqlSource,
    [property: JsonPropertyName("operation")]   string Operation,
    [property: JsonPropertyName("arguments")]   IReadOnlyList<IntermediateArgument> Arguments,
    [property: JsonPropertyName("description")] string? Description = null,
    [property: JsonPropertyName("cascade")]     bool? Cascade = null);

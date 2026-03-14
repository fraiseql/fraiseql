using System.Text.Json.Serialization;

namespace FraiseQL.Models;

/// <summary>
/// Optional REST transport annotation for a query or mutation.
/// When set, the compiler emits <c>"rest": {"path": "...", "method": "..."}</c> in the schema JSON.
/// </summary>
/// <param name="Path">The REST path template, e.g. <c>/users/{id}</c>.</param>
/// <param name="Method">The HTTP method, e.g. <c>POST</c>.</param>
public record RestAnnotation(
    [property: JsonPropertyName("path")]   string Path,
    [property: JsonPropertyName("method")] string Method);

/// <summary>
/// Represents a GraphQL mutation in the intermediate schema format consumed by <c>fraiseql compile</c>.
/// All keys are snake_case in the JSON output. Optional fields are omitted when <see langword="null"/>.
/// </summary>
/// <param name="Name">The mutation name.</param>
/// <param name="ReturnType">The GraphQL return type name.</param>
/// <param name="SqlSource">The backing SQL function (e.g. <c>fn_create_author</c>).</param>
/// <param name="Operation">The operation kind: <c>"CREATE"</c>, <c>"UPDATE"</c>, <c>"DELETE"</c>, or <c>"CUSTOM"</c>.</param>
/// <param name="Arguments">Ordered list of mutation arguments (always present, empty array if none).</param>
/// <param name="Description">Optional description, omitted from JSON when <see langword="null"/>.</param>
/// <param name="Rest">Optional REST transport annotation, omitted from JSON when <see langword="null"/>.</param>
public record IntermediateMutation(
    [property: JsonPropertyName("name")]        string Name,
    [property: JsonPropertyName("return_type")] string ReturnType,
    [property: JsonPropertyName("sql_source")]  string SqlSource,
    [property: JsonPropertyName("operation")]   string Operation,
    [property: JsonPropertyName("arguments")]   IReadOnlyList<IntermediateArgument> Arguments,
    [property: JsonPropertyName("description")] string? Description = null,
    [property: JsonPropertyName("rest")]        RestAnnotation? Rest = null);

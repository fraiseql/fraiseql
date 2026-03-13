using System.Text.Json.Serialization;

namespace FraiseQL.Models;

/// <summary>
/// The top-level intermediate schema document produced by <see cref="FraiseQL.Export.SchemaExporter"/>.
/// This is the JSON format consumed by the <c>fraiseql compile</c> Rust CLI.
/// </summary>
/// <param name="Version">Schema format version, always <c>"2.0.0"</c>.</param>
/// <param name="Types">Registered GraphQL types.</param>
/// <param name="Queries">Registered GraphQL queries.</param>
/// <param name="Mutations">Registered GraphQL mutations.</param>
public record IntermediateSchema(
    [property: JsonPropertyName("version")]   string Version,
    [property: JsonPropertyName("types")]     IReadOnlyList<IntermediateType> Types,
    [property: JsonPropertyName("queries")]   IReadOnlyList<IntermediateQuery> Queries,
    [property: JsonPropertyName("mutations")] IReadOnlyList<IntermediateMutation> Mutations);

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
/// <param name="Federation">Optional federation metadata block, omitted from JSON when <see langword="null"/>.</param>
public record IntermediateSchema(
    [property: JsonPropertyName("version")]    string Version,
    [property: JsonPropertyName("types")]      IReadOnlyList<IntermediateType> Types,
    [property: JsonPropertyName("queries")]    IReadOnlyList<IntermediateQuery> Queries,
    [property: JsonPropertyName("mutations")]  IReadOnlyList<IntermediateMutation> Mutations,
    [property: JsonPropertyName("federation")] FederationBlock? Federation = null);

/// <summary>
/// Top-level federation metadata block emitted in the schema JSON when federation is enabled.
/// </summary>
/// <param name="Enabled">Always <see langword="true"/> when present.</param>
/// <param name="ServiceName">Logical name of this subgraph (e.g. "users-service").</param>
/// <param name="ApolloVersion">Apollo Federation spec version (1 or 2).</param>
/// <param name="Entities">List of entity descriptors with name and key fields.</param>
public record FederationBlock(
    [property: JsonPropertyName("enabled")]        bool Enabled,
    [property: JsonPropertyName("service_name")]   string ServiceName,
    [property: JsonPropertyName("apollo_version")] int ApolloVersion,
    [property: JsonPropertyName("entities")]       IReadOnlyList<FederationEntity> Entities);

/// <summary>
/// Describes a single federation entity type and its key fields.
/// </summary>
/// <param name="Name">The GraphQL type name.</param>
/// <param name="KeyFields">Fields used for entity resolution.</param>
public record FederationEntity(
    [property: JsonPropertyName("name")]       string Name,
    [property: JsonPropertyName("key_fields")] IReadOnlyList<string> KeyFields);

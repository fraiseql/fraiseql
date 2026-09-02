using System.Text.Json.Serialization;

namespace FraiseQL.Models;

/// <summary>
/// Represents a GraphQL type in the intermediate schema format consumed by <c>fraiseql compile</c>.
/// </summary>
/// <remarks>
/// <para>
/// <see cref="Relay"/> and <see cref="IsError"/> exist because the registry captures both
/// off <c>[GraphQLType]</c> and this record had nowhere to put them, so the exporter
/// discarded them with no diagnostic on either side — it had no field to warn about, and
/// the compiler never saw the key (#849).
/// </para>
/// <para>
/// Both are load-bearing. <c>relay</c> drives <c>Node</c>-interface synthesis and
/// base64 global IDs, so a Relay client got raw UUIDs where it expected opaque cursors;
/// <c>is_error</c> marks a type as a mutation error variant, so error-union synthesis
/// skipped it and the mutation returned the error as an ordinary payload. Both compiled
/// cleanly and were silently wrong at runtime.
/// </para>
/// <para>
/// They are nullable so <c>DefaultIgnoreCondition = WhenWritingNull</c> omits them when
/// unset, keeping the document byte-identical for a schema that uses neither.
/// </para>
/// </remarks>
/// <param name="Name">The GraphQL type name.</param>
/// <param name="SqlSource">The backing SQL view or function.</param>
/// <param name="Description">Optional description, omitted from JSON when <see langword="null"/>.</param>
/// <param name="Fields">Ordered list of fields on this type.</param>
/// <param name="Relay">Whether this type implements the Relay <c>Node</c> interface.</param>
/// <param name="IsError">Whether this type is a mutation error variant.</param>
/// <param name="Relationships">
/// Relationships followed by REST resource embedding (#1266). Nullable for the same
/// reason as the two flags above: <c>WhenWritingNull</c> omits it, so a type declaring
/// none is byte-identical to pre-#1266 output.
/// </param>
public record IntermediateType(
    [property: JsonPropertyName("name")]          string Name,
    [property: JsonPropertyName("sql_source")]    string SqlSource,
    [property: JsonPropertyName("description")]   string? Description,
    [property: JsonPropertyName("fields")]        IReadOnlyList<IntermediateField> Fields,
    [property: JsonPropertyName("relay")]         bool? Relay = null,
    [property: JsonPropertyName("is_error")]      bool? IsError = null,
    [property: JsonPropertyName("relationships")] IReadOnlyList<RelationshipDefinition>? Relationships = null);

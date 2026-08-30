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
/// <param name="Cascade">When <see langword="true"/>, enables cascade deletion. Omitted from JSON when <see langword="null"/>.</param>
/// <param name="Rest">Optional REST endpoint annotation, omitted from JSON when <see langword="null"/>.</param>
/// <param name="InjectParams">
/// Server-injected parameters, keyed by SQL parameter name. Values are <c>"jwt:&lt;claim&gt;"</c>.
/// </param>
/// <param name="RequiresRole">Role required to execute this mutation.</param>
/// <param name="RequiresActor">Actor types allowed to execute this mutation (#966).</param>
/// <param name="InvalidatesViews">
/// Views whose cached query results must be invalidated after this mutation succeeds.
/// Without it a mutation and the cached reads of what it wrote have no connection, and a
/// newly written row stays invisible for the whole of the reader's TTL.
/// </param>
/// <param name="InvalidatesFactTables">
/// Fact tables whose cached aggregates must be invalidated after this mutation succeeds.
/// Unlike views there is no inference fallback: an aggregate is only ever invalidated from
/// this list.
/// </param>
public record IntermediateMutation(
    [property: JsonPropertyName("name")]        string Name,
    [property: JsonPropertyName("return_type")] string ReturnType,
    [property: JsonPropertyName("sql_source")]  string SqlSource,
    [property: JsonPropertyName("operation")]   string Operation,
    [property: JsonPropertyName("arguments")]   IReadOnlyList<IntermediateArgument> Arguments,
    [property: JsonPropertyName("description")] string? Description = null,
    [property: JsonPropertyName("cascade")]     bool? Cascade = null,
    [property: JsonPropertyName("rest")]        RestAnnotation? Rest = null,
    [property: JsonPropertyName("inject_params")]           IReadOnlyDictionary<string, string>? InjectParams = null,
    [property: JsonPropertyName("requires_role")]           string? RequiresRole = null,
    // See IntermediateQuery.RequiresActor.
    [property: JsonPropertyName("requires_actor")]          IReadOnlyList<string>? RequiresActor = null,
    [property: JsonPropertyName("invalidates_views")]       IReadOnlyList<string>? InvalidatesViews = null,
    [property: JsonPropertyName("invalidates_fact_tables")] IReadOnlyList<string>? InvalidatesFactTables = null);

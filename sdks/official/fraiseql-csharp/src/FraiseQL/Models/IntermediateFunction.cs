using System.Text.Json.Serialization;

namespace FraiseQL.Models;

/// <summary>
/// A serverless function definition (#1325) — an out-of-band handler the server
/// dispatches on a trigger and runs in a WASM or Deno sandbox.
/// </summary>
/// <remarks>
/// <paramref name="Name"/> is also the module <em>file stem</em>: the server loads
/// <c>&lt;module_dir&gt;/&lt;name&gt;.&lt;ext&gt;</c>, so it is carried verbatim and never
/// recased. <c>module_dir</c> and <c>dlq_store</c> are deliberately absent — they are
/// deployment settings owned by <c>[functions]</c> in <c>fraiseql.toml</c>.
/// </remarks>
/// <param name="Name">Function name, and the module file stem.</param>
/// <param name="Trigger">e.g. <c>after:mutation:Order:update</c>; <c>after:mutation</c> matches the mutation's return type.</param>
/// <param name="Runtime"><c>Deno</c> or <c>Wasm</c>.</param>
/// <param name="TimeoutMs">Timeout override in milliseconds, omitted when unset.</param>
/// <param name="When"><c>when</c> predicates (#597), omitted when empty.</param>
/// <param name="ReRunnable">Opt out of durable dispatch, omitted when false.</param>
public record IntermediateFunction(
    [property: JsonPropertyName("name")]        string Name,
    [property: JsonPropertyName("trigger")]     string Trigger,
    [property: JsonPropertyName("runtime")]     string Runtime,
    [property: JsonPropertyName("timeout_ms")]  int? TimeoutMs = null,
    [property: JsonPropertyName("when")]        IReadOnlyList<IntermediateFunctionPredicate>? When = null,
    [property: JsonPropertyName("re_runnable")] bool? ReRunnable = null);

/// <summary>
/// One <c>when</c> conjunct (#597) the dispatcher evaluates on the row images before
/// firing. Exactly one operator is set; <c>changed_to</c> is UPDATE-only.
/// </summary>
/// <param name="Field">The field in the row image to test.</param>
/// <param name="Eq">The value the field must currently equal.</param>
/// <param name="ChangedTo">The value the field must have changed to.</param>
public record IntermediateFunctionPredicate(
    [property: JsonPropertyName("field")]      string Field,
    [property: JsonPropertyName("eq")]         object? Eq = null,
    [property: JsonPropertyName("changed_to")] object? ChangedTo = null);

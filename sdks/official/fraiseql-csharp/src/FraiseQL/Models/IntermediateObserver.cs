using System.Text.Json.Serialization;

namespace FraiseQL.Models;

/// <summary>An observer (#2147): actions run when a row of an entity changes.</summary>
/// <param name="Name">The observer name, unique within a schema.</param>
/// <param name="Entity">The type whose rows are observed.</param>
/// <param name="Event"><c>INSERT</c>, <c>UPDATE</c> or <c>DELETE</c>.</param>
/// <param name="Condition">An expression on the row; the actions run only when it holds.</param>
/// <param name="Actions">Each action's keys, as the server's observer runtime reads them.</param>
/// <param name="Retry">How a failed action is retried; <see langword="null"/>, the server's own policy.</param>
public record IntermediateObserver(
    [property: JsonPropertyName("name")]      string Name,
    [property: JsonPropertyName("entity")]    string Entity,
    [property: JsonPropertyName("event")]     string Event,
    [property: JsonPropertyName("condition")] string? Condition,
    [property: JsonPropertyName("actions")]   IReadOnlyList<IReadOnlyDictionary<string, object>> Actions,
    [property: JsonPropertyName("retry")]     IntermediateObserverRetry? Retry);

/// <summary>An observer's retry policy, as the compiler reads it.</summary>
/// <param name="MaxAttempts">The attempts, the first included.</param>
/// <param name="BackoffStrategy"><c>fixed</c>, <c>linear</c> or <c>exponential</c>.</param>
/// <param name="InitialDelayMs">The delay before the first retry.</param>
/// <param name="MaxDelayMs">The longest delay between retries.</param>
public record IntermediateObserverRetry(
    [property: JsonPropertyName("max_attempts")]     int MaxAttempts,
    [property: JsonPropertyName("backoff_strategy")] string BackoffStrategy,
    [property: JsonPropertyName("initial_delay_ms")] int InitialDelayMs,
    [property: JsonPropertyName("max_delay_ms")]     int MaxDelayMs);

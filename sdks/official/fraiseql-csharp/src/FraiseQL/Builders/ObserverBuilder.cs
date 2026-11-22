using FraiseQL.Models;
using FraiseQL.Registry;

namespace FraiseQL.Builders;

/// <summary>
/// Fluent builder for an observer (#2147): actions run when a row of an entity is
/// inserted, updated or deleted.
/// </summary>
/// <example>
/// <code>
/// ObserverBuilder.Observer("orderShipped")
///     .Entity("Order")
///     .Event("UPDATE")
///     .Condition("status == 'shipped'")
///     .Action(ObserverAction.Slack("#orders", "Order {{ id }} shipped"))
///     .Register();
/// </code>
/// </example>
public sealed class ObserverBuilder
{
    private readonly string _name;
    private string _entity = string.Empty;
    private string _event = string.Empty;
    private string? _condition;
    private readonly List<ObserverAction> _actions = new();
    private RetryConfig? _retry;

    private ObserverBuilder(string name) => _name = name;

    /// <summary>Starts building an observer with the given name.</summary>
    /// <param name="name">The observer name, unique within a schema.</param>
    /// <returns>A new builder.</returns>
    public static ObserverBuilder Observer(string name) => new(name);

    /// <summary>Sets the type whose rows are observed.</summary>
    /// <param name="entity">The type name.</param>
    /// <returns>This builder.</returns>
    public ObserverBuilder Entity(string entity) { _entity = entity; return this; }

    /// <summary>Sets the event: <c>INSERT</c>, <c>UPDATE</c> or <c>DELETE</c>.</summary>
    /// <param name="event">The event.</param>
    /// <returns>This builder.</returns>
    public ObserverBuilder Event(string @event) { _event = @event; return this; }

    /// <summary>Sets an expression on the row; the actions run only when it holds.</summary>
    /// <param name="condition">The expression.</param>
    /// <returns>This builder.</returns>
    public ObserverBuilder Condition(string condition) { _condition = condition; return this; }

    /// <summary>Adds an action.</summary>
    /// <param name="action">The action.</param>
    /// <returns>This builder.</returns>
    public ObserverBuilder Action(ObserverAction action) { _actions.Add(action); return this; }

    /// <summary>Sets how a failed action is retried; with none, the server's own policy applies.</summary>
    /// <param name="retry">The retry policy.</param>
    /// <returns>This builder.</returns>
    public ObserverBuilder Retry(RetryConfig retry) { _retry = retry; return this; }

    /// <summary>Builds the definition without registering it.</summary>
    /// <returns>The observer definition.</returns>
    public IntermediateObserver Build() => new(
        _name,
        _entity,
        _event,
        _condition,
        _actions.Select(a => a.Config).ToList().AsReadOnly(),
        _retry is null
            ? null
            : new IntermediateObserverRetry(
                _retry.MaxAttempts, _retry.BackoffStrategy, _retry.InitialDelayMs, _retry.MaxDelayMs));

    /// <summary>Builds the definition and registers it with the schema registry.</summary>
    /// <returns>The registered observer definition.</returns>
    /// <exception cref="InvalidOperationException">An observer of this name is already registered.</exception>
    public IntermediateObserver Register()
    {
        var observer = Build();
        SchemaRegistry.Instance.RegisterObserver(observer);
        return observer;
    }
}

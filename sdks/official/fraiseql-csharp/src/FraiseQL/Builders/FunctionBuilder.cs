using FraiseQL.Models;
using FraiseQL.Registry;

namespace FraiseQL.Builders;

/// <summary>
/// Fluent builder for a serverless function definition (#1325) — an out-of-band handler
/// the server dispatches on a trigger and runs in a WASM or Deno sandbox.
/// </summary>
/// <remarks>
/// The name is the module <em>file stem</em> as well as the function's identity: the
/// server loads <c>&lt;module_dir&gt;/&lt;name&gt;.&lt;ext&gt;</c>, so pass it exactly as
/// the file is named and expect no recasing. <c>module_dir</c> and <c>dlq_store</c> are
/// not builder options — they are deployment settings owned by <c>[functions]</c> in
/// <c>fraiseql.toml</c>.
/// </remarks>
/// <example>
/// <code>
/// FunctionBuilder.Function("notify_approved")
///     .Trigger("after:mutation:Order:update")
///     .TimeoutMs(2000)
///     .WhenChangedTo("status", "approved")
///     .Register();
/// </code>
/// </example>
public sealed class FunctionBuilder
{
    private readonly string _name;
    private string _trigger = string.Empty;
    private string _runtime = "Deno";
    private int? _timeoutMs;
    private readonly List<IntermediateFunctionPredicate> _when = new();
    private bool _reRunnable;

    private FunctionBuilder(string name) => _name = name;

    /// <summary>Starts building a function with the given name and module file stem.</summary>
    /// <param name="name">The function name, which is also the module file stem.</param>
    /// <returns>A new builder.</returns>
    public static FunctionBuilder Function(string name) => new(name);

    /// <summary>
    /// Sets the trigger, e.g. <c>after:mutation:Order:update</c>. <c>after:mutation</c>
    /// matches the mutation's <em>return type</em>, not its name.
    /// </summary>
    /// <param name="trigger">The trigger string.</param>
    /// <returns>This builder.</returns>
    public FunctionBuilder Trigger(string trigger)
    {
        _trigger = trigger;
        return this;
    }

    /// <summary>Sets the sandbox that runs the module: <c>Deno</c> (default) or <c>Wasm</c>.</summary>
    /// <param name="runtime">The runtime name.</param>
    /// <returns>This builder.</returns>
    public FunctionBuilder Runtime(string runtime)
    {
        _runtime = runtime;
        return this;
    }

    /// <summary>Overrides the trigger's default timeout.</summary>
    /// <param name="milliseconds">The timeout in milliseconds.</param>
    /// <returns>This builder.</returns>
    public FunctionBuilder TimeoutMs(int milliseconds)
    {
        _timeoutMs = milliseconds;
        return this;
    }

    /// <summary>Adds a <c>when</c> predicate (#597) testing that a field currently equals a value.</summary>
    /// <param name="field">The field in the row image.</param>
    /// <param name="value">The value it must equal.</param>
    /// <returns>This builder.</returns>
    public FunctionBuilder WhenEquals(string field, object value)
    {
        _when.Add(new IntermediateFunctionPredicate(field, Eq: value));
        return this;
    }

    /// <summary>
    /// Adds a <c>when</c> predicate (#597) testing that a field <em>changed to</em> a
    /// value. UPDATE-only.
    /// </summary>
    /// <param name="field">The field in the row image.</param>
    /// <param name="value">The value it changed to.</param>
    /// <returns>This builder.</returns>
    public FunctionBuilder WhenChangedTo(string field, object value)
    {
        _when.Add(new IntermediateFunctionPredicate(field, ChangedTo: value));
        return this;
    }

    /// <summary>Opts out of durable dispatch for work that is safe to re-run.</summary>
    /// <returns>This builder.</returns>
    public FunctionBuilder ReRunnable()
    {
        _reRunnable = true;
        return this;
    }

    /// <summary>Builds the definition without registering it.</summary>
    /// <returns>The function definition.</returns>
    public IntermediateFunction Build() => new(
        _name,
        _trigger,
        _runtime,
        _timeoutMs,
        _when.Count > 0 ? _when.AsReadOnly() : null,
        _reRunnable ? true : null);

    /// <summary>Builds the definition and registers it with the schema registry.</summary>
    /// <returns>The registered function definition.</returns>
    public IntermediateFunction Register()
    {
        var function = Build();
        SchemaRegistry.Instance.RegisterFunction(function);
        return function;
    }
}

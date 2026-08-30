namespace FraiseQL;

/// <summary>
/// The actor roster #966's actor gate is an allow-list of.
/// </summary>
/// <remarks>
/// snake_case, as the compiler spells it
/// (<c>crates/fraiseql-core/src/security/actor_type.rs</c>) and as the change-log
/// <c>actor_type TEXT</c> column stores it.
/// </remarks>
public static class ActorType
{
    /// <summary>A human end user — the default classification for an ordinary user JWT.</summary>
    public const string HumanUser = "human_user";

    /// <summary>A non-human service account: an API key, or a token carrying the scope.</summary>
    public const string ServiceAccount = "service_account";

    /// <summary>An autonomous agent acting for a user, via an RFC 8693 <c>act</c> claim.</summary>
    public const string AiAgent = "ai_agent";

    /// <summary>An internal scheduled or system-triggered job. Never token-derived.</summary>
    public const string SystemJob = "system_job";

    /// <summary>Every valid token, in declaration order.</summary>
    public static readonly IReadOnlyList<string> All =
        new[] { HumanUser, ServiceAccount, AiAgent, SystemJob };

    /// <summary>
    /// Check an allow-list where the author wrote it.
    /// </summary>
    /// <remarks>
    /// The compiler refuses an unknown token by name, but only at compile time, and this is
    /// a security gate enforced in the same executor arm as <c>requires_role</c> on every
    /// transport — one that fails late fails after the author has stopped looking (#1123).
    /// An empty list is refused rather than passed on: the compiled schema omits the key
    /// when empty, so an empty allow-list reads as a declared gate and compiles to none.
    /// </remarks>
    public static void Validate(string operationName, IReadOnlyList<string> actors)
    {
        if (actors.Count == 0)
        {
            throw new ArgumentException(
                $"{operationName}: RequiresActor was given an empty list. An empty allow-list "
                + "admits nobody and is dropped from the compiled schema, which admits "
                + $"everybody \u2014 name the actor types instead. Valid: {string.Join(", ", All)}");
        }

        var unknown = actors.Where(a => !All.Contains(a)).ToList();
        if (unknown.Count > 0)
        {
            throw new ArgumentException(
                $"{operationName}: RequiresActor names unknown actor type(s) "
                + $"{string.Join(", ", unknown)}. Valid: {string.Join(", ", All)}");
        }
    }
}

namespace FraiseQL.Builders;

/// <summary>
/// One action of an observer (#2147): a webhook, a Slack message or an email, with the
/// keys the server's observer runtime reads. <c>fraiseql compile</c> checks them against it.
/// </summary>
public sealed class ObserverAction
{
    /// <summary>The variable a Slack action reads its incoming-webhook URL from, unless told another.</summary>
    public const string DefaultSlackWebhookEnv = "SLACK_WEBHOOK_URL";

    private ObserverAction(IReadOnlyDictionary<string, object> config) => Config = config;

    /// <summary>The action's keys, as they are exported.</summary>
    public IReadOnlyDictionary<string, object> Config { get; }

    /// <summary>
    /// An HTTP request to <paramref name="url"/>, or to the URL the server reads from
    /// <paramref name="urlEnv"/> at delivery.
    /// </summary>
    /// <param name="url">The URL, written into the schema.</param>
    /// <param name="urlEnv">The variable holding the URL. Exactly one of the two is given.</param>
    /// <param name="method">The HTTP method; absent, the server's default.</param>
    /// <param name="bodyTemplate">The body, a template with <c>{{ field }}</c> placeholders.</param>
    /// <param name="signingSecretEnv">The variable holding the key the request is signed with.</param>
    /// <param name="headers">Headers sent as written.</param>
    /// <param name="headersEnv">Headers whose values the server reads from these variables.</param>
    /// <returns>The action.</returns>
    /// <exception cref="ArgumentException">Neither or both of <paramref name="url"/> and <paramref name="urlEnv"/>.</exception>
    public static ObserverAction Webhook(
        string? url = null,
        string? urlEnv = null,
        string? method = null,
        string? bodyTemplate = null,
        string? signingSecretEnv = null,
        IReadOnlyDictionary<string, string>? headers = null,
        IReadOnlyDictionary<string, string>? headersEnv = null)
    {
        if ((url is null) == (urlEnv is null))
            throw new ArgumentException(
                "ObserverAction.Webhook: give exactly one of url and urlEnv (the destination).");
        var config = new Dictionary<string, object> { ["type"] = "webhook" };
        AddIfSet(config, "url", url);
        AddIfSet(config, "url_env", urlEnv);
        AddIfSet(config, "method", method);
        AddIfSet(config, "body_template", bodyTemplate);
        AddIfSet(config, "signing_secret_env", signingSecretEnv);
        if (headers is { Count: > 0 })
            config["headers"] = headers;
        if (headersEnv is { Count: > 0 })
            config["headers_env"] = headersEnv;
        return new ObserverAction(config);
    }

    /// <summary>
    /// Posts <paramref name="message"/>, a <c>{{ field }}</c> template, to
    /// <paramref name="channel"/>. The incoming-webhook URL is read from
    /// <see cref="DefaultSlackWebhookEnv"/> unless <paramref name="webhookUrl"/> (written
    /// into the schema) or <paramref name="webhookUrlEnv"/> names another.
    /// </summary>
    /// <param name="channel">The channel.</param>
    /// <param name="message">The message template.</param>
    /// <param name="webhookUrl">The incoming-webhook URL, written into the schema.</param>
    /// <param name="webhookUrlEnv">The variable holding the incoming-webhook URL.</param>
    /// <returns>The action.</returns>
    /// <exception cref="ArgumentException">Both <paramref name="webhookUrl"/> and <paramref name="webhookUrlEnv"/>.</exception>
    public static ObserverAction Slack(
        string channel, string message, string? webhookUrl = null, string? webhookUrlEnv = null)
    {
        if (webhookUrl is not null && webhookUrlEnv is not null)
            throw new ArgumentException(
                "ObserverAction.Slack: give at most one of webhookUrl and webhookUrlEnv (the destination).");
        var config = new Dictionary<string, object>
        {
            ["type"] = "slack",
            ["channel"] = channel,
            ["message_template"] = message,
        };
        if (webhookUrl is not null)
            config["webhook_url"] = webhookUrl;
        else
            config["webhook_url_env"] = webhookUrlEnv ?? DefaultSlackWebhookEnv;
        return new ObserverAction(config);
    }

    /// <summary>
    /// An email to <paramref name="to"/>; <paramref name="body"/> is a <c>{{ field }}</c>
    /// template. The sender is the server's SMTP configuration: an action cannot set one.
    /// </summary>
    /// <param name="to">The recipient.</param>
    /// <param name="subject">The subject.</param>
    /// <param name="body">The body template.</param>
    /// <returns>The action.</returns>
    public static ObserverAction Email(string to, string subject, string body) =>
        new(new Dictionary<string, object>
        {
            ["type"] = "email",
            ["to"] = to,
            ["subject"] = subject,
            ["body_template"] = body,
        });

    private static void AddIfSet(Dictionary<string, object> config, string key, string? value)
    {
        if (value is not null)
            config[key] = value;
    }
}

/// <summary>How an observer retries an action that fails (#2147).</summary>
/// <param name="MaxAttempts">The attempts, the first included.</param>
/// <param name="BackoffStrategy"><c>fixed</c>, <c>linear</c> or <c>exponential</c>.</param>
/// <param name="InitialDelayMs">The delay before the first retry.</param>
/// <param name="MaxDelayMs">The longest delay between retries.</param>
public sealed record RetryConfig(int MaxAttempts, string BackoffStrategy, int InitialDelayMs, int MaxDelayMs);

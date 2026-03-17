namespace FraiseQL;

/// <summary>Configures automatic retry behaviour for <see cref="FraiseQLClient"/>.</summary>
public class RetryOptions
{
    /// <summary>Gets or sets the maximum number of attempts (including the first).</summary>
    public int MaxAttempts { get; set; } = 1;

    /// <summary>Gets or sets the initial delay between retries.</summary>
    public TimeSpan BaseDelay { get; set; } = TimeSpan.FromSeconds(1);

    /// <summary>Gets or sets the maximum delay between retries.</summary>
    public TimeSpan MaxDelay { get; set; } = TimeSpan.FromSeconds(30);

    /// <summary>Gets or sets whether to apply random jitter to the delay.</summary>
    public bool Jitter { get; set; } = true;
}

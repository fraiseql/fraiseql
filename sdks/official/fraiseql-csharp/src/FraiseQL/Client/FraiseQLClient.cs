using System.Net.Http.Json;
using System.Text.Json;

namespace FraiseQL;

/// <summary>Options for constructing a <see cref="FraiseQLClient"/>.</summary>
public class FraiseQLClientOptions
{
    /// <summary>Gets or sets the GraphQL endpoint URL.</summary>
    public string Url { get; set; } = "";

    /// <summary>
    /// Gets or sets a static <c>Authorization</c> header value (e.g. <c>"Bearer &lt;token&gt;"</c>).
    /// Ignored when <see cref="AuthorizationFactory"/> is set.
    /// </summary>
    public string? Authorization { get; set; }

    /// <summary>
    /// Gets or sets an async factory that returns the <c>Authorization</c> header value on each request.
    /// Takes precedence over <see cref="Authorization"/> when set.
    /// </summary>
    public Func<Task<string>>? AuthorizationFactory { get; set; }

    /// <summary>Gets or sets the per-request timeout. Defaults to 30 seconds.</summary>
    public TimeSpan Timeout { get; set; } = TimeSpan.FromSeconds(30);

    /// <summary>Gets or sets optional retry configuration.</summary>
    public RetryOptions? Retry { get; set; }

    /// <summary>
    /// Gets or sets an externally managed <see cref="HttpClient"/> to use.
    /// When provided, the <see cref="FraiseQLClient"/> will not dispose it.
    /// </summary>
    public HttpClient? HttpClient { get; set; }
}

/// <summary>
/// HTTP client for executing GraphQL queries and mutations against a FraiseQL server.
/// </summary>
public sealed class FraiseQLClient : IDisposable
{
    private readonly HttpClient _http;
    private readonly FraiseQLClientOptions _options;
    private readonly bool _ownsHttpClient;

    /// <summary>Constructs a client pointing at <paramref name="url"/> with default options.</summary>
    public FraiseQLClient(string url) : this(new FraiseQLClientOptions { Url = url }) { }

    /// <summary>Constructs a client from the provided <paramref name="options"/>.</summary>
    public FraiseQLClient(FraiseQLClientOptions options)
    {
        _options = options;
        if (options.HttpClient is not null)
        {
            _http = options.HttpClient;
            _ownsHttpClient = false;
        }
        else
        {
            _http = new HttpClient { Timeout = options.Timeout };
            _ownsHttpClient = true;
        }
    }

    /// <summary>Executes a GraphQL query synchronously and returns the deserialized <c>data</c> field.</summary>
    public T Query<T>(string query, object? variables = null, string? operationName = null)
        => QueryAsync<T>(query, variables, operationName).GetAwaiter().GetResult();

    /// <summary>Executes a GraphQL mutation synchronously and returns the deserialized <c>data</c> field.</summary>
    public T Mutate<T>(string mutation, object? variables = null, string? operationName = null)
        => MutateAsync<T>(mutation, variables, operationName).GetAwaiter().GetResult();

    /// <summary>Executes a GraphQL query asynchronously and returns the deserialized <c>data</c> field.</summary>
    /// <exception cref="GraphQLException">The response contained one or more GraphQL errors.</exception>
    /// <exception cref="AuthenticationException">The server returned HTTP 401 or 403.</exception>
    /// <exception cref="RateLimitException">The server returned HTTP 429.</exception>
    /// <exception cref="FraiseQLTimeoutException">The request exceeded its timeout.</exception>
    /// <exception cref="NetworkException">A network-level error occurred.</exception>
    public async Task<T> QueryAsync<T>(string query, object? variables = null, string? operationName = null, CancellationToken ct = default)
        => await ExecuteAsync<T>(query, variables, operationName, ct);

    /// <summary>Executes a GraphQL mutation asynchronously and returns the deserialized <c>data</c> field.</summary>
    /// <exception cref="GraphQLException">The response contained one or more GraphQL errors.</exception>
    /// <exception cref="AuthenticationException">The server returned HTTP 401 or 403.</exception>
    /// <exception cref="RateLimitException">The server returned HTTP 429.</exception>
    /// <exception cref="FraiseQLTimeoutException">The request exceeded its timeout.</exception>
    /// <exception cref="NetworkException">A network-level error occurred.</exception>
    public async Task<T> MutateAsync<T>(string mutation, object? variables = null, string? operationName = null, CancellationToken ct = default)
        => await ExecuteAsync<T>(mutation, variables, operationName, ct);

    private async Task<T> ExecuteAsync<T>(string gqlQuery, object? variables, string? operationName, CancellationToken ct)
    {
        var body = operationName is not null
            ? (object)new { query = gqlQuery, variables, operationName }
            : new { query = gqlQuery, variables };
        using var request = new HttpRequestMessage(HttpMethod.Post, _options.Url)
        {
            Content = JsonContent.Create(body)
        };

        if (_options.AuthorizationFactory is not null)
            request.Headers.TryAddWithoutValidation("Authorization", await _options.AuthorizationFactory());
        else if (_options.Authorization is not null)
            request.Headers.TryAddWithoutValidation("Authorization", _options.Authorization);

        HttpResponseMessage response;
        try
        {
            response = await _http.SendAsync(request, ct);
        }
        catch (TaskCanceledException ex) when (!ct.IsCancellationRequested)
        {
            throw new FraiseQLTimeoutException("Request timed out", ex);
        }
        catch (HttpRequestException ex)
        {
            throw new NetworkException(ex.Message, ex);
        }

        using (response)
        {
            var statusCode = (int)response.StatusCode;
            if (statusCode is 401 or 403)
                throw new AuthenticationException(statusCode);

            if (statusCode == 429)
            {
                TimeSpan? retryAfter = null;
                if (response.Headers.RetryAfter?.Delta is { } delta)
                    retryAfter = delta;
                throw new RateLimitException(retryAfter);
            }

            var json = await response.Content.ReadAsStringAsync(ct);
            var doc = JsonDocument.Parse(json);

            if (doc.RootElement.TryGetProperty("errors", out var errorsEl)
                && errorsEl.ValueKind == JsonValueKind.Array
                && errorsEl.GetArrayLength() > 0)
            {
                var errors = errorsEl.EnumerateArray()
                    .Select(e => new GraphQLError(
                        e.TryGetProperty("message", out var m) ? m.GetString() ?? "" : ""))
                    .ToList();
                throw new GraphQLException(errors);
            }

            if (!doc.RootElement.TryGetProperty("data", out var dataEl)
                || dataEl.ValueKind == JsonValueKind.Null)
                return default!;

            return JsonSerializer.Deserialize<T>(
                dataEl.GetRawText(),
                new JsonSerializerOptions { PropertyNameCaseInsensitive = true }) ?? default!;
        }
    }

    /// <inheritdoc />
    public void Dispose()
    {
        if (_ownsHttpClient)
            _http.Dispose();
    }
}

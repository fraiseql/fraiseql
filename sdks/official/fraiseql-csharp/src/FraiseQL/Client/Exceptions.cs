namespace FraiseQL;

/// <summary>Base class for all FraiseQL SDK exceptions.</summary>
public class FraiseQLException : Exception
{
    /// <inheritdoc />
    public FraiseQLException(string message) : base(message) { }

    /// <inheritdoc />
    public FraiseQLException(string message, Exception inner) : base(message, inner) { }
}

/// <summary>One or more GraphQL protocol errors were returned in the response.</summary>
public class GraphQLException : FraiseQLException
{
    /// <summary>Gets the list of GraphQL errors from the response.</summary>
    public IReadOnlyList<GraphQLError> Errors { get; }

    /// <summary>Initializes a new instance of <see cref="GraphQLException"/> from a list of errors.</summary>
    public GraphQLException(IReadOnlyList<GraphQLError> errors)
        : base(errors.Count > 0 ? errors[0].Message : "GraphQL error")
        => Errors = errors;
}

/// <summary>Represents a single error entry in a GraphQL response.</summary>
/// <param name="Message">The human-readable error message.</param>
/// <param name="Locations">Optional source locations within the query document.</param>
public record GraphQLError(string Message, IReadOnlyList<GraphQLErrorLocation>? Locations = null);

/// <summary>Source location of a GraphQL error.</summary>
/// <param name="Line">One-based line number.</param>
/// <param name="Column">One-based column number.</param>
public record GraphQLErrorLocation(int Line, int Column);

/// <summary>A network-level error occurred while sending the request.</summary>
public class NetworkException : FraiseQLException
{
    /// <inheritdoc />
    public NetworkException(string message, Exception? inner = null) : base(message, inner!) { }
}

/// <summary>The request exceeded its configured timeout.</summary>
public class FraiseQLTimeoutException : FraiseQLException
{
    /// <inheritdoc />
    public FraiseQLTimeoutException(string message = "Request timed out") : base(message) { }

    /// <inheritdoc />
    public FraiseQLTimeoutException(string message, Exception inner) : base(message, inner) { }
}

/// <summary>The server rejected the request with an authentication or authorization error.</summary>
public class AuthenticationException : FraiseQLException
{
    /// <summary>Gets the HTTP status code returned by the server.</summary>
    public int StatusCode { get; }

    /// <summary>Initializes a new instance from an HTTP status code.</summary>
    public AuthenticationException(int statusCode)
        : base($"Authentication failed (HTTP {statusCode})")
        => StatusCode = statusCode;
}

/// <summary>The server returned a 429 Too Many Requests response.</summary>
public class RateLimitException : FraiseQLException
{
    /// <summary>Gets the suggested retry delay, if the server provided one.</summary>
    public TimeSpan? RetryAfter { get; }

    /// <summary>Initializes a new instance of <see cref="RateLimitException"/>.</summary>
    public RateLimitException(TimeSpan? retryAfter = null)
        : base("Rate limit exceeded")
        => RetryAfter = retryAfter;
}

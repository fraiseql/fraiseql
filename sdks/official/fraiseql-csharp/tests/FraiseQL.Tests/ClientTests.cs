using System.Net;
using System.Text;
using Xunit;

namespace FraiseQL.Tests;

/// <summary>
/// Tests for <see cref="FraiseQLClient"/> using a stub <see cref="HttpMessageHandler"/>
/// so no real network connection is required.
/// </summary>
public sealed class ClientTests
{
    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    private static FraiseQLClient BuildClient(HttpStatusCode status, string body)
    {
        var handler = new StubHandler(status, body);
        var http = new HttpClient(handler) { BaseAddress = null };
        var options = new FraiseQLClientOptions
        {
            Url = "http://localhost/graphql",
            HttpClient = http,
        };
        return new FraiseQLClient(options);
    }

    private static string DataJson(string payload)
        => $"{{\"data\":{payload}}}";

    private static string ErrorJson(string message)
        => $"{{\"errors\":[{{\"message\":\"{message}\"}}]}}";

    private static string NullErrorsJson(string payload)
        => $"{{\"data\":{payload},\"errors\":null}}";

    // ------------------------------------------------------------------
    // QueryAsync — success
    // ------------------------------------------------------------------

    [Fact]
    public async Task QueryAsync_ReturnsData_OnSuccess()
    {
        using var client = BuildClient(HttpStatusCode.OK, DataJson("{\"id\":42}"));

        var result = await client.QueryAsync<UserResult>("{ user { id } }");

        Assert.NotNull(result);
        Assert.Equal(42, result.Id);
    }

    [Fact]
    public async Task QueryAsync_ReturnsDefaultT_WhenDataIsNull()
    {
        using var client = BuildClient(HttpStatusCode.OK, "{\"data\":null}");

        var result = await client.QueryAsync<UserResult>("{ user { id } }");

        Assert.Null(result);
    }

    // ------------------------------------------------------------------
    // QueryAsync — null errors field treated as success
    // ------------------------------------------------------------------

    [Fact]
    public async Task QueryAsync_DoesNotThrow_WhenErrorsFieldIsNull()
    {
        using var client = BuildClient(HttpStatusCode.OK, NullErrorsJson("{\"id\":1}"));

        // Must NOT throw — null errors field is not an error
        var result = await client.QueryAsync<UserResult>("{ user { id } }");

        Assert.NotNull(result);
        Assert.Equal(1, result.Id);
    }

    [Fact]
    public async Task QueryAsync_DoesNotThrow_WhenErrorsArrayIsEmpty()
    {
        using var client = BuildClient(HttpStatusCode.OK, "{\"data\":{\"id\":7},\"errors\":[]}");

        var result = await client.QueryAsync<UserResult>("{ user { id } }");

        Assert.Equal(7, result.Id);
    }

    // ------------------------------------------------------------------
    // QueryAsync — GraphQL errors
    // ------------------------------------------------------------------

    [Fact]
    public async Task QueryAsync_ThrowsGraphQLException_WhenErrorsPresent()
    {
        using var client = BuildClient(HttpStatusCode.OK, ErrorJson("field not found"));

        var ex = await Assert.ThrowsAsync<GraphQLException>(
            () => client.QueryAsync<object>("{ bad }"));

        Assert.Single(ex.Errors);
        Assert.Equal("field not found", ex.Errors[0].Message);
    }

    [Fact]
    public async Task QueryAsync_ExceptionMessage_EqualsFirstErrorMessage()
    {
        using var client = BuildClient(HttpStatusCode.OK,
            "{\"errors\":[{\"message\":\"first\"},{\"message\":\"second\"}]}");

        var ex = await Assert.ThrowsAsync<GraphQLException>(
            () => client.QueryAsync<object>("{ bad }"));

        Assert.Equal("first", ex.Message);
        Assert.Equal(2, ex.Errors.Count);
    }

    // ------------------------------------------------------------------
    // QueryAsync — HTTP error status codes
    // ------------------------------------------------------------------

    [Fact]
    public async Task QueryAsync_ThrowsAuthenticationException_On401()
    {
        using var client = BuildClient(HttpStatusCode.Unauthorized, "Unauthorized");

        var ex = await Assert.ThrowsAsync<AuthenticationException>(
            () => client.QueryAsync<object>("{ x }"));

        Assert.Equal(401, ex.StatusCode);
    }

    [Fact]
    public async Task QueryAsync_ThrowsAuthenticationException_On403()
    {
        using var client = BuildClient(HttpStatusCode.Forbidden, "Forbidden");

        var ex = await Assert.ThrowsAsync<AuthenticationException>(
            () => client.QueryAsync<object>("{ x }"));

        Assert.Equal(403, ex.StatusCode);
    }

    [Fact]
    public async Task QueryAsync_ThrowsRateLimitException_On429()
    {
        using var client = BuildClient((HttpStatusCode)429, "Too Many Requests");

        await Assert.ThrowsAsync<RateLimitException>(
            () => client.QueryAsync<object>("{ x }"));
    }

    // ------------------------------------------------------------------
    // Synchronous wrappers
    // ------------------------------------------------------------------

    [Fact]
    public void Query_ReturnsData_Synchronously()
    {
        using var client = BuildClient(HttpStatusCode.OK, DataJson("{\"id\":99}"));

        var result = client.Query<UserResult>("{ user { id } }");

        Assert.Equal(99, result.Id);
    }

    [Fact]
    public void Mutate_ReturnsData_Synchronously()
    {
        using var client = BuildClient(HttpStatusCode.OK, DataJson("{\"id\":5}"));

        var result = client.Mutate<UserResult>("mutation { createUser { id } }");

        Assert.Equal(5, result.Id);
    }

    // ------------------------------------------------------------------
    // Authorization header
    // ------------------------------------------------------------------

    [Fact]
    public async Task QueryAsync_SendsStaticAuthorizationHeader()
    {
        var handler = new CapturingHandler(HttpStatusCode.OK, DataJson("null"));
        var http = new HttpClient(handler);
        using var client = new FraiseQLClient(new FraiseQLClientOptions
        {
            Url = "http://localhost/graphql",
            Authorization = "Bearer my-token",
            HttpClient = http,
        });

        await client.QueryAsync<object>("{ x }");

        Assert.True(handler.LastRequest?.Headers.Contains("Authorization"));
        Assert.Equal("Bearer my-token",
            handler.LastRequest?.Headers.GetValues("Authorization").First());
    }

    [Fact]
    public async Task QueryAsync_SendsDynamicAuthorizationHeader_WhenFactorySet()
    {
        var handler = new CapturingHandler(HttpStatusCode.OK, DataJson("null"));
        var http = new HttpClient(handler);
        using var client = new FraiseQLClient(new FraiseQLClientOptions
        {
            Url = "http://localhost/graphql",
            AuthorizationFactory = () => Task.FromResult("Bearer dynamic"),
            HttpClient = http,
        });

        await client.QueryAsync<object>("{ x }");

        Assert.Equal("Bearer dynamic",
            handler.LastRequest?.Headers.GetValues("Authorization").First());
    }

    // ------------------------------------------------------------------
    // Helpers — stub types
    // ------------------------------------------------------------------

    private sealed record UserResult(int Id);

    private sealed class StubHandler : HttpMessageHandler
    {
        private readonly HttpStatusCode _status;
        private readonly string _body;

        public StubHandler(HttpStatusCode status, string body)
        {
            _status = status;
            _body = body;
        }

        protected override Task<HttpResponseMessage> SendAsync(
            HttpRequestMessage request, CancellationToken cancellationToken)
        {
            var response = new HttpResponseMessage(_status)
            {
                Content = new StringContent(_body, Encoding.UTF8, "application/json")
            };
            return Task.FromResult(response);
        }
    }

    private sealed class CapturingHandler : HttpMessageHandler
    {
        private readonly HttpStatusCode _status;
        private readonly string _body;

        public HttpRequestMessage? LastRequest { get; private set; }

        public CapturingHandler(HttpStatusCode status, string body)
        {
            _status = status;
            _body = body;
        }

        protected override Task<HttpResponseMessage> SendAsync(
            HttpRequestMessage request, CancellationToken cancellationToken)
        {
            LastRequest = request;
            var response = new HttpResponseMessage(_status)
            {
                Content = new StringContent(_body, Encoding.UTF8, "application/json")
            };
            return Task.FromResult(response);
        }
    }
}

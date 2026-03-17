module FraiseQL.Tests.ClientTests

open System
open System.Net
open System.Net.Http
open System.Text
open System.Threading
open System.Threading.Tasks
open Xunit
open FraiseQL

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

type UserResult() =
    member val id: int = 0 with get, set

/// Stub HttpMessageHandler that always returns a fixed response.
type StubHandler(status: HttpStatusCode, body: string) =
    inherit HttpMessageHandler()
    override _.SendAsync(_, _) =
        let response = new HttpResponseMessage(status,
                           Content = new StringContent(body, Encoding.UTF8, "application/json"))
        Task.FromResult response

/// HttpMessageHandler that captures the last request sent.
type CapturingHandler(status: HttpStatusCode, body: string) =
    inherit HttpMessageHandler()
    let mutable lastRequest: HttpRequestMessage option = None
    member _.LastRequest = lastRequest
    override _.SendAsync(request, _) =
        lastRequest <- Some request
        let response = new HttpResponseMessage(status,
                           Content = new StringContent(body, Encoding.UTF8, "application/json"))
        Task.FromResult response

let private dataJson payload = $"""{"{"}"data":{payload}{"}"}"""
let private errorJson msg   = $"""{"{"}"errors":[{"{"}"message":"{msg}"{"}"}]{"}"}"""

let private buildClient status body =
    let handler = new StubHandler(status, body)
    let http    = new HttpClient(handler)
    let options = { FraiseQLClientOptions.defaultFor "http://localhost/graphql"
                    with HttpClient = Some http }
    new FraiseQLHttpClient(options)

// ---------------------------------------------------------------------------
// QueryAsync — success
// ---------------------------------------------------------------------------

[<Fact>]
let ``ExecuteAsync returns data on success`` () =
    let json = dataJson """{"id":42}"""
    use client = buildClient HttpStatusCode.OK json
    let result = client.ExecuteAsync<UserResult>("{ user { id } }").GetAwaiter().GetResult()
    Assert.Equal(42, result.id)


[<Fact>]
let ``AsyncQuery returns data on success`` () =
    async {
        use client = buildClient HttpStatusCode.OK (dataJson """{"id":42}""")
        let! result = client.AsyncQuery<UserResult>("{ user { id } }")
        Assert.Equal(42, result.id)
    } |> Async.RunSynchronously

[<Fact>]
let ``AsyncQuery returns default when data is null`` () =
    async {
        use client = buildClient HttpStatusCode.OK """{"data":null}"""
        let! result = client.AsyncQuery<UserResult option>("{ user { id } }")
        Assert.Equal(None, result)
    } |> Async.RunSynchronously

// ---------------------------------------------------------------------------
// QueryAsync — null errors field treated as success (CRITICAL)
// ---------------------------------------------------------------------------

[<Fact>]
let ``AsyncQuery does not throw when errors field is null`` () =
    async {
        use client = buildClient HttpStatusCode.OK """{"data":{"id":1},"errors":null}"""
        // Must NOT throw — null errors is not an error
        let! result = client.AsyncQuery<UserResult>("{ user { id } }")
        Assert.Equal(1, result.id)
    } |> Async.RunSynchronously

[<Fact>]
let ``AsyncQuery does not throw when errors array is empty`` () =
    async {
        use client = buildClient HttpStatusCode.OK """{"data":{"id":7},"errors":[]}"""
        let! result = client.AsyncQuery<UserResult>("{ user { id } }")
        Assert.Equal(7, result.id)
    } |> Async.RunSynchronously

// ---------------------------------------------------------------------------
// QueryAsync — GraphQL errors
// ---------------------------------------------------------------------------

[<Fact>]
let ``AsyncQuery throws GraphQLException when errors present`` () =
    async {
        use client = buildClient HttpStatusCode.OK (errorJson "field not found")
        let! ex = Assert.ThrowsAsync<GraphQLException>(
                      fun () -> client.ExecuteAsync<obj>("{ bad }"))
                  |> Async.AwaitTask
        Assert.Single(ex.Errors) |> ignore
        Assert.Equal("field not found", ex.Errors.[0].Message)
    } |> Async.RunSynchronously

// ---------------------------------------------------------------------------
// QueryAsync — HTTP errors
// ---------------------------------------------------------------------------

[<Fact>]
let ``AsyncQuery throws AuthenticationException on 401`` () =
    async {
        use client = buildClient HttpStatusCode.Unauthorized "Unauthorized"
        let! ex = Assert.ThrowsAsync<AuthenticationException>(
                      fun () -> client.ExecuteAsync<obj>("{ x }"))
                  |> Async.AwaitTask
        Assert.Equal(401, ex.StatusCode)
    } |> Async.RunSynchronously

[<Fact>]
let ``AsyncQuery throws AuthenticationException on 403`` () =
    async {
        use client = buildClient HttpStatusCode.Forbidden "Forbidden"
        let! ex = Assert.ThrowsAsync<AuthenticationException>(
                      fun () -> client.ExecuteAsync<obj>("{ x }"))
                  |> Async.AwaitTask
        Assert.Equal(403, ex.StatusCode)
    } |> Async.RunSynchronously

[<Fact>]
let ``AsyncQuery throws RateLimitException on 429`` () =
    async {
        use client = buildClient (enum<HttpStatusCode> 429) "Too Many Requests"
        do! Assert.ThrowsAsync<RateLimitException>(
                fun () -> client.ExecuteAsync<obj>("{ x }"))
            |> Async.AwaitTask
            |> Async.Ignore
    } |> Async.RunSynchronously

// ---------------------------------------------------------------------------
// Authorization headers
// ---------------------------------------------------------------------------

[<Fact>]
let ``ExecuteAsync sends static Authorization header`` () =
    async {
        let handler = new CapturingHandler(HttpStatusCode.OK, dataJson "null")
        let http    = new HttpClient(handler)
        use client  = new FraiseQLHttpClient(
                          { FraiseQLClientOptions.defaultFor "http://localhost/graphql"
                            with Authorization = Some "Bearer my-token"
                                 HttpClient    = Some http })
        let! _ = client.ExecuteAsync<obj>("{ x }") |> Async.AwaitTask
        let req = handler.LastRequest.Value
        Assert.True(req.Headers.Contains "Authorization")
        Assert.Equal("Bearer my-token",
            req.Headers.GetValues("Authorization") |> Seq.head)
    } |> Async.RunSynchronously

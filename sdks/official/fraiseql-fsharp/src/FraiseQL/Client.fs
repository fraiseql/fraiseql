namespace FraiseQL

open System
open System.Net.Http
open System.Net.Http.Json
open System.Text.Json
open System.Threading
open System.Threading.Tasks

// ---------------------------------------------------------------------------
// Exceptions
// ---------------------------------------------------------------------------

/// Base class for all FraiseQL F# SDK exceptions.
type FraiseQLException(message: string, ?inner: exn) =
    inherit Exception(message, defaultArg inner null)

/// One or more GraphQL protocol errors were returned in the response.
type GraphQLException(errors: {| Message: string |} list) =
    inherit FraiseQLException(
        if errors.IsEmpty then "GraphQL error" else errors.[0].Message)
    member _.Errors = errors

/// The server rejected the request with an authentication or authorization error.
type AuthenticationException(statusCode: int) =
    inherit FraiseQLException($"Authentication failed (HTTP {statusCode})")
    member _.StatusCode = statusCode

/// The server returned a 429 Too Many Requests response.
type RateLimitException(?retryAfter: TimeSpan) =
    inherit FraiseQLException("Rate limit exceeded")
    member _.RetryAfter: TimeSpan option = retryAfter

/// The request exceeded its configured timeout.
type FraiseQLTimeoutException(message: string, ?inner: exn) =
    inherit FraiseQLException(message, ?inner = inner)

/// A network-level error occurred while sending the request.
type FraiseQLNetworkException(message: string, ?inner: exn) =
    inherit FraiseQLException(message, ?inner = inner)

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

/// Configuration for <see cref="FraiseQLHttpClient"/>.
type FraiseQLClientOptions =
    {
        /// The GraphQL endpoint URL.
        Url: string
        /// Optional static <c>Authorization</c> header value.
        Authorization: string option
        /// Optional async factory for the <c>Authorization</c> header value.
        AuthorizationFactory: (unit -> Task<string>) option
        /// Per-request timeout. Defaults to 30 seconds.
        Timeout: TimeSpan
        /// An optional externally managed <see cref="HttpClient"/>.
        HttpClient: HttpClient option
    }

/// Default options pointing at the given URL.
module FraiseQLClientOptions =
    /// Creates a default options record for the given endpoint URL.
    let defaultFor url =
        {
            Url = url
            Authorization = None
            AuthorizationFactory = None
            Timeout = TimeSpan.FromSeconds 30.0
            HttpClient = None
        }

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

/// HTTP client for executing GraphQL queries and mutations against a FraiseQL server.
type FraiseQLHttpClient(options: FraiseQLClientOptions) =

    let http, ownsHttp =
        match options.HttpClient with
        | Some h -> h, false
        | None   -> new HttpClient(Timeout = options.Timeout), true

    /// Constructs a client pointing at <paramref name="url"/> with default options.
    new(url: string) = new FraiseQLHttpClient(FraiseQLClientOptions.defaultFor url)

    /// Executes a GraphQL query asynchronously and returns the deserialized <c>data</c> field.
    member _.ExecuteAsync<'T>(gqlQuery: string, ?variables: obj, ?ct: CancellationToken) : Task<'T> =
        let ct = defaultArg ct CancellationToken.None
        task {
            let body = {| query = gqlQuery; variables = variables |}
            use request = new HttpRequestMessage(HttpMethod.Post, options.Url,
                                                 Content = JsonContent.Create(body))

            match options.AuthorizationFactory with
            | Some factory ->
                let! token = factory ()
                request.Headers.TryAddWithoutValidation("Authorization", token) |> ignore
            | None ->
                match options.Authorization with
                | Some auth ->
                    request.Headers.TryAddWithoutValidation("Authorization", auth) |> ignore
                | None -> ()

            let! response =
                task {
                    try
                        return! http.SendAsync(request, ct)
                    with
                    | :? TaskCanceledException as ex when not ct.IsCancellationRequested ->
                        return raise (FraiseQLTimeoutException("Request timed out", ex))
                    | :? HttpRequestException as ex ->
                        return raise (FraiseQLNetworkException(ex.Message, ex))
                }

            use response = response
            let statusCode = int response.StatusCode

            if statusCode = 401 || statusCode = 403 then
                raise (AuthenticationException statusCode)

            if statusCode = 429 then
                raise (RateLimitException())

            let! json = response.Content.ReadAsStringAsync(ct)

            // Parse and extract what we need, disposing the document cleanly.
            let parseResult =
                use doc = JsonDocument.Parse(json)
                let errorsOpt =
                    match doc.RootElement.TryGetProperty("errors") with
                    | true, el when el.ValueKind = JsonValueKind.Array && el.GetArrayLength() > 0 ->
                        el.EnumerateArray()
                        |> Seq.map (fun e ->
                            let msg =
                                match e.TryGetProperty("message") with
                                | true, m -> m.GetString() |> Option.ofObj |> Option.defaultValue ""
                                | _ -> ""
                            {| Message = msg |})
                        |> Seq.toList
                        |> Some
                    | _ -> None
                let dataRaw =
                    match doc.RootElement.TryGetProperty("data") with
                    | true, dataEl when dataEl.ValueKind <> JsonValueKind.Null ->
                        Some (dataEl.GetRawText())
                    | _ -> None
                errorsOpt, dataRaw

            let errorsOpt, dataRaw = parseResult

            if errorsOpt.IsSome then
                raise (GraphQLException errorsOpt.Value)

            let opts = JsonSerializerOptions(PropertyNameCaseInsensitive = true)
            return
                match dataRaw with
                | Some raw -> JsonSerializer.Deserialize<'T>(raw, opts)
                | None     -> Unchecked.defaultof<'T>
        }

    /// Executes a GraphQL query and returns the deserialized <c>data</c> field as an F# Async.
    member this.AsyncQuery<'T>(query: string, ?variables: obj) : Async<'T> =
        this.ExecuteAsync<'T>(query, ?variables = variables)
        |> Async.AwaitTask

    /// Executes a GraphQL mutation and returns the deserialized <c>data</c> field as an F# Async.
    member this.AsyncMutate<'T>(mutation: string, ?variables: obj) : Async<'T> =
        this.ExecuteAsync<'T>(mutation, ?variables = variables)
        |> Async.AwaitTask

    interface IDisposable with
        member _.Dispose() =
            if ownsHttp then http.Dispose()

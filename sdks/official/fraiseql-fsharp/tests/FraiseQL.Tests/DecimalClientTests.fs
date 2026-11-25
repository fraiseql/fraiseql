/// A `Decimal` travels as the string of its exact digits. The server takes a `Decimal`
/// argument as a string such as "1.50", or an exact integer, and refuses a number with a
/// fraction; a response holds each `Decimal` as that string.
module FraiseQL.Tests.DecimalClientTests

open System
open System.Collections.Generic
open System.Globalization
open System.Net
open System.Net.Http
open System.Text
open System.Text.Json
open System.Threading.Tasks
open Xunit
open FraiseQL

[<CLIMutable>]
type Quote = { Amount: decimal; FxRate: decimal option }

/// Records the request body and answers with a fixed response.
type EchoHandler(response: string) =
    inherit HttpMessageHandler()
    let mutable sent: string = null
    member _.Sent = sent
    override _.SendAsync(request, ct) =
        task {
            let! body = request.Content.ReadAsStringAsync(ct)
            sent <- body
            return new HttpResponseMessage(HttpStatusCode.OK,
                       Content = new StringContent(response, Encoding.UTF8, "application/json"))
        }

let private build (response: string) =
    let handler = new EchoHandler(response)
    let options = { FraiseQLClientOptions.defaultFor "http://localhost/graphql"
                    with HttpClient = Some(new HttpClient(handler)) }
    new FraiseQLHttpClient(options), handler

let private sentVariables (handler: EchoHandler) =
    JsonDocument.Parse(handler.Sent).RootElement.GetProperty("variables")

let private quoteResponse (amount: string) =
    sprintf """{"data":{"amount":"%s","fxRate":null}}""" amount

[<Theory>]
[<InlineData("1.50")>]
[<InlineData("100.00")>]
[<InlineData("12345678901234567890.12")>]
[<InlineData("-0.0015")>]
[<InlineData("79228162514264337593543950335")>]
let ``a decimal variable is sent as a string of its digits`` (digits: string) : Task =
    task {
        let client, handler = build """{"data":{}}"""
        use _ = client
        let amount = Decimal.Parse(digits, CultureInfo.InvariantCulture)
        let! _ = client.ExecuteAsync<JsonElement>("query($a: Decimal) { echo(a: $a) }", {| a = amount |})
        let sent = (sentVariables handler).GetProperty("a")
        Assert.Equal(JsonValueKind.String, sent.ValueKind)
        Assert.Equal(digits, sent.GetString())
    }

[<Fact>]
let ``a decimal in a dictionary or a list is sent as a string`` () : Task =
    task {
        let client, handler = build """{"data":{}}"""
        use _ = client
        let variables = Dictionary<string, obj>()
        variables["a"] <- box (dict [ "amounts", box [| 1.50m; 2.0m |] ])
        let! _ = client.ExecuteAsync<JsonElement>("query($a: In) { echo(a: $a) }", variables)
        Assert.Equal("""["1.50","2.0"]""", (sentVariables handler).GetProperty("a").GetProperty("amounts").GetRawText())
    }

[<Fact>]
let ``other numbers and the body shape are unchanged`` () : Task =
    task {
        let client, handler = build """{"data":{}}"""
        use _ = client
        let! _ = client.ExecuteAsync<JsonElement>("{ x }", {| count = 3; ratio = 0.5 |}, "Op")
        let root = JsonDocument.Parse(handler.Sent).RootElement
        Assert.Equal("Op", root.GetProperty("operationName").GetString())
        Assert.Equal(JsonValueKind.Number, root.GetProperty("variables").GetProperty("count").ValueKind)
        Assert.Equal(JsonValueKind.Number, root.GetProperty("variables").GetProperty("ratio").ValueKind)
    }

[<Theory>]
[<InlineData("1.50")>]
[<InlineData("100.00")>]
[<InlineData("12345678901234567890.12")>]
[<InlineData("0.0015")>]
let ``a decimal response is read with its digits and scale`` (digits: string) : Task =
    task {
        let client, _ = build (quoteResponse digits)
        use _ = client
        let! quote = client.ExecuteAsync<Quote>("{ quote { amount fxRate } }")
        Assert.Equal(digits, quote.Amount.ToString(CultureInfo.InvariantCulture))
        Assert.Equal(None, quote.FxRate)
    }

[<Fact>]
let ``an optional decimal response is read`` () : Task =
    task {
        let client, _ = build """{"data":{"amount":"1","fxRate":"1.0825"}}"""
        use _ = client
        let! quote = client.ExecuteAsync<Quote>("{ quote { amount fxRate } }")
        Assert.Equal(Some 1.0825m, quote.FxRate)
    }

// More significant digits than a .NET decimal holds (28-29): Decimal.Parse would round
// the first and overflow on the second.
[<Theory>]
[<InlineData("1.234567890123456789012345678901")>]
[<InlineData("123456789012345678901234567890")>]
let ``a decimal response wider than a dotnet decimal fails and is not rounded`` (digits: string) : Task =
    task {
        let client, _ = build (quoteResponse digits)
        use _ = client
        let! error = Assert.ThrowsAsync<JsonException>(fun () ->
            client.ExecuteAsync<Quote>("{ quote { amount fxRate } }") :> Task)
        Assert.Contains(digits, error.Message)
        Assert.Contains("28", error.Message)
    }

[<Theory>]
[<InlineData("1e3")>]
[<InlineData("abc")>]
[<InlineData("")>]
let ``a string that is not decimal digits fails`` (text: string) : Task =
    task {
        let client, _ = build (quoteResponse text)
        use _ = client
        let! _ = Assert.ThrowsAsync<JsonException>(fun () ->
            client.ExecuteAsync<Quote>("{ quote { amount fxRate } }") :> Task)
        ()
    }

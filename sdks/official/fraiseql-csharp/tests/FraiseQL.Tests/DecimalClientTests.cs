using System.Net;
using System.Text;
using System.Text.Json;
using Xunit;

namespace FraiseQL.Tests;

/// <summary>
/// A <c>Decimal</c> travels as the string of its exact digits. The server takes a
/// <c>Decimal</c> argument as a string such as <c>"1.50"</c>, or an exact integer, and
/// refuses a number with a fraction; a response holds each <c>Decimal</c> as that string.
/// </summary>
public sealed class DecimalClientTests
{
    public sealed record Quote(decimal Amount, decimal? FxRate);

    private sealed class EchoHandler : HttpMessageHandler
    {
        private readonly string _response;
        public string? SentBody { get; private set; }

        public EchoHandler(string response) => _response = response;

        protected override async Task<HttpResponseMessage> SendAsync(
            HttpRequestMessage request, CancellationToken cancellationToken)
        {
            SentBody = await request.Content!.ReadAsStringAsync(cancellationToken);
            return new HttpResponseMessage(HttpStatusCode.OK)
            {
                Content = new StringContent(_response, Encoding.UTF8, "application/json"),
            };
        }
    }

    private static (FraiseQLClient Client, EchoHandler Handler) Build(string response = "{\"data\":{}}")
    {
        var handler = new EchoHandler(response);
        var client = new FraiseQLClient(new FraiseQLClientOptions
        {
            Url = "http://localhost/graphql",
            HttpClient = new HttpClient(handler),
        });
        return (client, handler);
    }

    private static JsonElement SentVariables(EchoHandler handler)
        => JsonDocument.Parse(handler.SentBody!).RootElement.GetProperty("variables");

    [Theory]
    [InlineData("1.50")]
    [InlineData("100.00")]
    [InlineData("12345678901234567890.12")]
    [InlineData("-0.0015")]
    [InlineData("79228162514264337593543950335")]
    public async Task ADecimalVariableIsSentAsAStringOfItsDigits(string digits)
    {
        var (client, handler) = Build();
        using (client)
        {
            var amount = decimal.Parse(digits, System.Globalization.CultureInfo.InvariantCulture);
            await client.QueryAsync<JsonElement>("query($a: Decimal) { echo(a: $a) }", new { a = amount });
        }

        var sent = SentVariables(handler).GetProperty("a");
        Assert.Equal(JsonValueKind.String, sent.ValueKind);
        Assert.Equal(digits, sent.GetString());
    }

    [Fact]
    public async Task ADecimalInADictionaryOrAListIsSentAsAString()
    {
        var (client, handler) = Build();
        using (client)
        {
            var variables = new Dictionary<string, object>
            {
                ["a"] = new Dictionary<string, object> { ["amounts"] = new[] { 1.50m, 2.0m } },
            };
            await client.QueryAsync<JsonElement>("query($a: In) { echo(a: $a) }", variables);
        }

        Assert.Equal("[\"1.50\",\"2.0\"]", SentVariables(handler).GetProperty("a").GetProperty("amounts").GetRawText());
    }

    [Fact]
    public async Task OtherNumbersAndTheBodyShapeAreUnchanged()
    {
        var (client, handler) = Build();
        using (client)
        {
            await client.QueryAsync<JsonElement>("{ x }", new { count = 3, ratio = 0.5 }, "Op");
        }

        var root = JsonDocument.Parse(handler.SentBody!).RootElement;
        Assert.Equal("Op", root.GetProperty("operationName").GetString());
        Assert.Equal(JsonValueKind.Number, root.GetProperty("variables").GetProperty("count").ValueKind);
        Assert.Equal(JsonValueKind.Number, root.GetProperty("variables").GetProperty("ratio").ValueKind);
    }

    [Theory]
    [InlineData("1.50")]
    [InlineData("100.00")]
    [InlineData("12345678901234567890.12")]
    [InlineData("0.0015")]
    public async Task ADecimalResponseIsReadWithItsDigitsAndScale(string digits)
    {
        var (client, _) = Build($"{{\"data\":{{\"amount\":\"{digits}\",\"fxRate\":null}}}}");
        Quote quote;
        using (client)
        {
            quote = await client.QueryAsync<Quote>("{ quote { amount fxRate } }");
        }

        Assert.Equal(digits, quote.Amount.ToString(System.Globalization.CultureInfo.InvariantCulture));
        Assert.Null(quote.FxRate);
    }

    [Fact]
    public async Task ANullableDecimalResponseIsRead()
    {
        var (client, _) = Build("{\"data\":{\"amount\":\"1\",\"fxRate\":\"1.0825\"}}");
        using (client)
        {
            var quote = await client.QueryAsync<Quote>("{ quote { amount fxRate } }");
            Assert.Equal(1.0825m, quote.FxRate);
        }
    }

    [Theory]
    // More significant digits than a .NET decimal holds (28-29): decimal.Parse would round
    // the first and overflow on the second.
    [InlineData("1.234567890123456789012345678901")]
    [InlineData("123456789012345678901234567890")]
    public async Task ADecimalResponseWiderThanADotNetDecimalFailsAndIsNotRounded(string digits)
    {
        var (client, _) = Build($"{{\"data\":{{\"amount\":\"{digits}\",\"fxRate\":null}}}}");
        using (client)
        {
            var error = await Assert.ThrowsAsync<JsonException>(
                () => client.QueryAsync<Quote>("{ quote { amount fxRate } }"));
            Assert.Contains(digits, error.Message);
            Assert.Contains("28", error.Message);
        }
    }

    [Theory]
    [InlineData("1e3")]
    [InlineData("abc")]
    [InlineData("")]
    public async Task AStringThatIsNotDecimalDigitsFails(string text)
    {
        var (client, _) = Build($"{{\"data\":{{\"amount\":\"{text}\",\"fxRate\":null}}}}");
        using (client)
        {
            await Assert.ThrowsAsync<JsonException>(
                () => client.QueryAsync<Quote>("{ quote { amount fxRate } }"));
        }
    }
}

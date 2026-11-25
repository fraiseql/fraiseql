using System.Globalization;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace FraiseQL;

/// <summary>
/// A <c>Decimal</c> on the wire: a JSON string of its exact digits, such as <c>"1.50"</c>.
/// </summary>
/// <remarks>
/// The server takes a <c>Decimal</c> argument as such a string, or an exact integer, and
/// refuses a number with a fraction or an exponent; a response holds each <c>Decimal</c>
/// as that string. A <see cref="decimal"/> is written with every digit and its scale,
/// never an exponent. A string is read only when a <see cref="decimal"/> holds it
/// exactly: a .NET decimal has 28 to 29 significant digits, and <c>decimal.Parse</c>
/// would round a longer value, so a longer value fails instead. A JSON number is read
/// as <see cref="Utf8JsonReader.GetDecimal()"/> reads it.
/// </remarks>
public sealed class DecimalDigitsConverter : JsonConverter<decimal>
{
    private const NumberStyles Digits = NumberStyles.AllowLeadingSign | NumberStyles.AllowDecimalPoint;

    /// <inheritdoc />
    public override decimal Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
    {
        if (reader.TokenType == JsonTokenType.Number)
            return reader.GetDecimal();
        if (reader.TokenType != JsonTokenType.String)
            throw new JsonException($"a Decimal is a string of digits, not {reader.TokenType}");

        var text = reader.GetString() ?? "";
        if (!decimal.TryParse(text, Digits, CultureInfo.InvariantCulture, out var value))
            throw new JsonException($"\"{text}\" is not the digits of a decimal a .NET decimal can hold (at most 28 significant digits)");
        if (!HoldsExactly(text, value))
            throw new JsonException($"\"{text}\" has more digits than a .NET decimal holds (at most 28 significant digits); it would be rounded");
        return value;
    }

    /// <inheritdoc />
    public override void Write(Utf8JsonWriter writer, decimal value, JsonSerializerOptions options)
        => writer.WriteStringValue(value.ToString(CultureInfo.InvariantCulture));

    /// <summary>Whether <paramref name="value"/> writes back as <paramref name="text"/>.</summary>
    private static bool HoldsExactly(string text, decimal value)
    {
        var written = value.ToString(CultureInfo.InvariantCulture);
        // A negative zero ("-0.00") parses to a zero that may write without its sign.
        return written == text || (value == 0m && text.StartsWith('-') && written.TrimStart('-') == text[1..]);
    }
}

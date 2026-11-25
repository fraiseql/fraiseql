namespace FraiseQL

open System
open System.Globalization
open System.Text.Json
open System.Text.Json.Serialization

/// <summary>
/// A <c>Decimal</c> on the wire: a JSON string of its exact digits, such as <c>"1.50"</c>.
/// </summary>
/// <remarks>
/// The server takes a <c>Decimal</c> argument as such a string, or an exact integer, and
/// refuses a number with a fraction or an exponent; a response holds each <c>Decimal</c>
/// as that string. A <c>decimal</c> is written with every digit and its scale, never an
/// exponent. A string is read only when a <c>decimal</c> holds it exactly: a .NET decimal
/// has 28 to 29 significant digits, and <c>Decimal.Parse</c> would round a longer value,
/// so a longer value fails instead. A JSON number is read as
/// <c>Utf8JsonReader.GetDecimal</c> reads it.
/// </remarks>
type DecimalDigitsConverter() =
    inherit JsonConverter<decimal>()

    static let digits = NumberStyles.AllowLeadingSign ||| NumberStyles.AllowDecimalPoint

    /// Whether `value` writes back as `text`; a negative zero ("-0.00") parses to a zero
    /// that may write without its sign.
    static let holdsExactly (text: string) (value: decimal) =
        let written = value.ToString(CultureInfo.InvariantCulture)
        written = text
        || (value = 0m && text.StartsWith('-') && written.TrimStart('-') = text.Substring 1)

    override _.Read(reader, _typeToConvert, _options) =
        match reader.TokenType with
        | JsonTokenType.Number -> reader.GetDecimal()
        | JsonTokenType.String ->
            let text = reader.GetString() |> Option.ofObj |> Option.defaultValue ""
            match Decimal.TryParse(text, digits, CultureInfo.InvariantCulture) with
            | false, _ ->
                raise (JsonException(sprintf "\"%s\" is not the digits of a decimal a .NET decimal can hold (at most 28 significant digits)" text))
            | true, value when not (holdsExactly text value) ->
                raise (JsonException(sprintf "\"%s\" has more digits than a .NET decimal holds (at most 28 significant digits); it would be rounded" text))
            | true, value -> value
        | other -> raise (JsonException(sprintf "a Decimal is a string of digits, not %A" other))

    override _.Write(writer, value, _options) =
        writer.WriteStringValue(value.ToString(CultureInfo.InvariantCulture))

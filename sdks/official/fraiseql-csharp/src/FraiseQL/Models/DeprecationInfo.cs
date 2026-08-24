using System.Text.Json.Serialization;

namespace FraiseQL.Models;

/// <summary>
/// Field deprecation, emitted as the <c>deprecated</c> object the compiler reads.
/// </summary>
/// <param name="Reason">
/// Why the field is deprecated. Absent means deprecated with no stated reason, which is
/// how the compiler models <c>[GraphQLField(Deprecated = "")]</c>.
/// </param>
public record DeprecationInfo(
    [property: JsonPropertyName("reason")] string? Reason = null);

namespace FraiseQL

open System.IO
open System.Text.Json
open System.Text.Json.Serialization

/// Serializes <see cref="IntermediateSchema"/> values to JSON in the format consumed
/// by <c>fraiseql compile</c>. Output uses snake_case keys and omits null fields.
module SchemaExporter =

    /// Custom <see cref="JsonNamingPolicy"/> that converts PascalCase property names
    /// to snake_case. Since all <see cref="IntermediateSchema"/> record fields are
    /// already snake_case, this policy acts as a safety net for any inherited
    /// PascalCase members.
    type SnakeCaseNamingPolicy() =
        inherit JsonNamingPolicy()

        override _.ConvertName(name: string) =
            if System.String.IsNullOrEmpty(name) then
                name
            else
                let sb = System.Text.StringBuilder()

                for i in 0 .. name.Length - 1 do
                    let c = name.[i]

                    if System.Char.IsUpper(c) && i > 0 then
                        sb.Append('_') |> ignore
                        sb.Append(System.Char.ToLowerInvariant(c)) |> ignore
                    else
                        sb.Append(System.Char.ToLowerInvariant(c)) |> ignore

                sb.ToString()

    let private buildOptions (writeIndented: bool) =
        let o = JsonSerializerOptions()
        o.PropertyNamingPolicy <- SnakeCaseNamingPolicy()
        o.WriteIndented <- writeIndented
        o.DefaultIgnoreCondition <- JsonIgnoreCondition.WhenWritingNull
        o

    let private prettyOptions = buildOptions true
    let private compactOptions = buildOptions false

    /// Serializes an <see cref="IntermediateSchema"/> to a pretty-printed JSON string.
    let fromSchema (schema: IntermediateSchema) : string =
        JsonSerializer.Serialize(schema, prettyOptions)

    /// Serializes an <see cref="IntermediateSchema"/> to a pretty-printed JSON string.
    /// Alias for <see cref="fromSchema"/> with an explicit name for API clarity.
    let exportSchema (schema: IntermediateSchema) : string = fromSchema schema

    /// Serializes an <see cref="IntermediateSchema"/> to a compact (non-indented) JSON string.
    let exportSchemaCompact (schema: IntermediateSchema) : string =
        JsonSerializer.Serialize(schema, compactOptions)

    /// Serializes an <see cref="IntermediateSchema"/> and writes it to the given file path.
    /// Creates any missing parent directories.
    let exportSchemaToFile (path: string) (schema: IntermediateSchema) : unit =
        let dir = Path.GetDirectoryName(path)

        if not (System.String.IsNullOrEmpty(dir)) then
            Directory.CreateDirectory(dir) |> ignore

        File.WriteAllText(path, fromSchema schema)

    /// Reads all registered definitions from <see cref="SchemaRegistry"/> and serializes them
    /// to a pretty-printed JSON string.
    let export () : string = fromSchema (SchemaRegistry.toIntermediateSchema ())

    /// Reads all registered definitions from <see cref="SchemaRegistry"/> and writes them
    /// to the given file path. Creates any missing parent directories.
    let exportToFile (path: string) : unit =
        let dir = Path.GetDirectoryName(path)

        if not (System.String.IsNullOrEmpty(dir)) then
            Directory.CreateDirectory(dir) |> ignore

        File.WriteAllText(path, export ())

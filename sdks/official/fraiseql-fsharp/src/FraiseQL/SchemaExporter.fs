namespace FraiseQL

open System.IO
open System.Text.Json
open System.Text.Json.Serialization

/// Serializes <see cref="IntermediateSchema"/> values to JSON in the format consumed
/// by <c>fraiseql compile</c>. Output uses snake_case keys and omits null fields.
/// When inject defaults are configured, they are merged into the exported schema.
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

    /// Writes a single field definition to the JSON writer.
    let private writeField (w: Utf8JsonWriter) (f: FieldDefinition) =
        w.WriteStartObject()
        w.WriteString("name", f.name)
        w.WriteString("type", f.type_)
        w.WriteBoolean("nullable", f.nullable)

        match f.description with
        | Some d -> w.WriteString("description", d)
        | None -> ()

        match f.scope with
        | Some s -> w.WriteString("scope", s)
        | None -> ()

        w.WriteEndObject()

    /// Writes a single type definition to the JSON writer,
    /// conditionally including tenant_scoped only when true.
    let private writeType (w: Utf8JsonWriter) (td: TypeDefinition) =
        w.WriteStartObject()
        w.WriteString("name", td.name)
        w.WriteString("sql_source", td.sql_source)

        match td.description with
        | Some d -> w.WriteString("description", d)
        | None -> ()

        w.WritePropertyName("fields")
        w.WriteStartArray()

        for f in td.fields do
            writeField w f

        w.WriteEndArray()
        w.WriteBoolean("is_input", td.is_input)
        w.WriteBoolean("relay", td.relay)
        w.WriteBoolean("is_error", td.is_error)

        if td.tenant_scoped then
            w.WriteBoolean("tenant_scoped", true)

        if td.key_fields.Length > 0 then
            w.WritePropertyName("key_fields")
            w.WriteStartArray()

            for kf in td.key_fields do
                w.WriteStringValue(kf)

            w.WriteEndArray()

        if td.extends_type then
            w.WriteBoolean("extends", true)

        w.WriteEndObject()

    /// Writes an argument definition to the JSON writer.
    let private writeArgument (w: Utf8JsonWriter) (a: ArgumentDefinition) =
        w.WriteStartObject()
        w.WriteString("name", a.name)
        w.WriteString("type", a.type_)
        w.WriteBoolean("nullable", a.nullable)
        w.WriteEndObject()

    /// Writes a query definition to the JSON writer.
    let private writeQuery (w: Utf8JsonWriter) (q: QueryDefinition) =
        w.WriteStartObject()
        w.WriteString("name", q.name)
        w.WriteString("return_type", q.return_type)
        w.WriteBoolean("returns_list", q.returns_list)
        w.WriteBoolean("nullable", q.nullable)
        w.WriteString("sql_source", q.sql_source)

        w.WritePropertyName("arguments")
        w.WriteStartArray()

        for a in q.arguments do
            writeArgument w a

        w.WriteEndArray()

        match q.cache_ttl_seconds with
        | Some ttl -> w.WriteNumber("cache_ttl_seconds", ttl)
        | None -> ()

        match q.description with
        | Some d -> w.WriteString("description", d)
        | None -> ()

        match q.rest_path with
        | Some path ->
            w.WritePropertyName("rest")
            w.WriteStartObject()
            w.WriteString("path", path)

            let method =
                match q.rest_method with
                | Some m -> m.ToUpperInvariant()
                | None -> "GET"

            w.WriteString("method", method)
            w.WriteEndObject()
        | None -> ()

        w.WriteEndObject()

    /// Writes a mutation definition to the JSON writer.
    let private writeMutation (w: Utf8JsonWriter) (m: MutationDefinition) =
        w.WriteStartObject()
        w.WriteString("name", m.name)
        w.WriteString("return_type", m.return_type)
        w.WriteString("sql_source", m.sql_source)
        w.WriteString("operation", m.operation)

        w.WritePropertyName("arguments")
        w.WriteStartArray()

        for a in m.arguments do
            writeArgument w a

        w.WriteEndArray()

        match m.description with
        | Some d -> w.WriteString("description", d)
        | None -> ()

        match m.rest_path with
        | Some path ->
            w.WritePropertyName("rest")
            w.WriteStartObject()
            w.WriteString("path", path)

            let method =
                match m.rest_method with
                | Some m -> m.ToUpperInvariant()
                | None -> "POST"

            w.WriteString("method", method)
            w.WriteEndObject()
        | None -> ()

        w.WriteEndObject()

    /// Writes a string-to-string dictionary as a JSON object.
    let private writeDictObject (w: Utf8JsonWriter) (name: string) (dict: System.Collections.Generic.IDictionary<string, string>) =
        if dict.Count > 0 then
            w.WritePropertyName(name)
            w.WriteStartObject()

            for kv in dict do
                w.WriteString(kv.Key, kv.Value)

            w.WriteEndObject()

    /// Merges inject defaults from SchemaRegistry into the schema JSON output.
    let private writeInjectDefaults (w: Utf8JsonWriter) =
        let baseD, queriesD, mutationsD = SchemaRegistry.getInjectDefaults ()

        if baseD.Count > 0 || queriesD.Count > 0 || mutationsD.Count > 0 then
            w.WritePropertyName("inject_defaults")
            w.WriteStartObject()
            writeDictObject w "base" baseD
            writeDictObject w "queries" queriesD
            writeDictObject w "mutations" mutationsD
            w.WriteEndObject()

    /// Serializes an IntermediateSchema to JSON using manual writing
    /// for conditional field emission and inject defaults support.
    let private writeSchema (schema: IntermediateSchema) (writeIndented: bool) : string =
        let options = JsonWriterOptions(Indented = writeIndented)
        use stream = new System.IO.MemoryStream()
        use w = new Utf8JsonWriter(stream, options)

        w.WriteStartObject()
        w.WriteString("version", schema.version)

        w.WritePropertyName("types")
        w.WriteStartArray()

        for td in schema.types do
            writeType w td

        w.WriteEndArray()

        w.WritePropertyName("queries")
        w.WriteStartArray()

        for q in schema.queries do
            writeQuery w q

        w.WriteEndArray()

        w.WritePropertyName("mutations")
        w.WriteStartArray()

        for m in schema.mutations do
            writeMutation w m

        w.WriteEndArray()

        writeInjectDefaults w

        w.WriteEndObject()
        w.Flush()

        System.Text.Encoding.UTF8.GetString(stream.ToArray())

    /// Serializes an <see cref="IntermediateSchema"/> to a pretty-printed JSON string.
    let fromSchema (schema: IntermediateSchema) : string =
        writeSchema schema true

    /// Serializes an <see cref="IntermediateSchema"/> to a pretty-printed JSON string.
    /// Alias for <see cref="fromSchema"/> with an explicit name for API clarity.
    let exportSchema (schema: IntermediateSchema) : string = fromSchema schema

    /// Serializes an <see cref="IntermediateSchema"/> to a compact (non-indented) JSON string.
    let exportSchemaCompact (schema: IntermediateSchema) : string =
        writeSchema schema false

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

    /// Writes the ``"federation"`` block into the JSON writer.
    /// Iterates all types, skipping error types, and builds an entity list.
    /// Types with explicit key_fields use those; others default to ``["id"]``.
    let private writeFederation (w: Utf8JsonWriter) (serviceName: string) (types: TypeDefinition list) =
        w.WritePropertyName("federation")
        w.WriteStartObject()
        w.WriteBoolean("enabled", true)
        w.WriteString("service_name", serviceName)
        w.WriteNumber("apollo_version", 2)

        w.WritePropertyName("entities")
        w.WriteStartArray()

        for td in types do
            if not td.is_error then
                w.WriteStartObject()
                w.WriteString("name", td.name)
                w.WritePropertyName("key_fields")
                w.WriteStartArray()

                let keys =
                    if td.key_fields.Length > 0 then td.key_fields
                    else [| "id" |]

                for kf in keys do
                    w.WriteStringValue(kf)

                w.WriteEndArray()
                w.WriteEndObject()

        w.WriteEndArray()
        w.WriteEndObject()

    /// Serializes an IntermediateSchema to JSON with a federation block.
    let private writeSchemaWithFederation (schema: IntermediateSchema) (serviceName: string) (writeIndented: bool) : string =
        let options = JsonWriterOptions(Indented = writeIndented)
        use stream = new System.IO.MemoryStream()
        use w = new Utf8JsonWriter(stream, options)

        w.WriteStartObject()
        w.WriteString("version", schema.version)

        w.WritePropertyName("types")
        w.WriteStartArray()

        for td in schema.types do
            writeType w td

        w.WriteEndArray()

        w.WritePropertyName("queries")
        w.WriteStartArray()

        for q in schema.queries do
            writeQuery w q

        w.WriteEndArray()

        w.WritePropertyName("mutations")
        w.WriteStartArray()

        for m in schema.mutations do
            writeMutation w m

        w.WriteEndArray()

        writeInjectDefaults w
        writeFederation w serviceName schema.types

        w.WriteEndObject()
        w.Flush()

        System.Text.Encoding.UTF8.GetString(stream.ToArray())

    /// Serializes an <see cref="IntermediateSchema"/> to a pretty-printed JSON string
    /// with a federation block. Types without explicit key_fields default to ``["id"]``.
    /// Error types are excluded from the federation entity list.
    let exportSchemaWithFederation (serviceName: string) (schema: IntermediateSchema) : string =
        writeSchemaWithFederation schema serviceName true

    /// Reads all registered definitions from <see cref="SchemaRegistry"/> and serializes
    /// them to a pretty-printed JSON string with a federation block.
    let exportWithFederation (serviceName: string) : string =
        exportSchemaWithFederation serviceName (SchemaRegistry.toIntermediateSchema ())

    /// Reads all registered definitions from <see cref="SchemaRegistry"/> and writes them
    /// to the given file path with a federation block. Creates any missing parent directories.
    let exportWithFederationToFile (serviceName: string) (path: string) : unit =
        let dir = Path.GetDirectoryName(path)

        if not (System.String.IsNullOrEmpty(dir)) then
            Directory.CreateDirectory(dir) |> ignore

        File.WriteAllText(path, exportWithFederation serviceName)

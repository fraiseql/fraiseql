/// FraiseQL F# schema export tool.
///
/// Loads a compiled .NET assembly, discovers all types decorated with
/// <c>[&lt;GraphQLType&gt;]</c>, and writes a <c>schema.json</c> file
/// suitable for passing to <c>fraiseql compile</c>.
///
/// Usage:
///   fraiseql-schema-fsharp export &lt;assembly.dll&gt; [--output &lt;path&gt;] [--compact]
///   fraiseql-schema-fsharp export --help
///   fraiseql-schema-fsharp --version
module FraiseQL.Tool.Program

open System
open System.IO
open System.Reflection

/// Loads all <see cref="FraiseQL.GraphQLTypeAttribute"/>-decorated types from the
/// assembly at <paramref name="assemblyPath"/>.
let private loadTypes (assemblyPath: string) : Type array =
    let asm = Assembly.LoadFrom(Path.GetFullPath(assemblyPath))

    asm.GetTypes()
    |> Array.filter (fun t ->
        t.GetCustomAttributes(typeof<FraiseQL.GraphQLTypeAttribute>, false).Length > 0)

/// Registers all provided types in the <see cref="FraiseQL.SchemaRegistry"/> and
/// returns the serialized schema JSON string.
let private buildJson (types: Type array) (compact: bool) : string =
    FraiseQL.SchemaRegistry.reset ()
    types |> Array.iter FraiseQL.SchemaRegistry.register

    let schema = FraiseQL.SchemaRegistry.toIntermediateSchema ()

    if compact then
        FraiseQL.SchemaExporter.exportSchemaCompact schema
    else
        FraiseQL.SchemaExporter.exportSchema schema

/// Writes output to <paramref name="outputPath"/>, creating any missing parent directories.
let private writeOutput (outputPath: string) (json: string) : unit =
    let dir = Path.GetDirectoryName(outputPath)

    if not (String.IsNullOrEmpty(dir)) then
        Directory.CreateDirectory(dir) |> ignore

    File.WriteAllText(outputPath, json)

/// Exports the schema from the given assembly to the given output path.
/// Returns exit code 0 on success or 1 on error.
let private exportAssembly (assemblyPath: string) (outputPath: string) (compact: bool) : int =
    try
        if not (File.Exists(assemblyPath)) then
            eprintfn "Error: Assembly not found: %s" assemblyPath
            1
        else
            let types = loadTypes assemblyPath
            let json = buildJson types compact
            writeOutput outputPath json
            printfn "Exported schema with %d type(s) to %s" types.Length outputPath
            0
    with ex ->
        eprintfn "Error: %s" ex.Message
        1

let private printUsage () =
    printfn "Usage:"
    printfn "  fraiseql-schema-fsharp export <assembly.dll> [--output <path>] [--compact]"
    printfn ""
    printfn "Commands:"
    printfn "  export    Load a .NET assembly and generate schema.json"
    printfn ""
    printfn "Options:"
    printfn "  --output <path>    Output file path (default: schema.json)"
    printfn "  --compact          Write compact (non-indented) JSON"
    printfn "  --version          Show version information"
    printfn "  --help, -h         Show this help message"

[<EntryPoint>]
let main (argv: string[]) : int =
    match argv with
    | [| "--version" |] ->
        let version = Assembly.GetExecutingAssembly().GetName().Version

        printfn "fraiseql-schema-fsharp %O" version
        0
    | [| "--help" |]
    | [| "-h" |] ->
        printUsage ()
        0
    | [| "export"; "--help" |]
    | [| "export"; "-h" |] ->
        printUsage ()
        0
    | [| "export"; assemblyPath |] -> exportAssembly assemblyPath "schema.json" false
    | [| "export"; assemblyPath; "--output"; outputPath |] ->
        exportAssembly assemblyPath outputPath false
    | [| "export"; assemblyPath; "--compact" |] ->
        exportAssembly assemblyPath "schema.json" true
    | [| "export"; assemblyPath; "--output"; outputPath; "--compact" |]
    | [| "export"; assemblyPath; "--compact"; "--output"; outputPath |] ->
        exportAssembly assemblyPath outputPath true
    | _ ->
        eprintfn "Unknown command or missing arguments."
        eprintfn ""
        printUsage ()
        1

using System.CommandLine;
using FraiseQL.Tool;

var rootCommand = new RootCommand("FraiseQL schema authoring tool");

var exportCommand = new Command("export", "Export schema.json from a compiled assembly");
var assemblyArg = new Argument<FileInfo>("assembly", "Path to the compiled assembly (.dll)");
var outputOpt = new Option<string>(
    new[] { "--output", "-o" },
    getDefaultValue: () => "schema.json",
    description: "Output file path for the exported schema.json");
var compactOpt = new Option<bool>(
    new[] { "--compact" },
    description: "Output compact (non-pretty-printed) JSON");

exportCommand.AddArgument(assemblyArg);
exportCommand.AddOption(outputOpt);
exportCommand.AddOption(compactOpt);

exportCommand.SetHandler((assembly, output, compact) =>
{
    var exitCode = AssemblyLoader.LoadAndExport(assembly.FullName, output, !compact);
    Environment.Exit(exitCode);
}, assemblyArg, outputOpt, compactOpt);

rootCommand.AddCommand(exportCommand);
return await rootCommand.InvokeAsync(args);

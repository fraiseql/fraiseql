namespace FraiseQL

open System.Collections.Generic
open System.IO

/// Minimal TOML configuration loader for FraiseQL settings.
/// Parses the <c>[inject_defaults]</c> section from a <c>fraiseql.toml</c> file
/// and applies the values to <see cref="SchemaRegistry"/>.
module Config =

    /// Represents the three sub-sections of inject_defaults.
    type InjectDefaultsConfig =
        {
            /// Default inject parameters applied to all operations.
            baseDefaults: Dictionary<string, string>
            /// Default inject parameters applied to queries only.
            queries: Dictionary<string, string>
            /// Default inject parameters applied to mutations only.
            mutations: Dictionary<string, string>
        }

    /// Parses a simple TOML file and extracts inject_defaults sections.
    /// Supports [inject_defaults], [inject_defaults.queries], and [inject_defaults.mutations].
    let loadConfig (tomlPath: string) : InjectDefaultsConfig =
        let baseDefaults = Dictionary<string, string>()
        let queriesDefaults = Dictionary<string, string>()
        let mutationsDefaults = Dictionary<string, string>()

        if File.Exists(tomlPath) then
            let lines = File.ReadAllLines(tomlPath)
            let mutable currentSection = ""

            for line in lines do
                let trimmed = line.Trim()

                if trimmed.StartsWith("[") && trimmed.EndsWith("]") then
                    currentSection <- trimmed.[1 .. trimmed.Length - 2].Trim()
                elif trimmed <> "" && not (trimmed.StartsWith("#")) && trimmed.Contains("=") then
                    let eqIdx = trimmed.IndexOf('=')
                    let key = trimmed.[.. eqIdx - 1].Trim()
                    let rawVal = trimmed.[eqIdx + 1 ..].Trim()

                    let value =
                        if rawVal.StartsWith("\"") && rawVal.EndsWith("\"") && rawVal.Length >= 2 then
                            rawVal.[1 .. rawVal.Length - 2]
                        else
                            rawVal

                    match currentSection with
                    | "inject_defaults" -> baseDefaults.[key] <- value
                    | "inject_defaults.queries" -> queriesDefaults.[key] <- value
                    | "inject_defaults.mutations" -> mutationsDefaults.[key] <- value
                    | _ -> ()

        {
            baseDefaults = baseDefaults
            queries = queriesDefaults
            mutations = mutationsDefaults
        }

    /// Loads a TOML configuration file and applies inject_defaults to the SchemaRegistry.
    let loadAndApply (tomlPath: string) : unit =
        let config = loadConfig tomlPath
        SchemaRegistry.setInjectDefaults config.baseDefaults config.queries config.mutations

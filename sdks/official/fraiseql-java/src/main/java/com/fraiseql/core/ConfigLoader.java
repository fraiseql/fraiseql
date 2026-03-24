package com.fraiseql.core;

import java.io.BufferedReader;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.HashMap;
import java.util.Map;

/**
 * Loads FraiseQL configuration from a TOML file.
 * Extracts {@code [inject_defaults]} sections and applies them to the SchemaRegistry.
 *
 * <p>Supported TOML sections:
 * <pre>
 * [inject_defaults]
 * tenant_id = "jwt:tenant_id"
 *
 * [inject_defaults.queries]
 * viewer_id = "jwt:sub"
 *
 * [inject_defaults.mutations]
 * actor_id = "jwt:sub"
 * </pre>
 */
public class ConfigLoader {

    private ConfigLoader() {
        // Utility class
    }

    /**
     * Load configuration from a TOML file and apply inject defaults to the registry.
     *
     * @param tomlPath path to the fraiseql.toml file
     * @throws IOException if reading the file fails
     */
    public static void loadConfig(String tomlPath) throws IOException {
        Map<String, String> base = new HashMap<>();
        Map<String, String> queries = new HashMap<>();
        Map<String, String> mutations = new HashMap<>();

        String currentSection = null;

        try (BufferedReader reader = Files.newBufferedReader(Path.of(tomlPath))) {
            String line;
            while ((line = reader.readLine()) != null) {
                line = line.strip();

                // Skip comments and blank lines
                if (line.isEmpty() || line.startsWith("#")) {
                    continue;
                }

                // Section header
                if (line.startsWith("[")) {
                    currentSection = line.replace("[", "").replace("]", "").strip();
                    continue;
                }

                // Key = "value" pair
                if (currentSection != null && currentSection.startsWith("inject_defaults") && line.contains("=")) {
                    int eqIdx = line.indexOf('=');
                    String key = line.substring(0, eqIdx).strip();
                    String value = line.substring(eqIdx + 1).strip();

                    // Strip surrounding quotes
                    if (value.startsWith("\"") && value.endsWith("\"")) {
                        value = value.substring(1, value.length() - 1);
                    } else if (value.startsWith("'") && value.endsWith("'")) {
                        value = value.substring(1, value.length() - 1);
                    }

                    if ("inject_defaults".equals(currentSection)) {
                        base.put(key, value);
                    } else if ("inject_defaults.queries".equals(currentSection)) {
                        queries.put(key, value);
                    } else if ("inject_defaults.mutations".equals(currentSection)) {
                        mutations.put(key, value);
                    }
                }
            }
        }

        SchemaRegistry.getInstance().setInjectDefaults(base, queries, mutations);
    }
}

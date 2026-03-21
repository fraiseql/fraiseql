package fraiseql

import (
	"bufio"
	"fmt"
	"os"
	"strings"
)

// LoadConfig reads a TOML configuration file and applies inject_defaults
// settings to the global schema registry.
//
// Supported sections:
//
//	[inject_defaults]            -> base defaults (applied to queries AND mutations)
//	[inject_defaults.queries]    -> query-specific overrides
//	[inject_defaults.mutations]  -> mutation-specific overrides
//
// Values use the "source:claim" format, e.g.:
//
//	tenant_id = "jwt:tenant_id"
func LoadConfig(tomlPath string) error {
	f, err := os.Open(tomlPath)
	if err != nil {
		return fmt.Errorf("failed to open config file %s: %w", tomlPath, err)
	}
	defer f.Close()

	base := make(map[string]string)
	queries := make(map[string]string)
	mutations := make(map[string]string)

	var currentSection string

	scanner := bufio.NewScanner(f)
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())

		// Skip empty lines and comments
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}

		// Section header
		if strings.HasPrefix(line, "[") && strings.HasSuffix(line, "]") {
			currentSection = strings.TrimSpace(line[1 : len(line)-1])
			continue
		}

		// Key = value pair — only process within inject_defaults sections
		if !strings.HasPrefix(currentSection, "inject_defaults") {
			continue
		}

		eqIdx := strings.Index(line, "=")
		if eqIdx < 0 {
			continue
		}

		key := strings.TrimSpace(line[:eqIdx])
		value := strings.TrimSpace(line[eqIdx+1:])

		// Strip surrounding quotes (single or double)
		value = stripQuotes(value)

		switch currentSection {
		case "inject_defaults":
			base[key] = value
		case "inject_defaults.queries":
			queries[key] = value
		case "inject_defaults.mutations":
			mutations[key] = value
		}
	}

	if err := scanner.Err(); err != nil {
		return fmt.Errorf("error reading config file %s: %w", tomlPath, err)
	}

	// Only call SetInjectDefaults if at least one section had entries
	if len(base) > 0 || len(queries) > 0 || len(mutations) > 0 {
		SetInjectDefaults(base, queries, mutations)
	}

	return nil
}

// stripQuotes removes surrounding double or single quotes from a string.
func stripQuotes(s string) string {
	if len(s) >= 2 {
		if (s[0] == '"' && s[len(s)-1] == '"') || (s[0] == '\'' && s[len(s)-1] == '\'') {
			return s[1 : len(s)-1]
		}
	}
	return s
}

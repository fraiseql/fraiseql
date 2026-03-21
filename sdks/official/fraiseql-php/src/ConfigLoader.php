<?php

declare(strict_types=1);

namespace FraiseQL;

/**
 * Utility class that loads `fraiseql.toml` and parses the [inject_defaults]
 * section, then configures the SchemaRegistry with the parsed values.
 *
 * Supports three TOML sections:
 * - [inject_defaults]           -> base defaults for all operations
 * - [inject_defaults.queries]   -> additional defaults for queries
 * - [inject_defaults.mutations] -> additional defaults for mutations
 *
 * Each section contains key = "value" pairs (e.g. tenant_id = "jwt:tenant_id").
 *
 * Usage:
 * ```php
 * ConfigLoader::load('fraiseql.toml');
 * // Registry now has inject defaults configured
 * ```
 */
final class ConfigLoader
{
    /**
     * Load a fraiseql.toml file and apply inject_defaults to the SchemaRegistry.
     *
     * @param string $path Path to the fraiseql.toml file
     * @return void
     *
     * @throws FraiseQLException If the file cannot be read
     */
    public static function load(string $path): void
    {
        $content = file_get_contents($path);
        if ($content === false) {
            throw new FraiseQLException("Failed to read config file: $path");
        }

        $sections = self::parseInjectDefaults($content);

        SchemaRegistry::getInstance()->setInjectDefaults(
            $sections['base'],
            $sections['queries'],
            $sections['mutations'],
        );
    }

    /**
     * Parse inject_defaults sections from TOML content using line-by-line parsing.
     *
     * @param string $content The TOML file content
     * @return array{base: array<string, string>, queries: array<string, string>, mutations: array<string, string>}
     */
    private static function parseInjectDefaults(string $content): array
    {
        $base = [];
        $queries = [];
        $mutations = [];

        $currentSection = null;
        $lines = explode("\n", $content);

        foreach ($lines as $line) {
            $trimmed = trim($line);

            // Skip empty lines and comments
            if ($trimmed === '' || str_starts_with($trimmed, '#')) {
                continue;
            }

            // Detect section headers
            if (str_starts_with($trimmed, '[')) {
                $currentSection = self::parseSectionHeader($trimmed);
                continue;
            }

            // Parse key = "value" pairs within inject_defaults sections
            if ($currentSection !== null && str_contains($trimmed, '=')) {
                $pair = self::parseKeyValue($trimmed);
                if ($pair !== null) {
                    match ($currentSection) {
                        'base' => $base[$pair[0]] = $pair[1],
                        'queries' => $queries[$pair[0]] = $pair[1],
                        'mutations' => $mutations[$pair[0]] = $pair[1],
                        default => null,
                    };
                }
            }
        }

        return ['base' => $base, 'queries' => $queries, 'mutations' => $mutations];
    }

    /**
     * Parse a TOML section header and return the inject_defaults sub-section name.
     *
     * @param string $header The section header line (e.g. "[inject_defaults.queries]")
     * @return string|null The sub-section name ('base', 'queries', 'mutations') or null
     */
    private static function parseSectionHeader(string $header): ?string
    {
        // Remove brackets and whitespace
        $section = trim($header, "[] \t");

        if ($section === 'inject_defaults') {
            return 'base';
        }

        if ($section === 'inject_defaults.queries') {
            return 'queries';
        }

        if ($section === 'inject_defaults.mutations') {
            return 'mutations';
        }

        return null;
    }

    /**
     * Parse a TOML key = "value" line.
     *
     * @param string $line The line to parse
     * @return array{0: string, 1: string}|null The [key, value] pair or null
     */
    private static function parseKeyValue(string $line): ?array
    {
        $eqPos = strpos($line, '=');
        if ($eqPos === false) {
            return null;
        }

        $key = trim(substr($line, 0, $eqPos));
        $value = trim(substr($line, $eqPos + 1));

        // Strip surrounding quotes (single or double)
        if (
            (str_starts_with($value, '"') && str_ends_with($value, '"'))
            || (str_starts_with($value, "'") && str_ends_with($value, "'"))
        ) {
            $value = substr($value, 1, -1);
        }

        return [$key, $value];
    }
}

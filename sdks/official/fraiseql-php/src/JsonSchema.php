<?php

declare(strict_types=1);

namespace FraiseQL;

/**
 * An author-side handle on a FraiseQL schema document — the `schema.json` that
 * `SchemaExporter::export()` produces and `fraiseql compile` reads.
 *
 * Obtain one from the exporter, not by hand:
 *
 * ```php
 * $schema = JsonSchema::fromJson(SchemaExporter::export());   // or ::fromArray(SchemaExporter::toArray())
 * $schema->getTypeNames();                                    // ['User', 'Post']
 * $schema->saveToFile('schema.json');                         // still compiles
 * ```
 *
 * **The document is carried verbatim.** `toArray()` returns what came in, key for key,
 * and `toJson()` / `saveToFile()` round-trip it unchanged. That is the whole design
 * constraint, and it is not decorative: the compiler's `IntermediateSchema` declares
 * `#[serde(deny_unknown_fields)]`, so a value type that drops a key it does not model —
 * or adds one it invents — writes a document `fraiseql compile` either misreads or
 * refuses outright.
 *
 * This class used to do exactly that. It modelled `SchemaFormatter`'s document — types as
 * a map keyed by name, a `scalars` map, `version` 1.0 — which #1245 removed for being
 * uncompilable. Against the document the SDK actually produces, `getTypeNames()` returned
 * list indices (`[0, 1]`) rather than names, `getType('User')` was null, and a round trip
 * through `saveToFile()` **dropped `queries` and `mutations` entirely** and emitted a
 * `scalars` key, which `fraiseql compile` then refused with
 * `unknown field 'scalars'`. Its whole test suite was green, because every fixture in it
 * was hand-built in the map shape no producer emits (#1264).
 */
final class JsonSchema
{
    /**
     * The schema document, verbatim.
     *
     * @var array<string, mixed>
     */
    private readonly array $document;

    /** Schema format version, as declared by the document. */
    public readonly string $version;

    /**
     * @param array<string, mixed> $document A FraiseQL schema document
     *
     * @throws FraiseQLException If a top-level collection is not a JSON array
     */
    private function __construct(array $document)
    {
        // A map here means the caller built the pre-#1245 shape by hand. Naming it is
        // worth more than accepting it: accepted, it survives as far as `saveToFile()`
        // and fails in the compiler, one layer away from the mistake.
        foreach (['types', 'queries', 'mutations', 'input_types', 'subscriptions'] as $collection) {
            $value = $document[$collection] ?? null;
            if ($value === null) {
                continue;
            }
            if (!is_array($value) || !array_is_list($value)) {
                throw new FraiseQLException(
                    "Schema document key '$collection' must be a JSON array of objects, not an "
                    . 'object keyed by name. The compiler reads a list here; a map keyed by name '
                    . 'is the pre-2.0.0 shape and does not compile.',
                );
            }
        }

        $version = $document['version'] ?? '2.0.0';
        if (!is_string($version)) {
            throw new FraiseQLException("Schema document key 'version' must be a string");
        }

        $this->document = $document;
        $this->version = $version;
    }

    /**
     * Wrap a schema document already in array form.
     *
     * `SchemaExporter::toArray()` is the producer; this is the allocation-free path when
     * the document does not need to become a string first.
     *
     * @param array<string, mixed> $document
     * @return self
     *
     * @throws FraiseQLException If the document shape is not a schema document
     */
    public static function fromArray(array $document): self
    {
        return new self($document);
    }

    /**
     * Parse a schema document from a JSON string.
     *
     * `SchemaExporter::export()` is the producer.
     *
     * @param string $json The JSON string
     * @return self
     *
     * @throws FraiseQLException If JSON parsing fails or the shape is not a schema document
     */
    public static function fromJson(string $json): self
    {
        $data = json_decode($json, true);

        if ($data === null) {
            throw new FraiseQLException('Failed to decode JSON: ' . json_last_error_msg());
        }

        if (!is_array($data) || array_is_list($data)) {
            throw new FraiseQLException('A schema document must decode to a JSON object');
        }

        return new self($data);
    }

    /**
     * Read a schema document from a file.
     *
     * @param string $filePath The file path to load from
     * @return self
     *
     * @throws FraiseQLException If file read or JSON parsing fails
     */
    public static function loadFromFile(string $filePath): self
    {
        if (!file_exists($filePath)) {
            throw new FraiseQLException("Schema file not found: $filePath");
        }

        $json = file_get_contents($filePath);

        if ($json === false) {
            throw new FraiseQLException("Failed to read schema file: $filePath");
        }

        return self::fromJson($json);
    }

    /**
     * The document, verbatim — same keys, same order, same values.
     *
     * @return array<string, mixed>
     */
    public function toArray(): array
    {
        return $this->document;
    }

    /**
     * The document as JSON, byte-for-byte re-encodable by the compiler.
     *
     * @param int $flags JSON_* flags for json_encode
     * @return string
     *
     * @throws FraiseQLException If encoding fails
     */
    public function toJson(int $flags = JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES): string
    {
        // Through `toArray()`, not `$this->document`: two projections of the same value
        // are two things that can drift apart, and the one that reaches a file is the one
        // that matters. A single accessor means a regression in either shows up in both.
        $json = json_encode($this->toArray(), $flags);

        if ($json === false) {
            throw new FraiseQLException('Failed to encode schema to JSON: ' . json_last_error_msg());
        }

        return $json;
    }

    /**
     * Write the document to a file.
     *
     * @param string $filePath The file path to save to
     * @param int $flags JSON_* flags for json_encode
     * @return int The number of bytes written
     *
     * @throws FraiseQLException If file write fails
     */
    public function saveToFile(string $filePath, int $flags = JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES): int
    {
        $json = $this->toJson($flags);
        $bytes = file_put_contents($filePath, $json);

        if ($bytes === false) {
            throw new FraiseQLException("Failed to write schema to file: $filePath");
        }

        return $bytes;
    }

    /**
     * Names of the object types the document declares, in document order.
     *
     * @return array<string>
     */
    public function getTypeNames(): array
    {
        return self::namesOf($this->collection('types'));
    }

    /**
     * A type definition by name.
     *
     * @param string $typeName The type name
     * @return array<string, mixed>|null The type definition, or null if absent
     */
    public function getType(string $typeName): ?array
    {
        foreach ($this->collection('types') as $type) {
            if (is_array($type) && ($type['name'] ?? null) === $typeName) {
                /** @var array<string, mixed> $type */
                return $type;
            }
        }

        return null;
    }

    /**
     * Whether the document declares a type by this name.
     *
     * @param string $typeName The type name
     * @return bool
     */
    public function hasType(string $typeName): bool
    {
        return $this->getType($typeName) !== null;
    }

    /**
     * How many object types the document declares.
     *
     * @return int
     */
    public function getTypeCount(): int
    {
        return count($this->collection('types'));
    }

    /**
     * Names of the root queries the document declares, in document order.
     *
     * @return array<string>
     */
    public function getQueryNames(): array
    {
        return self::namesOf($this->collection('queries'));
    }

    /**
     * Names of the root mutations the document declares, in document order.
     *
     * @return array<string>
     */
    public function getMutationNames(): array
    {
        return self::namesOf($this->collection('mutations'));
    }

    /**
     * A top-level list, or the empty list when the key is absent.
     *
     * @param string $key
     * @return array<int, mixed>
     */
    private function collection(string $key): array
    {
        $value = $this->document[$key] ?? [];

        if (!is_array($value)) {
            return [];
        }

        /** @var array<int, mixed> $value — the constructor refused any non-list */
        return $value;
    }

    /**
     * @param array<int, mixed> $entries
     * @return array<string>
     */
    private static function namesOf(array $entries): array
    {
        $names = [];

        foreach ($entries as $entry) {
            if (is_array($entry) && isset($entry['name']) && is_string($entry['name'])) {
                $names[] = $entry['name'];
            }
        }

        return $names;
    }
}

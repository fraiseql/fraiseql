<?php

declare(strict_types=1);

namespace FraiseQL;

use ReflectionProperty;
use ReflectionType;
use ReflectionUnionType;
use ReflectionNamedType;
use FraiseQL\Attributes\GraphQLField;

/**
 * Converts PHP types to GraphQL types with reflection and attribute support.
 *
 * This class handles:
 * - Built-in type conversion (int → Int, string → String, etc.)
 * - Custom class type detection
 * - Nullable type handling
 * - List/array type detection
 * - GraphQL attribute parsing
 */
final class TypeConverter
{
    /**
     * Mapping from PHP type names to GraphQL scalar types.
     */
    private const PHP_TO_GRAPHQL_MAP = [
        'int' => 'Int',
        'string' => 'String',
        'bool' => 'Boolean',
        'float' => 'Float',
        'double' => 'Float',
        'mixed' => 'String',
        // PHP's date-time classes are the engine's DateTime (#1530): DateTimeImmutable and
        // DateTimeInterface used to export as their class names.
        'DateTime' => 'DateTime',
        'DateTimeImmutable' => 'DateTime',
        'DateTimeInterface' => 'DateTime',
    ];

    /**
     * Convert a ReflectionProperty to TypeInfo by analyzing its type hints and attributes.
     *
     * @param ReflectionProperty $property The property to convert
     * @return TypeInfo The converted type information
     * @throws \Exception If scope validation fails
     */
    public static function fromReflectionProperty(ReflectionProperty $property): TypeInfo
    {
        // Check for GraphQLField attribute
        $graphQLFieldAttribute = null;
        foreach ($property->getAttributes(GraphQLField::class) as $attribute) {
            /** @var GraphQLField $graphQLFieldAttribute */
            $graphQLFieldAttribute = $attribute->newInstance();
            break;
        }

        // Validate scopes if present
        if ($graphQLFieldAttribute !== null) {
            if ($graphQLFieldAttribute->scope !== null && $graphQLFieldAttribute->scopes !== null) {
                throw new \Exception(
                    "Field {$property->getName()} cannot have both scope and scopes"
                );
            }

            if ($graphQLFieldAttribute->scope !== null) {
                self::validateScope($graphQLFieldAttribute->scope, $property->getName());
            }

            if ($graphQLFieldAttribute->scopes !== null) {
                if (empty($graphQLFieldAttribute->scopes)) {
                    throw new \Exception(
                        "Field {$property->getName()} has empty scopes array"
                    );
                }
                foreach ($graphQLFieldAttribute->scopes as $scope) {
                    if (empty($scope)) {
                        throw new \Exception(
                            "Field {$property->getName()} has empty scope in scopes array"
                        );
                    }
                    self::validateScope($scope, $property->getName());
                }
            }
        }

        $type = $property->getType();

        // A list position is non-null unless PHP marks it nullable (#1586, #1608): `?array`
        // for the list itself, the docblock generic (`list<?float>`) for its element, the
        // place PHPStan, Psalm and IDEs read an array's element type. A type the attribute
        // names is published as written (an `array` may be a `Vector`), below.
        if ($type instanceof ReflectionNamedType && $type->getName() === 'array'
            && $graphQLFieldAttribute?->type === null) {
            return self::canonicalizeIdField(
                self::fromArrayProperty($property, $type, $graphQLFieldAttribute),
                $property->getName(),
            );
        }

        // If explicit type is provided in attribute, use it. The field is nullable when the
        // attribute says so or the property's own type does (`?string`).
        if ($graphQLFieldAttribute !== null && $graphQLFieldAttribute->type !== null) {
            return self::canonicalizeIdField(
                new TypeInfo(
                    phpType: $property->getName(),
                    graphQLType: $graphQLFieldAttribute->type,
                    isNullable: $graphQLFieldAttribute->nullable || ($type?->allowsNull() ?? false),
                    description: $graphQLFieldAttribute->description,
                    scope: $graphQLFieldAttribute->scope,
                    scopes: $graphQLFieldAttribute->scopes,
                ),
                $property->getName(),
            );
        }

        // Otherwise, extract from reflection
        if ($type !== null) {
            return self::canonicalizeIdField(
                self::fromReflectionType($type, $graphQLFieldAttribute),
                $property->getName(),
            );
        }

        // Fallback for untyped properties
        return self::canonicalizeIdField(
            new TypeInfo(
                phpType: 'mixed',
                graphQLType: 'String',
                isNullable: true,
                description: $graphQLFieldAttribute?->description,
                scope: $graphQLFieldAttribute?->scope,
                scopes: $graphQLFieldAttribute?->scopes,
            ),
            $property->getName(),
        );
    }

    /**
     * Wire-transparent string representations of an identity: PHP `string` maps to
     * GraphQL `String`, and the explicit `UUID` scalar both serialize as a JSON
     * string, identical to GraphQL `ID`.
     */
    private const STRING_SHAPED_ID_TYPES = ['String', 'UUID'];

    /**
     * Enforce the entity-identity convention: a field named `id` is emitted as `ID`.
     *
     * A global `id: ID!` is the identity contract the FraiseQL compiler enforces
     * (Node / CascadeNode / federation `@key(fields: "id")` — see fraiseql-core
     * ADR-0017). `string` and the explicit `UUID` scalar are wire-identical string
     * representations of an identity, so an `id` typed either way is canonicalized to
     * `ID` at authoring time — keeping the emitted schema.json honest instead of
     * leaking `id: String` that the compiler would then reject. A numeric `id: int`
     * is left as `Int` (not wire-compatible with `ID`); the compiler flags it if the
     * type opts into a Node-style interface, prompting a real `ID` identity.
     *
     * Nullability and all other metadata are preserved.
     *
     * @param TypeInfo $typeInfo The type information to canonicalize
     * @param string $fieldName The emitted GraphQL field name
     * @return TypeInfo The canonicalized type information
     */
    private static function canonicalizeIdField(TypeInfo $typeInfo, string $fieldName): TypeInfo
    {
        if ($fieldName !== 'id') {
            return $typeInfo;
        }

        if (!in_array($typeInfo->graphQLType, self::STRING_SHAPED_ID_TYPES, true)) {
            return $typeInfo;
        }

        return new TypeInfo(
            phpType: $typeInfo->phpType,
            graphQLType: 'ID',
            isNullable: $typeInfo->isNullable,
            isList: $typeInfo->isList,
            description: $typeInfo->description,
            scope: $typeInfo->scope,
            scopes: $typeInfo->scopes,
        );
    }

    /**
     * An `array` property's GraphQL list type, from the docblock's `@var` (`list<?float>`,
     * `array<int, string>`, `int[]`, nested at any depth).
     *
     * @throws \InvalidArgumentException when the docblock does not say what the elements are
     */
    private static function fromArrayProperty(
        \ReflectionProperty $property,
        ReflectionType $type,
        ?GraphQLField $attribute,
    ): TypeInfo {
        $where = $property->getDeclaringClass()->getShortName() . '::$' . $property->getName();
        $declared = self::docVarType($property);
        if ($declared === null) {
            throw new \InvalidArgumentException(
                "$where is an array with no element type to export; declare it in the docblock "
                . "(/** @var list<int> */), or name the GraphQL type (#[GraphQLField(type: '[Int!]')])"
            );
        }
        [$graphQLType, $docNullable] = self::docType($declared, $where);
        if (!str_starts_with($graphQLType, '[')) {
            throw new \InvalidArgumentException(
                "$where is an array but its @var type `$declared` is not a list type"
            );
        }

        return new TypeInfo(
            phpType: 'array',
            graphQLType: $graphQLType,
            isNullable: $type->allowsNull() || $docNullable || ($attribute?->nullable ?? false),
            isList: true,
            description: $attribute?->description,
            scope: $attribute?->scope,
            scopes: $attribute?->scopes,
        );
    }

    /** The type expression of a property's docblock `@var`, if it has one. */
    private static function docVarType(\ReflectionProperty $property): ?string
    {
        $doc = $property->getDocComment();
        if ($doc === false || !preg_match('/@var\s+/', $doc, $m, PREG_OFFSET_CAPTURE)) {
            return null;
        }
        $start = $m[0][1] + strlen($m[0][0]);
        $depth = 0;
        $end = $start;
        $length = strlen($doc);
        for (; $end < $length; $end++) {
            $c = $doc[$end];
            if ($c === '<' || $c === '(') {
                $depth++;
            } elseif ($c === '>' || $c === ')') {
                $depth--;
            } elseif ($depth === 0 && ctype_space($c)) {
                break;
            }
        }
        $expression = rtrim(substr($doc, $start, $end - $start), '*/');
        return $expression === '' ? null : $expression;
    }

    /**
     * A docblock type expression as a GraphQL type: `[GraphQLType, nullable, nullable]`, the
     * last two the same (whether this position may be null), every list level with its items'
     * nullability.
     *
     * @return array{0: string, 1: bool, 2: bool}
     */
    private static function docType(string $expression, string $where): array
    {
        $t = trim($expression);
        $nullable = false;
        if (str_starts_with($t, '?')) {
            $nullable = true;
            $t = trim(substr($t, 1));
        }
        $parts = self::splitTop($t, '|');
        if (count($parts) > 1) {
            $nonNull = array_values(array_filter($parts, static fn (string $p) => strtolower(trim($p)) !== 'null'));
            $nullable = $nullable || count($nonNull) !== count($parts);
            if (count($nonNull) !== 1) {
                throw new \InvalidArgumentException("$where: `$expression` is a union GraphQL cannot express");
            }
            $t = trim($nonNull[0]);
        }
        if (str_starts_with($t, '(') && str_ends_with($t, ')') && count(self::splitTop($t, '|')) === 1) {
            [$inner, $innerNullable] = self::docType(substr($t, 1, -1), $where);
            return [$inner, $nullable || $innerNullable, $nullable || $innerNullable];
        }
        if (str_ends_with($t, '[]')) {
            return [self::listOf(substr($t, 0, -2), $where), $nullable, $nullable];
        }
        if (preg_match('/^(?:non-empty-)?(?:list|array|iterable)<(.*)>$/s', $t, $m)) {
            $arguments = self::splitTop($m[1], ',');
            return [self::listOf((string) end($arguments), $where), $nullable, $nullable];
        }
        $name = ltrim($t, '\\');
        if (in_array(strtolower($name), ['array', 'mixed', 'iterable'], true)) {
            throw new \InvalidArgumentException(
                "$where: `$expression` has no element type to export; write it (list<int>)"
            );
        }
        $short = str_contains($name, '\\') ? substr($name, strrpos($name, '\\') + 1) : $name;
        return [self::PHP_TO_GRAPHQL_MAP[strtolower($short)] ?? $short, $nullable, $nullable];
    }

    /** `[T!]` or `[T]`, `T` the GraphQL type of the element expression. */
    private static function listOf(string $element, string $where): string
    {
        [$type, $elementNullable] = self::docType($element, $where);
        return '[' . $type . ($elementNullable ? '' : '!') . ']';
    }

    /**
     * Split `$s` on `$separator` outside `<>` and `()`.
     *
     * @return list<string>
     */
    private static function splitTop(string $s, string $separator): array
    {
        $parts = [];
        $depth = 0;
        $current = '';
        for ($i = 0, $n = strlen($s); $i < $n; $i++) {
            $c = $s[$i];
            if ($c === '<' || $c === '(') {
                $depth++;
            } elseif ($c === '>' || $c === ')') {
                $depth--;
            }
            if ($c === $separator && $depth === 0) {
                $parts[] = $current;
                $current = '';
                continue;
            }
            $current .= $c;
        }
        $parts[] = $current;
        return $parts;
    }

    /**
     * Convert a ReflectionType to TypeInfo.
     *
     * @param ReflectionType $type The reflection type to convert
     * @param GraphQLField|null $fieldAttribute Optional field attribute for additional metadata
     * @return TypeInfo
     */
    public static function fromReflectionType(
        ReflectionType $type,
        ?GraphQLField $fieldAttribute = null,
    ): TypeInfo {
        $isNullable = $type->allowsNull();
        $isList = false;

        if ($type instanceof ReflectionUnionType) {
            // Handle union types — getTypes() returns ReflectionNamedType[] for union types
            $types = $type->getTypes();
            $nonNullTypes = array_values(array_filter(
                $types,
                static fn (\ReflectionNamedType|\ReflectionIntersectionType $t) =>
                    $t instanceof ReflectionNamedType && $t->getName() !== 'null',
            ));

            if (count($nonNullTypes) === 1) {
                // Union with null = nullable
                $type = $nonNullTypes[0];
                $isNullable = true;
            } else {
                // Multiple non-null types = use String as fallback
                return new TypeInfo(
                    phpType: 'mixed',
                    graphQLType: 'String',
                    isNullable: true,
                    description: $fieldAttribute?->description,
                    scope: $fieldAttribute?->scope,
                    scopes: $fieldAttribute?->scopes,
                );
            }
        }

        if ($type instanceof ReflectionNamedType) {
            $typeName = $type->getName();

            // Handle array type
            if ($typeName === 'array') {
                return new TypeInfo(
                    phpType: 'array',
                    graphQLType: $fieldAttribute?->type ?? 'String',
                    isNullable: $isNullable,
                    isList: true,
                    description: $fieldAttribute?->description,
                    scope: $fieldAttribute?->scope,
                    scopes: $fieldAttribute?->scopes,
                );
            }

            // Handle built-in types
            $graphQLType = self::PHP_TO_GRAPHQL_MAP[$typeName] ?? $typeName;

            return new TypeInfo(
                phpType: $typeName,
                graphQLType: $graphQLType,
                isNullable: $isNullable,
                description: $fieldAttribute?->description,
                scope: $fieldAttribute?->scope,
                scopes: $fieldAttribute?->scopes,
            );
        }

        // Fallback
        return new TypeInfo(
            phpType: 'mixed',
            graphQLType: 'String',
            isNullable: true,
            description: $fieldAttribute?->description,
            scope: $fieldAttribute?->scope,
            scopes: $fieldAttribute?->scopes,
        );
    }

    /**
     * Validate scope format: action:resource
     * Valid patterns:
     * - * (global wildcard)
     * - action:resource (read:user.email, write:User.salary)
     * - action:* (admin:*, read:*)
     *
     * @param string $scope The scope to validate
     * @param string $fieldName The field name for error reporting
     * @throws \Exception If scope format is invalid
     */
    private static function validateScope(string $scope, string $fieldName): void
    {
        if (empty($scope)) {
            throw new \Exception("Field {$fieldName} has empty scope");
        }

        // Global wildcard is always valid
        if ($scope === '*') {
            return;
        }

        // Must contain at least one colon
        if (!str_contains($scope, ':')) {
            throw new \Exception(
                "Field {$fieldName} has invalid scope '{$scope}' (missing colon)"
            );
        }

        [$action, $resource] = explode(':', $scope, 2);

        // Validate action: [a-zA-Z_][a-zA-Z0-9_]*
        if (!self::isValidAction($action)) {
            throw new \Exception(
                "Field {$fieldName} has invalid action in scope '{$scope}' (must be alphanumeric + underscore)"
            );
        }

        // Validate resource: [a-zA-Z_][a-zA-Z0-9_.]*|*
        if (!self::isValidResource($resource)) {
            throw new \Exception(
                "Field {$fieldName} has invalid resource in scope '{$scope}' (must be alphanumeric + underscore + dot, or *)"
            );
        }
    }

    /**
     * Check if action matches [a-zA-Z_][a-zA-Z0-9_]*
     */
    private static function isValidAction(string $action): bool
    {
        if (empty($action)) {
            return false;
        }

        // First character must be letter or underscore
        $firstChar = $action[0];
        if (!ctype_alpha($firstChar) && $firstChar !== '_') {
            return false;
        }

        // Rest must be letters, digits, or underscores
        for ($i = 1; $i < strlen($action); $i++) {
            $char = $action[$i];
            if (!ctype_alnum($char) && $char !== '_') {
                return false;
            }
        }

        return true;
    }

    /**
     * Check if resource matches [a-zA-Z_][a-zA-Z0-9_.]*|*
     */
    private static function isValidResource(string $resource): bool
    {
        if ($resource === '*') {
            return true;
        }

        if (empty($resource)) {
            return false;
        }

        // First character must be letter or underscore
        $firstChar = $resource[0];
        if (!ctype_alpha($firstChar) && $firstChar !== '_') {
            return false;
        }

        // Rest must be letters, digits, underscores, or dots
        for ($i = 1; $i < strlen($resource); $i++) {
            $char = $resource[$i];
            if (!ctype_alnum($char) && $char !== '_' && $char !== '.') {
                return false;
            }
        }

        return true;
    }

    /**
     * Convert a PHP type string (including unions) to TypeInfo.
     *
     * Examples:
     * - "int" → TypeInfo(phpType: 'int', graphQLType: 'Int', isNullable: false)
     * - "?string" → TypeInfo(phpType: 'string', graphQLType: 'String', isNullable: true)
     * - "User|null" → TypeInfo(phpType: 'User', graphQLType: 'User', isNullable: true)
     *
     * @param string $typeString The PHP type string to convert
     * @return TypeInfo
     */
    public static function fromTypeString(string $typeString): TypeInfo
    {
        return TypeInfo::fromString($typeString);
    }

    /**
     * Check if a PHP type is a built-in scalar type.
     *
     * @param string $typeName The type name to check
     * @return bool
     */
    public static function isScalarType(string $typeName): bool
    {
        return isset(self::PHP_TO_GRAPHQL_MAP[$typeName]);
    }

    /**
     * Check if a type is a list/array type.
     *
     * @param ReflectionType|null $type The type to check
     * @return bool
     */
    public static function isListType(?ReflectionType $type): bool
    {
        if ($type instanceof ReflectionNamedType) {
            return $type->getName() === 'array';
        }

        return false;
    }
}

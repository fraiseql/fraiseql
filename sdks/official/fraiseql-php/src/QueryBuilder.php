<?php

declare(strict_types=1);

namespace FraiseQL;

/**
 * Fluent builder for GraphQL query definitions.
 *
 * Usage:
 * ```php
 * StaticAPI::query('users')
 *     ->returnType('User')
 *     ->returnsList(true)
 *     ->sqlSource('v_user')
 *     ->cacheTtlSeconds(300)
 *     ->register();
 * ```
 */
final class QueryBuilder
{
    private string $returnTypeValue = '';
    private bool $returnsListValue = false;
    private bool $nullableValue = false;
    private ?string $sqlSourceValue = null;
    private ?string $descriptionValue = null;
    private bool $autoParamsValue = false;

    /** @var array<string, array{type: string, nullable: bool, default: mixed}> */
    private array $arguments = [];

    /** @var array<string, string> */
    private array $injectMap = [];

    private ?int $cacheTtlSecondsValue = null;

    /** @var array<string> */
    private array $additionalViewsList = [];

    private ?string $requiresRoleValue = null;
    private ?string $deprecationReason = null;
    private ?string $relayCursorTypeValue = null;
    private ?string $restPathValue = null;
    private ?string $restMethodValue = null;

    private const array VALID_REST_METHODS = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE'];

    private function __construct(private readonly string $name)
    {
    }

    public static function query(string $name): self
    {
        return new self($name);
    }

    public function returnType(string $type): self
    {
        $this->returnTypeValue = $type;
        return $this;
    }

    public function returnsList(bool $isList = true): self
    {
        $this->returnsListValue = $isList;
        return $this;
    }

    /**
     * Set whether this query can return null.
     *
     * @param bool $nullable Whether the result is nullable
     * @return self Fluent interface
     */
    public function nullable(bool $nullable = true): self
    {
        $this->nullableValue = $nullable;
        return $this;
    }

    public function sqlSource(string $source): self
    {
        $this->sqlSourceValue = $source;
        return $this;
    }

    public function description(string $desc): self
    {
        $this->descriptionValue = $desc;
        return $this;
    }

    public function autoParams(bool $auto = true): self
    {
        $this->autoParamsValue = $auto;
        return $this;
    }

    public function argument(string $argName, string $type, bool $nullable = true, mixed $default = null): self
    {
        $this->arguments[$argName] = ['type' => $type, 'nullable' => $nullable, 'default' => $default];
        return $this;
    }

    /**
     * Inject JWT claims as query parameters.
     *
     * @param array<string, string> $inject Map of param name to 'jwt:<claim>'
     */
    public function inject(array $inject): self
    {
        $this->injectMap = $inject;
        return $this;
    }

    public function cacheTtlSeconds(int $ttl): self
    {
        $this->cacheTtlSecondsValue = $ttl;
        return $this;
    }

    /**
     * @param array<string> $views
     */
    public function additionalViews(array $views): self
    {
        $this->additionalViewsList = $views;
        return $this;
    }

    public function requiresRole(string $role): self
    {
        $this->requiresRoleValue = $role;
        return $this;
    }

    public function deprecated(string $reason): self
    {
        $this->deprecationReason = $reason;
        return $this;
    }

    public function relayCursorType(string $type): self
    {
        $this->relayCursorTypeValue = $type;
        return $this;
    }

    public function restPath(string $path): self
    {
        $this->restPathValue = $path;
        return $this;
    }

    public function restMethod(string $method): self
    {
        $upper = strtoupper($method);
        if (!in_array($upper, self::VALID_REST_METHODS, true)) {
            throw new \InvalidArgumentException(
                sprintf('Invalid REST method "%s". Allowed: %s', $method, implode(', ', self::VALID_REST_METHODS))
            );
        }
        $this->restMethodValue = $upper;
        return $this;
    }

    public function register(): void
    {
        SchemaRegistry::getInstance()->registerQuery($this);
    }

    public function getName(): string
    {
        return $this->name;
    }

    /**
     * Export in canonical IntermediateSchema format consumed by `fraiseql compile`.
     *
     * Keys match the Rust IntermediateQuery struct exactly (snake_case).
     *
     * @return array<string, mixed>
     */
    public function toIntermediateArray(): array
    {
        $result = [
            'name'         => $this->name,
            'return_type'  => $this->returnTypeValue,
            'returns_list' => $this->returnsListValue,
            'nullable'     => $this->nullableValue,
            'arguments'    => $this->buildIntermediateArguments(),
        ];

        if ($this->sqlSourceValue !== null) {
            $result['sql_source'] = $this->sqlSourceValue;
        }

        if ($this->descriptionValue !== null) {
            $result['description'] = $this->descriptionValue;
        }

        if ($this->cacheTtlSecondsValue !== null) {
            $result['cache_ttl_seconds'] = $this->cacheTtlSecondsValue;
        }

        if (!empty($this->additionalViewsList)) {
            $result['additional_views'] = $this->additionalViewsList;
        }

        if (!empty($this->injectMap)) {
            $result['inject'] = $this->injectMap;
        }

        if ($this->requiresRoleValue !== null) {
            $result['requires_role'] = $this->requiresRoleValue;
        }

        if ($this->restPathValue !== null) {
            $rest = ['path' => $this->restPathValue, 'method' => $this->restMethodValue ?? 'GET'];
            $result['rest'] = $rest;
        }

        return $result;
    }

    /**
     * @return array<string, mixed>
     */
    public function toArray(): array
    {
        $result = [
            'name'         => $this->name,
            'returnType'   => $this->returnsListValue
                ? '[' . $this->returnTypeValue . ']'
                : $this->returnTypeValue,
            'returns_list' => $this->returnsListValue,
        ];

        if ($this->sqlSourceValue !== null) {
            $result['sql_source'] = $this->sqlSourceValue;
        }

        if ($this->descriptionValue !== null) {
            $result['description'] = $this->descriptionValue;
        }

        if (!empty($this->arguments)) {
            $result['arguments'] = $this->arguments;
        }

        $injectParams = $this->buildInjectParams();
        if (!empty($injectParams)) {
            $result['inject_params'] = $injectParams;
        }

        if ($this->cacheTtlSecondsValue !== null) {
            $result['cache_ttl_seconds'] = $this->cacheTtlSecondsValue;
        }

        if (!empty($this->additionalViewsList)) {
            $result['additional_views'] = $this->additionalViewsList;
        }

        if ($this->requiresRoleValue !== null) {
            $result['requires_role'] = $this->requiresRoleValue;
        }

        if ($this->deprecationReason !== null) {
            $result['deprecation'] = ['reason' => $this->deprecationReason];
        }

        if ($this->relayCursorTypeValue !== null) {
            $result['relay_cursor_type'] = $this->relayCursorTypeValue;
        }

        if ($this->autoParamsValue) {
            $result['auto_params'] = true;
        }

        if ($this->restPathValue !== null) {
            $rest = ['path' => $this->restPathValue, 'method' => $this->restMethodValue ?? 'GET'];
            $result['rest'] = $rest;
        }

        return $result;
    }

    /**
     * Parse inject map into structured inject_params array.
     *
     * @return array<string, array{source: string, claim: string}>
     */
    private function buildInjectParams(): array
    {
        $params = [];
        foreach ($this->injectMap as $param => $source) {
            if (str_starts_with($source, 'jwt:')) {
                $claim = substr($source, 4);
                $params[$param] = ['source' => 'jwt', 'claim' => $claim];
            }
        }
        return $params;
    }

    /**
     * Build arguments array in IntermediateSchema format (list of {name, type, nullable}).
     *
     * @return array<int, array{name: string, type: string, nullable: bool}>
     */
    private function buildIntermediateArguments(): array
    {
        $result = [];
        foreach ($this->arguments as $name => $arg) {
            $result[] = [
                'name'     => $name,
                'type'     => $arg['type'],
                'nullable' => $arg['nullable'],
            ];
        }
        return $result;
    }
}

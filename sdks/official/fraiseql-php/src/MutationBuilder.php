<?php

declare(strict_types=1);

namespace FraiseQL;

/**
 * Fluent builder for GraphQL mutation definitions.
 *
 * Usage:
 * ```php
 * StaticAPI::mutation('createOrder')
 *     ->returnType('Order')
 *     ->sqlSource('fn_create_order')
 *     ->operation('insert')
 *     ->invalidatesViews(['v_order_summary'])
 *     ->register();
 * ```
 */
final class MutationBuilder
{
    private string $returnTypeValue = '';
    private bool $returnsListValue = false;
    private bool $nullableValue = false;
    private ?string $requiresRoleValue = null;

    /** @var list<string> */
    private array $requiresActorList = [];
    private ?string $sqlSourceValue = null;
    private ?string $operationValue = null;
    private ?string $descriptionValue = null;

    /** @var array<string, array{type: string, nullable: bool, default: mixed}> */
    private array $arguments = [];

    /** @var array<string, string> */
    private array $injectMap = [];

    /** @var array<string> */
    private array $invalidatesViewsList = [];

    /** @var array<string> */
    private array $invalidatesFactTablesList = [];

    private bool $cascadeValue = false;

    private ?string $restPathValue = null;
    private ?string $restMethodValue = null;

    /** @var string[] */
    private const VALID_REST_METHODS = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE'];

    private function __construct(private readonly string $name)
    {
    }

    public static function mutation(string $name): self
    {
        return new self($name);
    }

    public function returnType(string $type): self
    {
        $this->returnTypeValue = $type;
        return $this;
    }

    /**
     * Whether this mutation returns a list of the return type.
     *
     * `IntermediateMutation` has always carried `returns_list` and `nullable`; the
     * builder simply had no way to set them, so every PHP-authored mutation compiled as
     * a non-null single value whatever the author intended.
     */
    public function returnsList(bool $isList = true): self
    {
        $this->returnsListValue = $isList;
        return $this;
    }

    /**
     * Whether this mutation's result may be null.
     */
    public function nullable(bool $nullable = true): self
    {
        $this->nullableValue = $nullable;
        return $this;
    }

    /**
     * Role required to execute this mutation and to see it in introspection.
     */
    public function requiresRole(string $role): self
    {
        $this->requiresRoleValue = $role;
        return $this;
    }

    /**
     * Restrict this mutation to an allow-list of actor types (#966).
     *
     * Enforced in the same executor gate as {@see requiresRole()}, on every transport.
     * Until #1123 it was expressible only by hand-writing `schema.json`.
     *
     * @param list<string> $actors one or more {@see ActorType} constants
     */
    public function requiresActor(array $actors): self
    {
        ActorType::validate($actors, "mutation '{$this->name}'");
        $this->requiresActorList = $actors;
        return $this;
    }

    public function sqlSource(string $source): self
    {
        $this->sqlSourceValue = $source;
        return $this;
    }

    public function operation(string $op): self
    {
        $this->operationValue = $op;
        return $this;
    }

    public function description(string $desc): self
    {
        $this->descriptionValue = $desc;
        return $this;
    }

    public function argument(string $argName, string $type, bool $nullable = true, mixed $default = null): self
    {
        $this->arguments[$argName] = ['type' => $type, 'nullable' => $nullable, 'default' => $default];
        return $this;
    }

    /**
     * Inject JWT claims as mutation parameters.
     *
     * @param array<string, string> $inject Map of param name to 'jwt:<claim>'
     */
    public function inject(array $inject): self
    {
        $this->injectMap = $inject;
        return $this;
    }

    /**
     * @param array<string> $views
     */
    public function invalidatesViews(array $views): self
    {
        $this->invalidatesViewsList = $views;
        return $this;
    }

    /**
     * @param array<string> $tables
     */
    public function invalidatesFactTables(array $tables): self
    {
        $this->invalidatesFactTablesList = $tables;
        return $this;
    }

    /**
     * Enable cascade support on this mutation.
     *
     * @param bool $cascade Whether this mutation uses cascade
     * @return self Fluent interface
     */
    public function cascade(bool $cascade = true): self
    {
        $this->cascadeValue = $cascade;
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
        SchemaRegistry::getInstance()->registerMutation($this);
    }

    public function getName(): string
    {
        return $this->name;
    }

    /**
     * Export in canonical IntermediateSchema format consumed by `fraiseql compile`.
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

        if ($this->operationValue !== null) {
            $result['operation'] = $this->operationValue;
        }

        if ($this->descriptionValue !== null) {
            $result['description'] = $this->descriptionValue;
        }

        // `invalidates_views` and `invalidates_fact_tables` are the keys the compiler
        // reads. This method wrote the first as `invalidates` and never wrote the second
        // at all, while the sibling `toArray()` below used both correctly — and this is
        // the method `SchemaExporter` calls, so the canonical path was the broken one.
        // Result: `invalidatesViews()` and `invalidatesFactTables()` were silent no-ops,
        // a compile that reported success, and cached reads that never saw a write (#852).
        if (!empty($this->invalidatesViewsList)) {
            $result['invalidates_views'] = $this->invalidatesViewsList;
        }

        if (!empty($this->invalidatesFactTablesList)) {
            $result['invalidates_fact_tables'] = $this->invalidatesFactTablesList;
        }

        // `inject_params`, not `inject`, and in the nested `{source, claim}` form —
        // the same drift, and now a hard compile error rather than a silent drop (#806).
        $injectParams = $this->buildInjectParams();
        if (!empty($injectParams)) {
            $result['inject_params'] = $injectParams;
        }

        if ($this->requiresRoleValue !== null) {
            $result['requires_role'] = $this->requiresRoleValue;
        }

        if ($this->requiresActorList !== []) {
            $result['requires_actor'] = $this->requiresActorList;
        }

        if ($this->cascadeValue) {
            $result['cascade'] = true;
        }

        if ($this->restPathValue !== null) {
            $rest = ['path' => $this->restPathValue, 'method' => $this->restMethodValue ?? 'POST'];
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
            'name'       => $this->name,
            'returnType' => $this->returnTypeValue,
        ];

        if ($this->sqlSourceValue !== null) {
            $result['sql_source'] = $this->sqlSourceValue;
        }

        if ($this->operationValue !== null) {
            $result['operation'] = $this->operationValue;
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

        if (!empty($this->invalidatesViewsList)) {
            $result['invalidates_views'] = $this->invalidatesViewsList;
        }

        if (!empty($this->invalidatesFactTablesList)) {
            $result['invalidates_fact_tables'] = $this->invalidatesFactTablesList;
        }

        if ($this->cascadeValue) {
            $result['cascade'] = true;
        }

        if ($this->restPathValue !== null) {
            $rest = ['path' => $this->restPathValue, 'method' => $this->restMethodValue ?? 'POST'];
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

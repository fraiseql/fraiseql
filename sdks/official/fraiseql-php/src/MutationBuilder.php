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
            'name'        => $this->name,
            'return_type' => $this->returnTypeValue,
            'arguments'   => $this->buildIntermediateArguments(),
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

        if (!empty($this->invalidatesViewsList)) {
            $result['invalidates'] = $this->invalidatesViewsList;
        }

        if (!empty($this->injectMap)) {
            $result['inject'] = $this->injectMap;
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

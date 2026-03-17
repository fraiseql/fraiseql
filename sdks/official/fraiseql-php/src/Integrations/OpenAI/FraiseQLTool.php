<?php

declare(strict_types=1);

namespace FraiseQL\Integrations\OpenAI;

use FraiseQL\FraiseQLClient;

final class FraiseQLTool
{
    /** @param array<string, mixed> $parametersSchema */
    public function __construct(
        private readonly FraiseQLClient $client,
        private readonly string $name,
        private readonly string $description,
        private readonly string $query,
        private readonly array $parametersSchema,
    ) {}

    /** @return array<string, mixed> */
    public function toDefinition(): array
    {
        return [
            'type' => 'function',
            'function' => [
                'name' => $this->name,
                'description' => $this->description,
                'parameters' => $this->parametersSchema,
            ],
        ];
    }

    /**
     * @param array<string, mixed> $arguments
     * @return array<string, mixed>
     */
    public function execute(array $arguments): array
    {
        return $this->client->query($this->query, $arguments);
    }
}

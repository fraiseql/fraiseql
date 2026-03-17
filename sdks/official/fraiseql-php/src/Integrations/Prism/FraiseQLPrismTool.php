<?php

declare(strict_types=1);

namespace FraiseQL\Integrations\Prism;

use FraiseQL\FraiseQLClient;

final class FraiseQLPrismTool
{
    public function __construct(
        private readonly FraiseQLClient $client,
        private readonly string $name,
        private readonly string $description,
        private readonly string $query,
    ) {}

    public function getName(): string
    {
        return $this->name;
    }

    public function getDescription(): string
    {
        return $this->description;
    }

    /**
     * @param array<string, mixed> $args
     * @return string JSON result
     */
    public function handle(array $args = []): string
    {
        $result = $this->client->query($this->query, $args);
        return json_encode($result, JSON_THROW_ON_ERROR);
    }
}

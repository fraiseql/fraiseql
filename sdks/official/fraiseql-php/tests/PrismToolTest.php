<?php

declare(strict_types=1);

use PHPUnit\Framework\TestCase;
use FraiseQL\FraiseQLClient;
use FraiseQL\Integrations\Prism\FraiseQLPrismTool;

class PrismToolTest extends TestCase
{
    private function makeTool(): FraiseQLPrismTool
    {
        $client = new FraiseQLClient('http://localhost:9999');
        return new FraiseQLPrismTool(
            client: $client,
            name: 'search_products',
            description: 'Search products by keyword',
            query: 'query Search($q: String!) { products(search: $q) { id name } }',
        );
    }

    public function testGetName(): void
    {
        $tool = $this->makeTool();
        $this->assertSame('search_products', $tool->getName());
    }

    public function testGetDescription(): void
    {
        $tool = $this->makeTool();
        $this->assertSame('Search products by keyword', $tool->getDescription());
    }

    public function testInstantiates(): void
    {
        $tool = $this->makeTool();
        $this->assertInstanceOf(FraiseQLPrismTool::class, $tool);
    }
}

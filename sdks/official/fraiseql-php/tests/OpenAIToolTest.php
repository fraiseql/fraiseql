<?php

declare(strict_types=1);

use PHPUnit\Framework\TestCase;
use FraiseQL\FraiseQLClient;
use FraiseQL\Integrations\OpenAI\FraiseQLTool;

class OpenAIToolTest extends TestCase
{
    private function makeTool(): FraiseQLTool
    {
        $client = new FraiseQLClient('http://localhost:9999');
        return new FraiseQLTool(
            client: $client,
            name: 'get_user',
            description: 'Fetch a user by ID',
            query: 'query GetUser($id: ID!) { user(id: $id) { id name } }',
            parametersSchema: [
                'type' => 'object',
                'properties' => [
                    'id' => ['type' => 'string', 'description' => 'The user ID'],
                ],
                'required' => ['id'],
            ],
        );
    }

    public function testToDefinitionReturnsCorrectFormat(): void
    {
        $tool = $this->makeTool();
        $definition = $tool->toDefinition();

        $this->assertSame('function', $definition['type']);
        $this->assertArrayHasKey('function', $definition);

        $fn = $definition['function'];
        $this->assertSame('get_user', $fn['name']);
        $this->assertSame('Fetch a user by ID', $fn['description']);
        $this->assertArrayHasKey('parameters', $fn);
        $this->assertSame('object', $fn['parameters']['type']);
        $this->assertArrayHasKey('id', $fn['parameters']['properties']);
    }

    public function testToDefinitionStructure(): void
    {
        $tool = $this->makeTool();
        $definition = $tool->toDefinition();

        // Verify top-level keys match OpenAI tool format
        $this->assertSame(['type', 'function'], array_keys($definition));
        $this->assertSame(['name', 'description', 'parameters'], array_keys($definition['function']));
    }

    public function testToDefinitionParametersSchema(): void
    {
        $schema = [
            'type' => 'object',
            'properties' => ['limit' => ['type' => 'integer']],
            'required' => [],
        ];
        $client = new FraiseQLClient('http://localhost:9999');
        $tool = new FraiseQLTool($client, 'list_users', 'List users', 'query { users { id } }', $schema);
        $definition = $tool->toDefinition();

        $this->assertSame($schema, $definition['function']['parameters']);
    }
}

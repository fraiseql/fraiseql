"""
LLM Token Cost Test Suite for GraphQL Architecture Comparison

This test suite measures the token requirements for generating equivalent
applications across different GraphQL architectures.
"""

import asyncio
import json
import re
import sys
import time
from abc import ABC, abstractmethod
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from typing import Optional

import tiktoken

# Add parent directory to path for imports
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

try:
    from benchmarks.llm_config import (
        LLMClient,
        LLMFactory,
        LLMProvider,
        Settings,
        calculate_total_cost,
    )
except ImportError:
    # Fallback for when config is not available
    LLMProvider = None
    Settings = None
    LLMFactory = None
    LLMClient = None


class Architecture(Enum):
    FRAISEQL = "fraiseql"
    PRISMA_GRAPHQL = "prisma_graphql"
    HASURA = "hasura"
    POSTGRAPHILE = "postgraphile"
    STRAWBERRY = "strawberry"


@dataclass
class TokenMetrics:
    """Metrics for token usage in generated code"""

    total_tokens: int
    file_breakdown: dict[str, int]
    prompt_tokens: int
    completion_tokens: int
    functional_tokens: int  # Tokens for actual functionality
    boilerplate_tokens: int  # Tokens for boilerplate/setup
    comment_tokens: int  # Tokens in comments
    import_tokens: int  # Tokens in imports


@dataclass
class GenerationResult:
    """Result of generating code for a specific architecture"""

    architecture: Architecture
    files: dict[str, str]  # filename -> content
    metrics: TokenMetrics
    is_functional: bool  # Whether the generated code would work
    missing_features: list[str]  # Features not implemented
    generation_time: float  # Time taken to generate


@dataclass
class TestScenario:
    """A test scenario for code generation"""

    name: str
    description: str
    requirements: list[str]
    entities: list[str]
    relationships: list[tuple[str, str, str]]  # (from, to, type)
    business_rules: list[str]
    expected_features: list[str]


class TokenCounter:
    """Utility for counting tokens in code"""

    def __init__(self, model: str = "gpt-4"):
        self.encoder = tiktoken.encoding_for_model(model)

    def count_tokens(self, text: str) -> int:
        """Count total tokens in text"""
        return len(self.encoder.encode(text))

    def count_code_tokens(self, code: str, language: str = "python") -> TokenMetrics:
        """Count tokens with breakdown by type"""
        total = self.count_tokens(code)

        # Extract different parts of code
        imports = self._extract_imports(code, language)
        comments = self._extract_comments(code, language)

        import_tokens = self.count_tokens(imports)
        comment_tokens = self.count_tokens(comments)

        # Remove imports and comments to get functional code
        functional_code = code.replace(imports, "").replace(comments, "")
        functional_tokens = self.count_tokens(functional_code)

        # Estimate boilerplate (rough heuristic)
        boilerplate_tokens = self._estimate_boilerplate(functional_code, language)

        return TokenMetrics(
            total_tokens=total,
            file_breakdown={},
            prompt_tokens=0,
            completion_tokens=total,
            functional_tokens=functional_tokens - boilerplate_tokens,
            boilerplate_tokens=boilerplate_tokens,
            comment_tokens=comment_tokens,
            import_tokens=import_tokens,
        )

    def _extract_imports(self, code: str, language: str) -> str:
        """Extract import statements"""
        if language == "python":
            lines = code.split("\n")
            import_lines = [l for l in lines if l.strip().startswith(("import ", "from "))]
            return "\n".join(import_lines)
        elif language == "typescript":
            import_pattern = r"^import\s+.*?;$"
            imports = re.findall(import_pattern, code, re.MULTILINE)
            return "\n".join(imports)
        return ""

    def _extract_comments(self, code: str, language: str) -> str:
        """Extract comments from code"""
        if language == "python":
            # Simple regex for Python comments
            comment_pattern = r'#.*$|"""[\s\S]*?"""|\'\'\'[\s\S]*?\'\'\''
            comments = re.findall(comment_pattern, code, re.MULTILINE)
            return "\n".join(comments)
        elif language == "typescript":
            # Simple regex for TS/JS comments
            comment_pattern = r"//.*$|/\*[\s\S]*?\*/"
            comments = re.findall(comment_pattern, code, re.MULTILINE)
            return "\n".join(comments)
        return ""

    def _estimate_boilerplate(self, code: str, language: str) -> int:
        """Estimate boilerplate tokens"""
        boilerplate_patterns = {
            "python": [
                r"def __init__\(self.*?\):",
                r"class Meta:",
                r"@property",
                r"@staticmethod",
                r'if __name__ == "__main__":',
            ],
            "typescript": [
                r"constructor\(",
                r"export default",
                r"interface Props",
                r"extends Component",
            ],
        }

        patterns = boilerplate_patterns.get(language, [])
        boilerplate_text = ""

        for pattern in patterns:
            matches = re.findall(pattern, code, re.MULTILINE)
            boilerplate_text += " ".join(matches)

        return self.count_tokens(boilerplate_text)


class ArchitectureGenerator(ABC):
    """Base class for architecture-specific code generators"""

    def __init__(self, token_counter: TokenCounter, llm_client: Optional[LLMClient] = None):
        self.token_counter = token_counter
        self.llm_client = llm_client

    @abstractmethod
    async def generate(self, scenario: TestScenario) -> GenerationResult:
        """Generate code for the given scenario"""
        pass

    @abstractmethod
    def create_prompt(self, scenario: TestScenario) -> str:
        """Create the prompt for LLM generation"""
        pass

    def validate_functionality(
        self, files: dict[str, str], scenario: TestScenario
    ) -> tuple[bool, list[str]]:
        """Validate if generated code implements all required features"""
        missing_features = []

        # Check if all entities are defined
        all_content = "\n".join(files.values())
        for entity in scenario.entities:
            if entity.lower() not in all_content.lower():
                missing_features.append(f"Entity {entity} not found")

        # Check relationships
        for from_entity, to_entity, rel_type in scenario.relationships:
            # Simple heuristic - look for relationship definitions
            if not self._check_relationship(all_content, from_entity, to_entity, rel_type):
                missing_features.append(f"Relationship {from_entity}->{to_entity} not found")

        # Check business rules
        for rule in scenario.business_rules:
            # This would need more sophisticated checking in practice
            keywords = rule.lower().split()
            if not all(keyword in all_content.lower() for keyword in keywords[:3]):
                missing_features.append(f"Business rule not implemented: {rule}")

        is_functional = len(missing_features) == 0
        return is_functional, missing_features

    def _check_relationship(
        self, content: str, from_entity: str, to_entity: str, rel_type: str
    ) -> bool:
        """Check if a relationship is defined in the code"""
        # Simple heuristic - can be made more sophisticated
        patterns = [
            f"{from_entity}.*{to_entity}",
            f"{to_entity}.*{from_entity}",
            f"List\\[.*{to_entity}.*\\]",
            f"Array<.*{to_entity}.*>",
        ]

        content_lower = content.lower()
        return any(re.search(pattern.lower(), content_lower) for pattern in patterns)


class FraiseQLGenerator(ArchitectureGenerator):
    """Generator for FraiseQL architecture"""

    def create_prompt(self, scenario: TestScenario) -> str:
        entities_str = ", ".join(scenario.entities)
        relationships_str = "\n".join(
            [f"- {f} has {t} ({r})" for f, t, r in scenario.relationships]
        )
        rules_str = "\n".join([f"- {rule}" for rule in scenario.business_rules])

        return f"""Create a FraiseQL GraphQL API with the following requirements:

Entities: {entities_str}

Relationships:
{relationships_str}

Business Rules:
{rules_str}

Use @fraise_type decorators and fraise_field() for all field definitions.
Include appropriate type hints and field purposes."""

    async def generate(self, scenario: TestScenario) -> GenerationResult:
        """Generate FraiseQL code"""
        prompt = self.create_prompt(scenario)
        prompt_tokens = self.token_counter.count_tokens(prompt)

        start_time = time.time()

        if self.llm_client:
            # Use actual LLM
            response = await self.llm_client.generate(prompt)
            code = response["content"]
            completion_tokens = response["tokens"]["completion"]
        else:
            # Use mock generation
            code = self._generate_fraiseql_code(scenario)
            completion_tokens = self.token_counter.count_tokens(code)

        generation_time = time.time() - start_time

        files = {"models.py": code}

        # Calculate metrics
        metrics = self.token_counter.count_code_tokens(code, "python")
        metrics.prompt_tokens = prompt_tokens
        metrics.completion_tokens = completion_tokens
        metrics.total_tokens = prompt_tokens + completion_tokens
        metrics.file_breakdown = {
            filename: self.token_counter.count_tokens(content)
            for filename, content in files.items()
        }

        # Validate
        is_functional, missing_features = self.validate_functionality(files, scenario)

        return GenerationResult(
            architecture=Architecture.FRAISEQL,
            files=files,
            metrics=metrics,
            is_functional=is_functional,
            missing_features=missing_features,
            generation_time=generation_time,
        )

    def _generate_fraiseql_code(self, scenario: TestScenario) -> str:
        """Generate FraiseQL code for the scenario"""
        # This is a simplified example - real implementation would use LLM
        code_parts = [
            "from fraiseql import fraise_type, fraise_field, fraise_input",
            "from datetime import datetime",
            "from typing import List, Optional",
            "",
        ]

        # Generate entity classes
        for entity in scenario.entities:
            code_parts.extend(
                [
                    "@fraise_type",
                    f"class {entity}:",
                    "    id: int",
                    f"    name: str = fraise_field(purpose='{entity} name')",
                    "    created_at: datetime",
                    "",
                ]
            )

        return "\n".join(code_parts)


class PrismaGraphQLGenerator(ArchitectureGenerator):
    """Generator for Prisma + GraphQL architecture"""

    def create_prompt(self, scenario: TestScenario) -> str:
        entities_str = ", ".join(scenario.entities)
        relationships_str = "\n".join(
            [f"- {f} has {t} ({r})" for f, t, r in scenario.relationships]
        )
        rules_str = "\n".join([f"- {rule}" for rule in scenario.business_rules])

        return f"""Create a GraphQL API using Prisma with the following requirements:

Entities: {entities_str}

Relationships:
{relationships_str}

Business Rules:
{rules_str}

Generate:
1. Prisma schema file
2. GraphQL type definitions
3. Resolver implementations
4. Any necessary business logic"""

    async def generate(self, scenario: TestScenario) -> GenerationResult:
        """Generate Prisma + GraphQL code"""
        prompt = self.create_prompt(scenario)
        prompt_tokens = self.token_counter.count_tokens(prompt)

        # Generate multiple files
        files = {
            "schema.prisma": self._generate_prisma_schema(scenario),
            "schema.graphql": self._generate_graphql_schema(scenario),
            "resolvers.ts": self._generate_resolvers(scenario),
        }

        # Calculate metrics
        total_tokens = 0
        file_breakdown = {}

        for filename, content in files.items():
            tokens = self.token_counter.count_tokens(content)
            file_breakdown[filename] = tokens
            total_tokens += tokens

        metrics = TokenMetrics(
            total_tokens=total_tokens,
            file_breakdown=file_breakdown,
            prompt_tokens=prompt_tokens,
            completion_tokens=total_tokens,
            functional_tokens=int(total_tokens * 0.6),  # Estimate
            boilerplate_tokens=int(total_tokens * 0.4),  # Estimate
            comment_tokens=0,
            import_tokens=int(total_tokens * 0.1),
        )

        # Validate
        is_functional, missing_features = self.validate_functionality(files, scenario)

        return GenerationResult(
            architecture=Architecture.PRISMA_GRAPHQL,
            files=files,
            metrics=metrics,
            is_functional=is_functional,
            missing_features=missing_features,
            generation_time=3.5,  # Simulated - typically takes longer
        )

    def _generate_prisma_schema(self, scenario: TestScenario) -> str:
        """Generate Prisma schema"""
        schema_parts = []

        for entity in scenario.entities:
            schema_parts.extend(
                [
                    f"model {entity} {{",
                    "  id        Int      @id @default(autoincrement())",
                    "  name      String",
                    "  createdAt DateTime @default(now())",
                    "}",
                    "",
                ]
            )

        return "\n".join(schema_parts)

    def _generate_graphql_schema(self, scenario: TestScenario) -> str:
        """Generate GraphQL schema"""
        schema_parts = []

        for entity in scenario.entities:
            schema_parts.extend(
                [
                    f"type {entity} {{",
                    "  id: ID!",
                    "  name: String!",
                    "  createdAt: DateTime!",
                    "}",
                    "",
                ]
            )

        schema_parts.extend(
            [
                "type Query {",
                *[f"  {entity.lower()}s: [{entity}!]!" for entity in scenario.entities],
                *[f"  {entity.lower()}(id: ID!): {entity}" for entity in scenario.entities],
                "}",
                "",
                "type Mutation {",
                *[
                    f"  create{entity}(input: Create{entity}Input!): {entity}!"
                    for entity in scenario.entities
                ],
                "}",
            ]
        )

        return "\n".join(schema_parts)

    def _generate_resolvers(self, scenario: TestScenario) -> str:
        """Generate resolver implementations"""
        resolver_parts = [
            "import { PrismaClient } from '@prisma/client'",
            "",
            "const prisma = new PrismaClient()",
            "",
            "export const resolvers = {",
            "  Query: {",
        ]

        for entity in scenario.entities:
            entity_lower = entity.lower()
            resolver_parts.extend(
                [
                    f"    {entity_lower}s: async () => {{",
                    f"      return prisma.{entity_lower}.findMany()",
                    "    },",
                    f"    {entity_lower}: async (_, {{ id }}) => {{",
                    f"      return prisma.{entity_lower}.findUnique({{ where: {{ id: parseInt(id) }} }})",
                    "    },",
                ]
            )

        resolver_parts.extend(
            ["  },", "  Mutation: {", "    // Mutation resolvers here", "  }", "}"]
        )

        return "\n".join(resolver_parts)


class TestSuite:
    """Main test suite for comparing architectures"""

    def __init__(self, use_llm: bool = False, provider: Optional[LLMProvider] = None):
        self.token_counter = TokenCounter()
        self.use_llm = use_llm
        self.settings = Settings() if Settings else None

        # Create LLM client if requested
        llm_client = None
        if use_llm and LLMFactory and provider:
            try:
                llm_client = LLMFactory.create_client(provider, self.settings)
            except Exception as e:
                print(f"Warning: Could not create LLM client: {e}")
                print("Falling back to mock generation")

        self.generators = {
            Architecture.FRAISEQL: FraiseQLGenerator(self.token_counter, llm_client),
            Architecture.PRISMA_GRAPHQL: PrismaGraphQLGenerator(self.token_counter, llm_client),
            # Add other generators as needed
        }
        self.results: list[dict] = []

    async def run_scenario(self, scenario: TestScenario) -> dict[Architecture, GenerationResult]:
        """Run a test scenario across all architectures"""
        results = {}

        for architecture, generator in self.generators.items():
            try:
                result = await generator.generate(scenario)
                results[architecture] = result
            except Exception as e:
                print(f"Error generating {architecture.value}: {e}")

        return results

    async def run_all_scenarios(self, scenarios: list[TestScenario]) -> None:
        """Run all test scenarios and collect results"""
        for scenario in scenarios:
            print(f"\nRunning scenario: {scenario.name}")
            results = await self.run_scenario(scenario)

            # Analyze and store results
            analysis = self.analyze_results(scenario, results)
            self.results.append(analysis)

            # Print summary
            self.print_scenario_summary(scenario, results)

    def analyze_results(
        self, scenario: TestScenario, results: dict[Architecture, GenerationResult]
    ) -> dict:
        """Analyze results for a scenario"""
        analysis = {"scenario": scenario.name, "architectures": {}}

        for arch, result in results.items():
            analysis["architectures"][arch.value] = {
                "total_tokens": result.metrics.total_tokens,
                "functional_tokens": result.metrics.functional_tokens,
                "boilerplate_tokens": result.metrics.boilerplate_tokens,
                "file_count": len(result.files),
                "is_functional": result.is_functional,
                "missing_features": result.missing_features,
                "generation_time": result.generation_time,
            }

        # Calculate relative metrics
        if Architecture.FRAISEQL in results:
            fraiseql_tokens = results[Architecture.FRAISEQL].metrics.total_tokens
            for arch, result in results.items():
                analysis["architectures"][arch.value]["token_ratio"] = (
                    result.metrics.total_tokens / fraiseql_tokens if fraiseql_tokens > 0 else 0
                )

        return analysis

    def print_scenario_summary(
        self, scenario: TestScenario, results: dict[Architecture, GenerationResult]
    ):
        """Print a summary of results for a scenario"""
        print(f"\nResults for: {scenario.name}")
        print("-" * 60)

        for arch, result in sorted(results.items(), key=lambda x: x[1].metrics.total_tokens):
            print(f"\n{arch.value}:")
            print(f"  Total tokens: {result.metrics.total_tokens}")
            print(f"  Functional tokens: {result.metrics.functional_tokens}")
            print(f"  Boilerplate tokens: {result.metrics.boilerplate_tokens}")
            print(f"  Files generated: {len(result.files)}")
            print(f"  Is functional: {result.is_functional}")
            if result.missing_features:
                print(f"  Missing features: {', '.join(result.missing_features)}")

    def generate_report(self, output_file: str = "llm_token_analysis_report.json"):
        """Generate a comprehensive report"""
        report = {
            "summary": self._generate_summary(),
            "scenarios": self.results,
            "recommendations": self._generate_recommendations(),
        }

        output_path = Path(output_file)
        with output_path.open("w") as f:
            json.dump(report, f, indent=2)

        print(f"\nReport saved to: {output_file}")

    def _generate_summary(self) -> dict:
        """Generate summary statistics"""
        if not self.results:
            return {}

        # Calculate average token savings
        total_savings = 0
        count = 0

        for result in self.results:
            archs = result["architectures"]
            if "fraiseql" in archs:
                fraiseql_tokens = archs["fraiseql"]["total_tokens"]
                for arch_name, arch_data in archs.items():
                    if arch_name != "fraiseql":
                        savings = 1 - (fraiseql_tokens / arch_data["total_tokens"])
                        total_savings += savings
                        count += 1

        avg_savings = (total_savings / count * 100) if count > 0 else 0

        return {
            "average_token_savings": f"{avg_savings:.1f}%",
            "scenarios_tested": len(self.results),
            "architectures_compared": len(self.generators),
        }

    def _generate_recommendations(self) -> list[str]:
        """Generate recommendations based on results"""
        recommendations = []

        # Analyze token efficiency
        if self.results:
            fraiseql_wins = sum(
                1
                for r in self.results
                if "fraiseql" in r["architectures"]
                and all(
                    r["architectures"]["fraiseql"]["total_tokens"] <= arch["total_tokens"]
                    for name, arch in r["architectures"].items()
                    if name != "fraiseql"
                )
            )

            if fraiseql_wins == len(self.results):
                recommendations.append(
                    "FraiseQL consistently requires fewer tokens across all scenarios"
                )

        return recommendations


# Example test scenarios
def create_test_scenarios() -> list[TestScenario]:
    """Create a set of test scenarios"""
    return [
        TestScenario(
            name="Simple Blog",
            description="Basic blog with users, posts, and comments",
            requirements=[
                "User authentication",
                "CRUD operations for all entities",
                "Comment moderation",
            ],
            entities=["User", "Post", "Comment"],
            relationships=[
                ("User", "Post", "one-to-many"),
                ("Post", "Comment", "one-to-many"),
                ("User", "Comment", "one-to-many"),
            ],
            business_rules=[
                "Users can only edit their own posts",
                "Comments require moderation before publishing",
                "Posts can be drafted or published",
            ],
            expected_features=[
                "User CRUD",
                "Post CRUD",
                "Comment CRUD",
                "Authentication",
                "Authorization",
                "Moderation",
            ],
        ),
        TestScenario(
            name="E-commerce Platform",
            description="Online store with products, orders, and inventory",
            requirements=[
                "Product catalog management",
                "Order processing",
                "Inventory tracking",
                "Customer management",
            ],
            entities=["Product", "Order", "Customer", "Inventory", "Category"],
            relationships=[
                ("Category", "Product", "one-to-many"),
                ("Customer", "Order", "one-to-many"),
                ("Order", "Product", "many-to-many"),
                ("Product", "Inventory", "one-to-one"),
            ],
            business_rules=[
                "Orders cannot exceed available inventory",
                "Prices must be positive",
                "Orders require customer information",
                "Products must belong to at least one category",
            ],
            expected_features=[
                "Product management",
                "Order processing",
                "Inventory tracking",
                "Price validation",
                "Category hierarchy",
            ],
        ),
    ]


async def main():
    """Run the test suite"""
    print("LLM Token Cost Analysis Test Suite")
    print("=" * 60)

    # Check if we should use actual LLM or mock
    use_llm = False
    provider = None

    if Settings and LLMProvider:
        settings = Settings()

        # Check for mock mode
        if hasattr(settings, "use_mock_mode") and settings.use_mock_mode:
            print("\nRunning in MOCK mode (no API calls)")
            provider = LLMProvider.MOCK
            use_llm = True
        else:
            # Check for available API keys
            if settings.openai_api_key:
                print("\nUsing OpenAI API")
                provider = LLMProvider.OPENAI
                use_llm = True
            elif settings.anthropic_api_key:
                print("\nUsing Anthropic API")
                provider = LLMProvider.ANTHROPIC
                use_llm = True
            else:
                print("\nNo API keys found - using mock generation")
    else:
        print("\nConfiguration not available - using mock generation")

    # Create test scenarios
    scenarios = create_test_scenarios()

    # Run test suite
    suite = TestSuite(use_llm=use_llm, provider=provider)
    await suite.run_all_scenarios(scenarios)

    # Generate report
    suite.generate_report()

    # Show cost summary if using real APIs
    if use_llm and provider not in [None, LLMProvider.MOCK]:
        costs = calculate_total_cost({"scenarios": suite.results}) if calculate_total_cost else {}
        if costs:
            print("\nCost Summary:")
            for arch, cost in costs.items():
                print(f"  {arch}: ${cost:.4f}")

    print("\nTest suite completed!")


if __name__ == "__main__":
    asyncio.run(main())

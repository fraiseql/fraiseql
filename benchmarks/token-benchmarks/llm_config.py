"""
Configuration for LLM providers and test settings
"""

from dataclasses import dataclass
from enum import Enum
from typing import Any, Optional, Protocol

from pydantic import BaseSettings, Field
from pydantic_settings import SettingsConfigDict


class LLMProvider(str, Enum):
    """Supported LLM providers"""

    OPENAI = "openai"
    ANTHROPIC = "anthropic"
    HUGGINGFACE = "huggingface"
    LOCAL = "local"
    MOCK = "mock"  # For testing without API calls


class Settings(BaseSettings):
    """Global settings for the test suite"""

    model_config = SettingsConfigDict(env_file="benchmarks/.env", env_file_encoding="utf-8")

    # OpenAI settings
    openai_api_key: Optional[str] = Field(None, env="OPENAI_API_KEY")
    openai_model: str = Field("gpt-4", env="OPENAI_MODEL")
    openai_temperature: float = Field(0.2, env="OPENAI_TEMPERATURE")

    # Anthropic settings
    anthropic_api_key: Optional[str] = Field(None, env="ANTHROPIC_API_KEY")
    anthropic_model: str = Field("claude-3-opus-20240229", env="ANTHROPIC_MODEL")
    anthropic_temperature: float = Field(0.2, env="ANTHROPIC_TEMPERATURE")

    # Hugging Face settings
    hf_model: str = Field("codellama/CodeLlama-7b-Python-hf", env="HF_MODEL")
    hf_device: str = Field("cpu", env="HF_DEVICE")

    # Test configuration
    max_tokens: int = Field(4000, env="MAX_TOKENS")
    timeout_seconds: int = Field(60, env="TIMEOUT_SECONDS")
    parallel_tests: bool = Field(False, env="PARALLEL_TESTS")
    retry_count: int = Field(3, env="RETRY_COUNT")

    # Output configuration
    save_generated_code: bool = Field(True, env="SAVE_GENERATED_CODE")
    generate_visualizations: bool = Field(True, env="GENERATE_VISUALIZATIONS")
    report_format: str = Field("json", env="REPORT_FORMAT")

    # Cost calculation (in USD per 1K tokens)
    token_costs: dict[str, dict[str, float]] = {
        "gpt-4": {"input": 0.03, "output": 0.06},
        "gpt-4-turbo": {"input": 0.01, "output": 0.03},
        "gpt-3.5-turbo": {"input": 0.0005, "output": 0.0015},
        "claude-3-opus-20240229": {"input": 0.015, "output": 0.075},
        "claude-3-sonnet-20240229": {"input": 0.003, "output": 0.015},
        "local": {"input": 0.0, "output": 0.0},
    }


@dataclass
class LLMConfig:
    """Configuration for a specific LLM provider"""

    provider: LLMProvider
    model: str
    api_key: Optional[str] = None
    temperature: float = 0.2
    max_tokens: int = 4000
    timeout: int = 60

    def get_cost_per_token(self, token_type: str = "output") -> float:
        """Get cost per token in USD"""
        settings = Settings()
        if self.model in settings.token_costs:
            return settings.token_costs[self.model][token_type] / 1000
        return 0.0


class LLMClient(Protocol):
    """Protocol for LLM client implementations"""

    async def generate(
        self, prompt: str, max_tokens: Optional[int] = None, temperature: Optional[float] = None
    ) -> dict[str, Any]:
        """Generate completion from prompt"""
        ...

    def count_tokens(self, text: str) -> int:
        """Count tokens in text"""
        ...


class OpenAIClient:
    """OpenAI API client"""

    def __init__(self, config: LLMConfig):
        self.config = config
        self._client = None

    async def _get_client(self):
        if self._client is None:
            try:
                import openai

                self._client = openai.AsyncOpenAI(api_key=self.config.api_key)
            except ImportError as e:
                raise ImportError("OpenAI package not installed. Run: pip install openai") from e
        return self._client

    async def generate(
        self, prompt: str, max_tokens: Optional[int] = None, temperature: Optional[float] = None
    ) -> dict[str, Any]:
        """Generate completion using OpenAI API"""
        client = await self._get_client()

        response = await client.chat.completions.create(
            model=self.config.model,
            messages=[{"role": "user", "content": prompt}],
            max_tokens=max_tokens or self.config.max_tokens,
            temperature=temperature or self.config.temperature,
        )

        return {
            "content": response.choices[0].message.content,
            "tokens": {
                "prompt": response.usage.prompt_tokens,
                "completion": response.usage.completion_tokens,
                "total": response.usage.total_tokens,
            },
            "model": response.model,
            "cost": self._calculate_cost(response.usage),
        }

    def count_tokens(self, text: str) -> int:
        """Count tokens using tiktoken"""
        import tiktoken

        try:
            enc = tiktoken.encoding_for_model(self.config.model)
        except KeyError:
            enc = tiktoken.get_encoding("cl100k_base")
        return len(enc.encode(text))

    def _calculate_cost(self, usage) -> float:
        """Calculate cost in USD"""
        input_cost = usage.prompt_tokens * self.config.get_cost_per_token("input")
        output_cost = usage.completion_tokens * self.config.get_cost_per_token("output")
        return round(input_cost + output_cost, 4)


class AnthropicClient:
    """Anthropic API client"""

    def __init__(self, config: LLMConfig):
        self.config = config
        self._client = None

    async def _get_client(self):
        if self._client is None:
            try:
                import anthropic

                self._client = anthropic.AsyncAnthropic(api_key=self.config.api_key)
            except ImportError as e:
                raise ImportError(
                    "Anthropic package not installed. Run: pip install anthropic"
                ) from e
        return self._client

    async def generate(
        self, prompt: str, max_tokens: Optional[int] = None, temperature: Optional[float] = None
    ) -> dict[str, Any]:
        """Generate completion using Anthropic API"""
        client = await self._get_client()

        response = await client.messages.create(
            model=self.config.model,
            messages=[{"role": "user", "content": prompt}],
            max_tokens=max_tokens or self.config.max_tokens,
            temperature=temperature or self.config.temperature,
        )

        # Extract text from content blocks
        content = ""
        for block in response.content:
            if hasattr(block, "text"):
                content += block.text

        return {
            "content": content,
            "tokens": {
                "prompt": response.usage.input_tokens,
                "completion": response.usage.output_tokens,
                "total": response.usage.input_tokens + response.usage.output_tokens,
            },
            "model": response.model,
            "cost": self._calculate_cost(response.usage),
        }

    def count_tokens(self, text: str) -> int:
        """Count tokens (approximate for Claude)"""
        # Rough approximation: 1 token ≈ 4 characters
        return len(text) // 4

    def _calculate_cost(self, usage) -> float:
        """Calculate cost in USD"""
        input_cost = usage.input_tokens * self.config.get_cost_per_token("input")
        output_cost = usage.output_tokens * self.config.get_cost_per_token("output")
        return round(input_cost + output_cost, 4)


class MockClient:
    """Mock client for testing without API calls"""

    def __init__(self, config: LLMConfig):
        self.config = config
        self.responses = self._get_mock_responses()

    def _get_mock_responses(self) -> dict[str, str]:
        """Get predefined mock responses"""
        return {
            "fraiseql": """from fraiseql import fraise_type, fraise_field
from datetime import datetime
from typing import List, Optional

@fraise_type
class User:
    id: int
    name: str = fraise_field(purpose="User's display name")
    email: str = fraise_field(purpose="User's email address")
    posts: List['Post'] = fraise_field(purpose="Posts authored by user")
    created_at: datetime

@fraise_type
class Post:
    id: int
    title: str = fraise_field(purpose="Post title")
    content: str = fraise_field(purpose="Post content")
    author: User = fraise_field(purpose="Post author")
    published_at: Optional[datetime]
""",
            "prisma": """model User {
  id        Int      @id @default(autoincrement())
  name      String
  email     String   @unique
  posts     Post[]
  createdAt DateTime @default(now())
}

model Post {
  id          Int       @id @default(autoincrement())
  title       String
  content     String
  authorId    Int
  author      User      @relation(fields: [authorId], references: [id])
  publishedAt DateTime?
}""",
        }

    async def generate(
        self, prompt: str, max_tokens: Optional[int] = None, temperature: Optional[float] = None
    ) -> dict[str, Any]:
        """Generate mock response"""
        # Determine which response to return based on prompt
        if "fraiseql" in prompt.lower():
            content = self.responses["fraiseql"]
        elif "prisma" in prompt.lower():
            content = self.responses["prisma"]
        else:
            content = "# Mock generated code"

        tokens = self.count_tokens(content)

        return {
            "content": content,
            "tokens": {
                "prompt": self.count_tokens(prompt),
                "completion": tokens,
                "total": self.count_tokens(prompt) + tokens,
            },
            "model": "mock",
            "cost": 0.0,
        }

    def count_tokens(self, text: str) -> int:
        """Count tokens using tiktoken"""
        import tiktoken

        enc = tiktoken.get_encoding("cl100k_base")
        return len(enc.encode(text))


class LLMFactory:
    """Factory for creating LLM clients"""

    @staticmethod
    def create_client(provider: LLMProvider, settings: Optional[Settings] = None) -> LLMClient:
        """Create an LLM client based on provider"""
        if settings is None:
            settings = Settings()

        if provider == LLMProvider.OPENAI:
            config = LLMConfig(
                provider=provider,
                model=settings.openai_model,
                api_key=settings.openai_api_key,
                temperature=settings.openai_temperature,
                max_tokens=settings.max_tokens,
                timeout=settings.timeout_seconds,
            )
            return OpenAIClient(config)

        elif provider == LLMProvider.ANTHROPIC:
            config = LLMConfig(
                provider=provider,
                model=settings.anthropic_model,
                api_key=settings.anthropic_api_key,
                temperature=settings.anthropic_temperature,
                max_tokens=settings.max_tokens,
                timeout=settings.timeout_seconds,
            )
            return AnthropicClient(config)

        elif provider == LLMProvider.MOCK:
            config = LLMConfig(provider=provider, model="mock", max_tokens=settings.max_tokens)
            return MockClient(config)

        else:
            raise ValueError(f"Unsupported provider: {provider}")


# Cost calculation utilities
def calculate_total_cost(results: dict[str, Any]) -> dict[str, float]:
    """Calculate total costs from test results"""
    costs = {}

    for scenario in results.get("scenarios", []):
        for arch_name, arch_data in scenario.get("architectures", {}).items():
            if arch_name not in costs:
                costs[arch_name] = 0.0

            # Add cost if available
            if "cost" in arch_data:
                costs[arch_name] += arch_data["cost"]

    return costs


def estimate_monthly_cost(
    daily_generations: int, average_tokens: int, model: str, settings: Optional[Settings] = None
) -> float:
    """Estimate monthly cost based on usage"""
    if settings is None:
        settings = Settings()

    if model in settings.token_costs:
        cost_per_token = settings.token_costs[model]["output"] / 1000
        daily_cost = daily_generations * average_tokens * cost_per_token
        return round(daily_cost * 30, 2)

    return 0.0

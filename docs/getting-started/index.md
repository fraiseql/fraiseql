# Getting Started with FraiseQL

Welcome to FraiseQL! This guide will help you get up and running with your first GraphQL API in minutes.

## Prerequisites

Before you begin, make sure you have:

- Python 3.13 or higher
- PostgreSQL 14 or higher
- Basic knowledge of Python and GraphQL

## Quick Overview

FraiseQL takes a unique approach to GraphQL APIs:

1. **Database Views**: Each entity has a PostgreSQL view that returns JSON data
2. **Single View Per Resolver**: Each GraphQL resolver fetches from exactly one view
3. **View Composition**: Nested objects are handled by composing views at the database level
4. **Efficient Queries**: The query builder selects only the requested fields from the JSON

## The FraiseQL Philosophy

Traditional GraphQL implementations often struggle with the N+1 query problem, requiring complex dataloaders or query optimization. FraiseQL solves this differently:

- **Push complexity to PostgreSQL**: Let the database handle relationships through view composition
- **One view, one resolver**: Each resolver simply fetches from its corresponding view
- **JSON-based**: Views return JSON data, making field selection trivial

## What You'll Learn

In this section, you'll learn how to:

- [Install FraiseQL](./installation.md) and set up your environment
- [Create your first API](./quickstart.md) with basic types and queries
- [Explore your API](./graphql-playground.md) using the interactive GraphQL Playground
- [Build a complete API](./first-api.md) with views and relationships

## Next Steps

After completing this guide, explore:

- [Core Concepts](../core-concepts/index.md) - Understand FraiseQL's architecture
- [Tutorials](../tutorials/index.md) - Build real-world applications
- [API Reference](../api-reference/index.md) - Detailed API documentation

Let's get started! →

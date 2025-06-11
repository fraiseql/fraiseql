# Comparisons

Understanding how FraiseQL compares to other solutions helps you make the right choice for your project.

## Available Comparisons

### [FraiseQL vs Alternatives](./alternatives.md)

A comprehensive comparison of FraiseQL with:
- **Hasura** - Configuration-based GraphQL engine
- **PostGraphile** - Node.js PostgreSQL-to-GraphQL
- **Strawberry GraphQL** - Python GraphQL framework
- **Prisma** - Type-safe ORM with GraphQL
- **Graphene-Django** - Django GraphQL integration
- **Supabase** - PostgreSQL-based BaaS
- **PostgREST** - REST API from PostgreSQL

## Quick Decision Guide

### Choose FraiseQL if you want:
- 🐍 Python with type safety
- 🐘 PostgreSQL-centric architecture
- ⚡ Maximum performance (with TurboRouter)
- 🎯 Business logic in the database
- 🚀 Simple deployment

### Consider alternatives if you need:
- 📡 GraphQL subscriptions (Hasura, PostGraphile)
- 🌐 Multiple database support (Prisma, Hasura)
- 🔌 Rich plugin ecosystem (PostGraphile)
- 📦 Complete backend platform (Supabase)
- 🔄 Database portability (Prisma)

## Performance at a Glance

| Solution | Request Overhead | Scalability | Best For |
|----------|-----------------|-------------|-----------|
| FraiseQL + TurboRouter | 0.06ms | Excellent | High-performance APIs |
| Hasura | 0.5ms | Excellent | Real-time apps |
| PostGraphile | 0.7ms | Excellent | CRUD-heavy apps |
| Strawberry + ORM | 2-5ms | Good | Flexible APIs |
| Prisma | 3-6ms | Good | Multi-database apps |

## Architecture Philosophy

FraiseQL takes a unique approach:
- **Database as the source of truth** - Not just for data, but for business logic
- **Thin Python layer** - Python orchestrates, PostgreSQL executes
- **Type safety everywhere** - From Python types to SQL queries
- **Performance by default** - Production mode and TurboRouter eliminate overhead

This philosophy differs from traditional approaches that keep business logic in the application layer.

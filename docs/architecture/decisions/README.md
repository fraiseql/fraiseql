# Architecture Decision Records

This directory contains Architecture Decision Records (ADRs) that document significant architectural decisions made in the FraiseQL project.

## What is an ADR?

An Architecture Decision Record captures an important architectural decision made along with its context and consequences. ADRs help future developers understand why certain decisions were made.

## ADR Format

Each ADR follows this template:

- **Title**: ADR-NNN: Brief description
- **Status**: Proposed, Accepted, Deprecated, Superseded
- **Context**: What is the issue that we're seeing that is motivating this decision?
- **Decision**: What is the change that we're proposing and/or doing?
- **Consequences**: What becomes easier or more difficult to do because of this change?

## Current ADRs

1. [ADR-001: CQRS Storage Strategy](001-cqrs-storage-strategy.md) - How we balance flexibility with data integrity using CQRS
2. [ADR-002: GraphQL Type System Design](002-graphql-type-system.md) - How we generate GraphQL schemas from Python with SQL-first resolution
3. [ADR-003: Security and Validation Strategy](003-security-validation.md) - Our defense-in-depth approach to security

## Creating a New ADR

1. Copy the template from an existing ADR
2. Number it sequentially (ADR-004, ADR-005, etc.)
3. Fill in all sections
4. Set status to "Proposed"
5. Submit for review
6. Update status to "Accepted" once approved

## Updating an ADR

- If a decision is reversed, mark the ADR as "Deprecated" and create a new ADR
- If a decision is modified, mark the old ADR as "Superseded by ADR-NNN" and create a new one
- Minor clarifications can be made directly

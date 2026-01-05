---
title: Getting Started
description: Quick start guide and introduction to FraiseQL
tags:
  - getting-started
  - introduction
  - quickstart
  - tutorial
---

# Getting Started with FraiseQL

Welcome! This directory contains everything you need to go from zero to building your first FraiseQL application.

## Learning Path

Follow this recommended progression:

### 1. **[Quickstart (5 minutes)](quickstart.md)** 🚀

Get a working GraphQL API running immediately.

**You'll build**: A simple note-taking API with queries and mutations

**You'll learn**:
- Installing FraiseQL
- Creating database views
- Defining GraphQL types
- Writing queries and mutations

**Start here if**: You want to see FraiseQL in action right now

---

### 2. **[First Hour Guide (60 minutes)](first-hour.md)** 📚

Progressive tutorial building on the quickstart.

**You'll build**: Extended note-taking API with filtering, timestamps, and error handling

**You'll learn**:
- Adding fields and filtering
- Where input types and operators
- Mutation error handling patterns
- Production patterns (timestamps, triggers)

**Start here if**: You completed the quickstart and want to go deeper

---

### 3. **[Installation Guide](installation.md)** 🔧

Platform-specific installation instructions and troubleshooting.

**You'll learn**:
- Python environment setup
- PostgreSQL installation by OS
- Dependency management
- Common installation issues

**Start here if**: You're having installation problems

---

## Choose Your HTTP Server

**NEW in v2.0.0**: FraiseQL now supports multiple pluggable HTTP servers!

Before diving deep, decide which HTTP framework you want to use:

### 🚀 [HTTP Servers Guide](../http-servers/README.md) - **START HERE**

Choose between:
- **Axum (Rust)** - Maximum performance (7-10x faster)
- **Starlette (Python)** - Lightweight, Python-only
- **FastAPI (Legacy)** - Existing code support

**Quick links**:
- **[Which server should I use?](../http-servers/README.md#decision-matrix-which-server-should-you-use)** - Decision matrix
- **[Detailed comparison](../http-servers/COMPARISON.md)** - Feature matrix, performance data
- **[Axum getting started](../http-servers/axum/01-getting-started.md)** - For high performance
- **[Starlette getting started](../http-servers/starlette/01-getting-started.md)** - For Python teams

---

## After Getting Started

Once you've completed these guides, continue your learning journey:

### Understanding the Architecture
- **[Understanding FraiseQL](../guides/understanding-fraiseql.md)** - 10-minute architecture deep dive
- **[Core Concepts](../core/concepts-glossary.md)** - CQRS, JSONB views, Trinity identifiers

### Building Real Applications
- **[Blog API Tutorial](../tutorials/blog-api.md)** - Complete application example
- **[Beginner Learning Path](../tutorials/beginner-path.md)** - Structured skill progression

### When Things Go Wrong
- **[Troubleshooting Guide](../guides/troubleshooting.md)** - Common issues and solutions
- **[Troubleshooting Decision Tree](../guides/troubleshooting-decision-tree.md)** - Diagnostic flowchart

## Quick Reference

**Prerequisites**: Python 3.13+, PostgreSQL 13+

**Installation**: `pip install fraiseql`

**Documentation Hub**: [docs/README.md](../README.md)

**Need help?**: [GitHub Discussions](../discussions)

---

**Ready to start?** → [Open the Quickstart Guide](quickstart.md)

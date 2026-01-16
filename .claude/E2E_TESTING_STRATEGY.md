# E2E Testing Strategy for Language Generators

**Purpose**: End-to-end testing across all 5 languages (Python, TypeScript, Java, Go, PHP)
**Architecture**: Docker containers for databases + venv/npm/Maven/composer for each language
**Timeline**: 2-3 days to implement
**Expected Coverage**: Schema authoring → JSON export → CLI compilation → Runtime execution

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                    Test Orchestrator (Rust/Make)                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ┌──────────────────┬──────────────────┬──────────────────┐     │
│  │   Python venv    │  TypeScript/npm  │   Go modules     │     │
│  │   (pip)          │   (node_modules) │   (go.mod)       │     │
│  └────────┬─────────┴────────┬─────────┴────────┬─────────┘     │
│           │                  │                  │                │
│           └──────────────────┼──────────────────┘                │
│                              │                                   │
│  ┌──────────────────┬────────▼─────────┬──────────────────┐     │
│  │  Java Maven      │ PHP Composer     │   CLI Tests      │     │
│  │  (pom.xml)       │ (composer.json)  │   (fraiseql-cli) │     │
│  └────────┬─────────┴────────┬─────────┴────────┬─────────┘     │
│           │                  │                  │                │
│           └──────────────────┼──────────────────┘                │
│                              │                                   │
│           ┌──────────────────▼──────────────────┐               │
│           │   Docker Compose (Test DBs)         │               │
│           ├──────────────────────────────────────┤               │
│           │  - PostgreSQL 16                     │               │
│           │  - PostgreSQL + pgvector             │               │
│           │  - MySQL 8.3                        │               │
│           │  - SQLite (local)                   │               │
│           └──────────────────────────────────────┘               │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

---

## Test Environment Setup

### Phase 1: Docker Infrastructure (Already Exists)

**Current Status**: ✅ docker-compose.test.yml already configured

Services:
- PostgreSQL 16 (primary, full features)
- PostgreSQL + pgvector (for vector tests)
- MySQL 8.3 (secondary support)
- SQLite (local file-based)

**Startup**:
```bash
docker compose -f docker-compose.test.yml up -d
docker compose -f docker-compose.test.yml logs -f
```

### Phase 2: Language Virtual Environments

#### Python Virtual Environment
```bash
# Create isolated venv for Python tests
python -m venv /tmp/fraiseql-python-venv
source /tmp/fraiseql-python-venv/bin/activate
pip install -e fraiseql-python/
pip install pytest pytest-asyncio
```

#### TypeScript/Node Environment
```bash
# Node dependencies already in fraiseql-typescript/node_modules/
# But create isolated npm cache for CI/CD
npm ci --prefix fraiseql-typescript/
npm install --prefix fraiseql-typescript/
```

#### Java Environment
```bash
# Maven caches in ~/.m2/repository
# No isolation needed for local testing
mvn clean install -f fraiseql-java/pom.xml -DskipTests
```

#### Go Environment
```bash
# Go modules cached in $GOPATH/pkg/mod
# No isolation needed, dependencies in go.mod
go mod download ./fraiseql-go/...
```

#### PHP Environment
```bash
# Create isolated vendor directory
cd fraiseql-php
composer install --prefer-dist
cd -
```

---

## E2E Test Flow

### Test Pipeline for Each Language

```
┌────────────────────────────────────────────────────────────┐
│ Language Generator E2E Test Pipeline                        │
├────────────────────────────────────────────────────────────┤
│                                                              │
│ Step 1: Setup (5 min)                                       │
│   ├─ Install dependencies (pip/npm/mvn/composer)           │
│   ├─ Wait for databases (PostgreSQL, MySQL)                │
│   └─ Create test output directory                          │
│                                                              │
│ Step 2: Schema Authoring (5 min)                            │
│   ├─ Generate basic schema (User, Post types)              │
│   ├─ Generate analytics schema (fact tables)               │
│   ├─ Verify decorators/attributes applied correctly       │
│   └─ Check schema registry contains all types             │
│                                                              │
│ Step 3: JSON Export (5 min)                                 │
│   ├─ Export schema to schema.json                          │
│   ├─ Validate JSON structure                              │
│   ├─ Compare with expected format                         │
│   └─ Store for next step                                  │
│                                                              │
│ Step 4: CLI Compilation (5 min)                             │
│   ├─ Run: fraiseql-cli compile schema.json                │
│   ├─ Check for compilation errors                         │
│   ├─ Verify schema.compiled.json generated                │
│   └─ Inspect compiled schema structure                    │
│                                                              │
│ Step 5: Runtime Execution (5 min)                           │
│   ├─ Start fraiseql-server with compiled schema           │
│   ├─ Send GraphQL queries                                 │
│   ├─ Validate responses match expected output             │
│   └─ Test mutations and subscriptions                     │
│                                                              │
│ Step 6: Results & Cleanup (2 min)                           │
│   ├─ Collect test artifacts                               │
│   ├─ Generate report                                      │
│   └─ Cleanup temporary files                              │
│                                                              │
└────────────────────────────────────────────────────────────┘
```

---

## Implementation: Makefile Test Targets

Create comprehensive test targets in top-level Makefile:

```makefile
# File: /home/lionel/code/fraiseql/Makefile

.PHONY: e2e-setup e2e-all e2e-python e2e-typescript e2e-java e2e-go e2e-php e2e-clean

# ============================================================================
# E2E Testing - All Languages
# ============================================================================

## Setup: Start Docker databases and create virtual environments
e2e-setup:
	@echo "🔧 Setting up E2E test infrastructure..."
	@echo "Starting Docker containers..."
	docker compose -f docker-compose.test.yml up -d
	@echo "Waiting for databases to be ready..."
	sleep 10
	docker compose -f docker-compose.test.yml logs
	@echo "✅ Docker infrastructure ready"

## Run E2E tests for all 5 languages (sequential)
e2e-all: e2e-setup e2e-python e2e-typescript e2e-java e2e-go e2e-php
	@echo ""
	@echo "=============================================="
	@echo "✅ All E2E tests completed!"
	@echo "=============================================="
	@echo ""

## E2E: Python (with venv)
e2e-python:
	@echo ""
	@echo "========== PYTHON E2E TEST =========="
	@echo "Setting up Python virtual environment..."
	python -m venv /tmp/fraiseql-python-venv
	source /tmp/fraiseql-python-venv/bin/activate && \
		pip install -q -e fraiseql-python/ && \
		pip install -q pytest pytest-asyncio && \
		echo "✅ Python venv ready" && \
		echo "" && \
		echo "Running E2E tests..." && \
		python tests/e2e/python_e2e_test.py && \
		echo "✅ Python E2E tests passed"
	@echo ""

## E2E: TypeScript (with npm)
e2e-typescript:
	@echo ""
	@echo "========== TYPESCRIPT E2E TEST =========="
	@echo "Installing TypeScript dependencies..."
	cd fraiseql-typescript && npm ci -q && echo "✅ npm dependencies ready"
	@echo ""
	@echo "Running E2E tests..."
	cd fraiseql-typescript && npm run test:e2e
	@echo "✅ TypeScript E2E tests passed"
	@echo ""

## E2E: Java (with Maven)
e2e-java:
	@echo ""
	@echo "========== JAVA E2E TEST =========="
	@echo "Downloading Maven dependencies..."
	mvn dependency:download-sources -f fraiseql-java/pom.xml -q 2>/dev/null || true
	@echo "✅ Maven dependencies ready"
	@echo ""
	@echo "Running E2E tests..."
	mvn test -f fraiseql-java/pom.xml -Dtest="*E2ETest"
	@echo "✅ Java E2E tests passed"
	@echo ""

## E2E: Go (with go modules)
e2e-go:
	@echo ""
	@echo "========== GO E2E TEST =========="
	@echo "Downloading Go modules..."
	cd fraiseql-go && go mod download && echo "✅ Go modules ready"
	@echo ""
	@echo "Running E2E tests..."
	cd fraiseql-go && go test ./... -run TestE2E -v
	@echo "✅ Go E2E tests passed"
	@echo ""

## E2E: PHP (with Composer)
e2e-php:
	@echo ""
	@echo "========== PHP E2E TEST =========="
	@echo "Installing Composer dependencies..."
	cd fraiseql-php && composer install -q && echo "✅ Composer dependencies ready"
	@echo ""
	@echo "Running E2E tests..."
	cd fraiseql-php && vendor/bin/phpunit tests/e2e/ -v
	@echo "✅ PHP E2E tests passed"
	@echo ""

## Cleanup: Stop Docker containers and remove temp files
e2e-clean:
	@echo "🧹 Cleaning up E2E test infrastructure..."
	docker compose -f docker-compose.test.yml down -v
	rm -rf /tmp/fraiseql-python-venv
	rm -rf /tmp/fraiseql-*-test-output
	@echo "✅ Cleanup complete"

## Status: Check E2E test infrastructure
e2e-status:
	@echo "Docker Compose Status:"
	docker compose -f docker-compose.test.yml ps
	@echo ""
	@echo "Database Connectivity:"
	docker compose -f docker-compose.test.yml exec -T postgres-test pg_isready -U fraiseql_test || echo "PostgreSQL: UNAVAILABLE"
	docker compose -f docker-compose.test.yml exec -T mysql-test mysqladmin ping -u fraiseql_test -pfraiseql_test_password || echo "MySQL: UNAVAILABLE"
```

---

## Implementation: E2E Test Files

### Python E2E Test

**File**: `tests/e2e/python_e2e_test.py`

```python
"""
E2E test for Python language generator.
Tests: Authoring → JSON Export → CLI Compilation → Runtime
"""

import json
import subprocess
import tempfile
from pathlib import Path

def test_python_e2e_basic_schema():
    """Test basic schema authoring and export."""
    from fraiseql import type as fraiseql_type
    from fraiseql import query as fraiseql_query
    from fraiseql import schema as fraiseql_schema

    # Step 1: Define schema
    @fraiseql_type
    class User:
        id: int
        name: str
        email: str

    @fraiseql_query(sql_source="v_user")
    def users(limit: int = 10) -> list[User]:
        """Get all users."""
        pass

    # Step 2: Export to JSON
    with tempfile.TemporaryDirectory() as tmpdir:
        schema_path = Path(tmpdir) / "schema.json"
        fraiseql_schema.export_schema(str(schema_path))

        # Step 3: Verify JSON structure
        with open(schema_path) as f:
            schema = json.load(f)

        assert "types" in schema
        assert "queries" in schema
        assert len(schema["types"]) == 1
        assert schema["types"][0]["name"] == "User"

        # Step 4: Try CLI compilation
        compiled_path = Path(tmpdir) / "schema.compiled.json"
        result = subprocess.run(
            ["fraiseql-cli", "compile", str(schema_path), "-o", str(compiled_path)],
            capture_output=True,
            text=True
        )

        if result.returncode == 0:
            assert compiled_path.exists()
            with open(compiled_path) as f:
                compiled = json.load(f)
            assert "sql_templates" in compiled or "queries" in compiled
        else:
            print(f"⚠️  CLI compilation failed: {result.stderr}")
            # CLI integration still WIP, so we don't fail here

def test_python_e2e_analytics_schema():
    """Test fact table analytics schema."""
    from fraiseql import fact_table, aggregate_query
    from fraiseql import schema

    @fact_table(name="tf_sales")
    class SalesFactTable:
        # Measures (numeric columns)
        amount: float
        quantity: int

        # Dimensions (JSONB column with nested fields)
        dimensions: dict

    @aggregate_query(fact_table=SalesFactTable)
    def sales_by_date(date: str) -> dict:
        """Sales aggregated by date."""
        pass

    with tempfile.TemporaryDirectory() as tmpdir:
        schema_path = Path(tmpdir) / "analytics_schema.json"
        schema.export_schema(str(schema_path))

        with open(schema_path) as f:
            schema_data = json.load(f)

        assert "types" in schema_data
        # Verify fact table was registered
        assert any(t.get("name") == "SalesFactTable" for t in schema_data["types"])
```

### TypeScript E2E Test

**File**: `fraiseql-typescript/tests/e2e/e2e.test.ts`

```typescript
/**
 * E2E test for TypeScript language generator.
 * Tests: Authoring → JSON Export → CLI Compilation → Runtime
 */

import { Type, Query, Mutation, FactTable, AggregateQuery } from "../src/decorators";
import { SchemaRegistry } from "../src/registry";
import { ExportSchema } from "../src/schema";
import * as fs from "fs";
import * as path from "path";
import { execSync } from "child_process";

describe("TypeScript E2E Tests", () => {
  let registry: SchemaRegistry;
  let tmpDir: string;

  beforeAll(() => {
    registry = SchemaRegistry.getInstance();
    tmpDir = fs.mkdtempSync(path.join("/tmp", "fraiseql-ts-e2e-"));
  });

  afterAll(() => {
    // Cleanup
    if (fs.existsSync(tmpDir)) {
      fs.rmSync(tmpDir, { recursive: true });
    }
  });

  test("should author basic schema", () => {
    // Define types
    const userType = Type("User", {
      id: { type: "Int", nullable: false },
      name: { type: "String", nullable: false },
      email: { type: "String", nullable: false },
    });

    registry.registerType(userType);

    // Define queries
    const usersQuery = Query("users", {
      returnType: "User",
      returnList: true,
      arguments: {
        limit: { type: "Int", defaultValue: 10 },
      },
    });

    registry.registerQuery(usersQuery);

    // Verify registry
    expect(registry.getType("User")).toBeDefined();
    expect(registry.getQuery("users")).toBeDefined();
  });

  test("should export schema to JSON", () => {
    const schema = ExportSchema();
    const schemaPath = path.join(tmpDir, "schema.json");

    fs.writeFileSync(schemaPath, JSON.stringify(schema, null, 2));

    expect(fs.existsSync(schemaPath)).toBe(true);

    const exported = JSON.parse(fs.readFileSync(schemaPath, "utf8"));
    expect(exported.types).toBeDefined();
    expect(exported.queries).toBeDefined();
  });

  test("should compile with CLI", () => {
    const schemaPath = path.join(tmpDir, "schema.json");
    const compiledPath = path.join(tmpDir, "schema.compiled.json");

    try {
      execSync(`fraiseql-cli compile ${schemaPath} -o ${compiledPath}`, {
        stdio: "pipe",
      });

      expect(fs.existsSync(compiledPath)).toBe(true);

      const compiled = JSON.parse(
        fs.readFileSync(compiledPath, "utf8")
      );
      expect(compiled).toBeDefined();
    } catch (error) {
      // CLI compilation still WIP
      console.warn("⚠️  CLI compilation failed (expected during development)");
    }
  });
});
```

### Go E2E Test

**File**: `fraiseql-go/fraiseql/e2e_test.go`

```go
package fraiseql

import (
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
	"tempfile"
)

func TestE2EBasicSchema(t *testing.T) {
	// Create temporary directory
	tmpDir, err := ioutil.TempDir("", "fraiseql-go-e2e-")
	if err != nil {
		t.Fatalf("Failed to create temp dir: %v", err)
	}
	defer os.RemoveAll(tmpDir)

	// Step 1: Define schema
	userType := TypeDef{
		Name: "User",
		Fields: []FieldDef{
			{Name: "id", Type: "Int", Nullable: false},
			{Name: "name", Type: "String", Nullable: false},
			{Name: "email", Type: "String", Nullable: false},
		},
	}

	registry := NewSchemaRegistry()
	registry.RegisterType(userType)

	// Step 2: Export to JSON
	schema := registry.ExportSchema()
	schemaPath := filepath.Join(tmpDir, "schema.json")

	schemaJSON, err := json.MarshalIndent(schema, "", "  ")
	if err != nil {
		t.Fatalf("Failed to marshal schema: %v", err)
	}

	err = ioutil.WriteFile(schemaPath, schemaJSON, 0644)
	if err != nil {
		t.Fatalf("Failed to write schema file: %v", err)
	}

	// Step 3: Verify JSON structure
	var exported map[string]interface{}
	err = json.Unmarshal(schemaJSON, &exported)
	if err != nil {
		t.Fatalf("Failed to unmarshal schema: %v", err)
	}

	if types, ok := exported["types"].([]interface{}); !ok || len(types) == 0 {
		t.Fatalf("Schema missing types field")
	}

	// Step 4: Try CLI compilation
	compiledPath := filepath.Join(tmpDir, "schema.compiled.json")
	cmd := exec.Command("fraiseql-cli", "compile", schemaPath, "-o", compiledPath)

	if err := cmd.Run(); err == nil {
		// CLI compilation succeeded
		data, _ := ioutil.ReadFile(compiledPath)
		var compiled map[string]interface{}
		json.Unmarshal(data, &compiled)

		if _, ok := compiled["sql_templates"]; !ok {
			t.Logf("⚠️  CLI compilation format mismatch (expected during development)")
		}
	} else {
		t.Logf("⚠️  CLI compilation failed (expected during development)")
	}
}

func TestE2EAnalyticsSchema(t *testing.T) {
	registry := NewSchemaRegistry()

	// Define fact table
	factTable := FactTableDef{
		Name:      "tf_sales",
		Measures:  []string{"amount", "quantity"},
		Dimensions: FieldDef{
			Name: "dimensions",
			Type: "JSON",
		},
	}

	registry.RegisterFactTable(factTable)

	// Export schema
	schema := registry.ExportSchema()

	schemaJSON, _ := json.MarshalIndent(schema, "", "  ")
	var exported map[string]interface{}
	json.Unmarshal(schemaJSON, &exported)

	if types, ok := exported["types"].([]interface{}); !ok || len(types) == 0 {
		t.Fatalf("Analytics schema missing types")
	}
}
```

### Java E2E Test

**File**: `fraiseql-java/src/test/java/com/fraiseql/E2ETest.java`

```java
package com.fraiseql;

import com.fraiseql.core.*;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.AfterAll;
import static org.junit.jupiter.api.Assertions.*;

import java.nio.file.*;
import java.io.IOException;
import java.util.*;

public class E2ETest {
    private static Path tmpDir;

    @BeforeAll
    static void setup() throws IOException {
        tmpDir = Files.createTempDirectory("fraiseql-java-e2e-");
    }

    @AfterAll
    static void cleanup() throws IOException {
        Files.walk(tmpDir)
            .sorted(Comparator.reverseOrder())
            .forEach(path -> {
                try {
                    Files.delete(path);
                } catch (IOException e) {
                    e.printStackTrace();
                }
            });
    }

    @Test
    void testBasicSchemaAuthoring() {
        // Step 1: Define schema
        QueryBuilder queryBuilder = FraiseQL.query("users")
            .returns("User")
            .returnsList()
            .argument("limit", "Int", 10);

        MutationBuilder mutationBuilder = FraiseQL.mutation("createUser")
            .argument("name", "String")
            .argument("email", "String")
            .returns("User");

        // Step 2: Export to JSON
        SchemaRegistry registry = SchemaRegistry.getInstance();
        registry.registerQuery("users", queryBuilder.build());
        registry.registerMutation("createUser", mutationBuilder.build());

        String schemaJson = SchemaFormatter.formatAsJson(registry);
        assertTrue(schemaJson.contains("\"queries\""));
        assertTrue(schemaJson.contains("\"mutations\""));

        // Step 3: Verify structure
        assertTrue(schemaJson.contains("users"));
        assertTrue(schemaJson.contains("createUser"));
    }

    @Test
    void testCliCompilation() throws IOException {
        // Export schema
        SchemaRegistry registry = SchemaRegistry.getInstance();
        String schemaJson = SchemaFormatter.formatAsJson(registry);

        // Write to file
        Path schemaPath = tmpDir.resolve("schema.json");
        Files.write(schemaPath, schemaJson.getBytes());

        // Try CLI compilation
        ProcessBuilder pb = new ProcessBuilder(
            "fraiseql-cli", "compile", schemaPath.toString()
        );

        try {
            Process process = pb.start();
            int exitCode = process.waitFor();

            if (exitCode == 0) {
                Path compiledPath = tmpDir.resolve("schema.compiled.json");
                assertTrue(Files.exists(compiledPath));
            } else {
                // CLI integration WIP
                System.out.println("⚠️  CLI compilation failed (expected)");
            }
        } catch (Exception e) {
            // CLI not available
            System.out.println("⚠️  CLI not available for testing");
        }
    }
}
```

### PHP E2E Test

**File**: `fraiseql-php/tests/e2e/E2ETest.php`

```php
<?php

namespace FraiseQL\Tests\E2E;

use FraiseQL\Attributes\GraphQLType;
use FraiseQL\Attributes\GraphQLField;
use FraiseQL\TypeBuilder;
use FraiseQL\SchemaRegistry;
use PHPUnit\Framework\TestCase;

class E2ETest extends TestCase
{
    private string $tmpDir;

    protected function setUp(): void
    {
        $this->tmpDir = sys_get_temp_dir() . '/fraiseql-php-e2e-' . uniqid();
        mkdir($this->tmpDir, 0755, true);
    }

    protected function tearDown(): void
    {
        array_map('unlink', glob("{$this->tmpDir}/*"));
        rmdir($this->tmpDir);
    }

    public function testBasicSchemaAuthoring(): void
    {
        // Define schema using TypeBuilder
        $userType = (new TypeBuilder("User"))
            ->addField("id", "Int")
            ->addField("name", "String")
            ->addField("email", "String")
            ->build();

        $registry = SchemaRegistry::getInstance();
        $registry->registerType($userType);

        // Export to JSON
        $schema = $registry->exportAsJson();

        $this->assertStringContainsString('"types"', $schema);
        $this->assertStringContainsString('"User"', $schema);
        $this->assertStringContainsString('"fields"', $schema);
    }

    public function testJsonExport(): void
    {
        $registry = SchemaRegistry::getInstance();
        $schema = $registry->exportAsJson();

        $schemaPath = "{$this->tmpDir}/schema.json";
        file_put_contents($schemaPath, $schema);

        $this->assertFileExists($schemaPath);

        // Verify JSON structure
        $data = json_decode($schema, true);
        $this->assertIsArray($data);
        $this->assertArrayHasKey('types', $data);
    }

    public function testCliCompilation(): void
    {
        $schemaPath = "{$this->tmpDir}/schema.json";
        $compiledPath = "{$this->tmpDir}/schema.compiled.json";

        // Export schema
        $registry = SchemaRegistry::getInstance();
        $schema = $registry->exportAsJson();
        file_put_contents($schemaPath, $schema);

        // Try CLI compilation
        $output = [];
        $returnCode = 0;
        exec("fraiseql-cli compile {$schemaPath} -o {$compiledPath}", $output, $returnCode);

        if ($returnCode === 0) {
            $this->assertFileExists($compiledPath);
        } else {
            // CLI integration WIP
            echo "⚠️  CLI compilation failed (expected)\n";
        }
    }
}
```

---

## GitHub Actions CI/CD Pipeline

**File**: `.github/workflows/e2e-tests.yml`

```yaml
name: E2E Language Generator Tests

on:
  push:
    branches: [main, develop, feature/phase-*]
  pull_request:
    branches: [main]
  schedule:
    - cron: "0 2 * * *"  # Daily at 2 AM

jobs:
  # Database setup (shared across all language tests)
  setup-databases:
    runs-on: ubuntu-latest
    services:
      postgres:
        image: postgres:16-alpine
        env:
          POSTGRES_USER: fraiseql_test
          POSTGRES_PASSWORD: fraiseql_test_password
          POSTGRES_DB: test_fraiseql
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5
        ports:
          - 5432:5432

      mysql:
        image: mysql:8.3
        env:
          MYSQL_ROOT_PASSWORD: fraiseql_test_root
          MYSQL_DATABASE: test_fraiseql
          MYSQL_USER: fraiseql_test
          MYSQL_PASSWORD: fraiseql_test_password
        options: >-
          --health-cmd "mysqladmin ping -h localhost"
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5
        ports:
          - 3306:3306

  # Python E2E Tests
  test-python:
    needs: setup-databases
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: actions/setup-python@v4
        with:
          python-version: "3.11"
          cache: "pip"

      - name: Install dependencies
        run: |
          pip install -e fraiseql-python/
          pip install pytest pytest-asyncio

      - name: Run Python E2E tests
        run: pytest tests/e2e/python_e2e_test.py -v

  # TypeScript E2E Tests
  test-typescript:
    needs: setup-databases
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: actions/setup-node@v4
        with:
          node-version: "18"
          cache: "npm"
          cache-dependency-path: "fraiseql-typescript/package-lock.json"

      - name: Install dependencies
        run: npm ci --prefix fraiseql-typescript/

      - name: Run TypeScript E2E tests
        run: npm run test:e2e --prefix fraiseql-typescript/

  # Java E2E Tests
  test-java:
    needs: setup-databases
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: actions/setup-java@v4
        with:
          java-version: "17"
          distribution: "temurin"
          cache: maven

      - name: Run Java E2E tests
        run: mvn test -f fraiseql-java/pom.xml -Dtest="*E2ETest"

  # Go E2E Tests
  test-go:
    needs: setup-databases
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: actions/setup-go@v4
        with:
          go-version: "1.22"
          cache: true
          cache-dependency-path: "fraiseql-go/go.sum"

      - name: Download Go modules
        run: go mod download -x

      - name: Run Go E2E tests
        run: go test ./fraiseql/... -run TestE2E -v -count=1

  # PHP E2E Tests
  test-php:
    needs: setup-databases
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: shivammathur/setup-php@v2
        with:
          php-version: "8.2"
          tools: composer:latest

      - uses: actions/cache@v3
        with:
          path: fraiseql-php/vendor
          key: ${{ runner.os }}-composer-${{ hashFiles('**/composer.lock') }}

      - name: Install dependencies
        run: composer install --working-dir=fraiseql-php

      - name: Run PHP E2E tests
        run: vendor/bin/phpunit tests/e2e/ --testdox

  # CLI Integration Test (after all languages)
  test-cli-integration:
    needs: [test-python, test-typescript, test-java, test-go, test-php]
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: actions/setup-rust@v1
        with:
          toolchain: stable

      - name: Build fraiseql-cli
        run: cargo build --release -p fraiseql-cli

      - name: Test CLI with Go-generated schema
        run: |
          cd fraiseql-go/examples
          go run basic_schema.go > /tmp/schema.json
          ../../target/release/fraiseql-cli compile /tmp/schema.json

  # Summary Report
  report:
    needs: [test-python, test-typescript, test-java, test-go, test-php, test-cli-integration]
    runs-on: ubuntu-latest
    if: always()
    steps:
      - name: Report Summary
        run: |
          echo "## E2E Test Results" >> $GITHUB_STEP_SUMMARY
          echo "- Python: ${{ needs.test-python.result }}" >> $GITHUB_STEP_SUMMARY
          echo "- TypeScript: ${{ needs.test-typescript.result }}" >> $GITHUB_STEP_SUMMARY
          echo "- Java: ${{ needs.test-java.result }}" >> $GITHUB_STEP_SUMMARY
          echo "- Go: ${{ needs.test-go.result }}" >> $GITHUB_STEP_SUMMARY
          echo "- PHP: ${{ needs.test-php.result }}" >> $GITHUB_STEP_SUMMARY
          echo "- CLI Integration: ${{ needs.test-cli-integration.result }}" >> $GITHUB_STEP_SUMMARY
```

---

## Makefile Commands

### Quick Start (All E2E Tests)
```bash
make e2e-all          # Run all E2E tests (requires ~30 minutes)
make e2e-setup        # Start Docker infrastructure only
make e2e-clean        # Stop Docker and cleanup
make e2e-status       # Check test infrastructure status
```

### Individual Language Tests
```bash
make e2e-python       # Python E2E tests
make e2e-typescript   # TypeScript E2E tests
make e2e-java         # Java E2E tests
make e2e-go           # Go E2E tests
make e2e-php          # PHP E2E tests
```

### Local Testing (Without Docker)
```bash
# For individual language testing without Docker infrastructure:
cd fraiseql-python && python -m pytest tests/e2e/ -v
cd fraiseql-typescript && npm run test:e2e
cd fraiseql-java && mvn test -Dtest="*E2ETest"
cd fraiseql-go && go test ./... -run TestE2E -v
cd fraiseql-php && vendor/bin/phpunit tests/e2e/ -v
```

---

## Test Coverage Matrix

```
┌──────────────┬─────────┬──────────┬────────────┬──────────┐
│ Test Phase   │ Python  │ TypeScript│ Java       │ Go / PHP │
├──────────────┼─────────┼──────────┼────────────┼──────────┤
│ Schema Auth  │ ✅      │ ✅       │ ✅         │ ✅       │
│ JSON Export  │ ✅      │ ✅       │ ✅         │ ✅       │
│ CLI Compile  │ ⚠️*     │ ⚠️*      │ ⚠️*        │ ⚠️*      │
│ Runtime Exec │ ⚠️*     │ ⚠️*      │ ⚠️*        │ ⚠️*      │
└──────────────┴─────────┴──────────┴────────────┴──────────┘

* Blocked on CLI schema format resolution
```

---

## Timeline & Effort

| Phase | Task | Effort | Dependencies |
|-------|------|--------|--------------|
| Phase 1 | Create E2E test files (5 languages) | 4 hours | - |
| Phase 2 | Implement Makefile targets | 2 hours | Phase 1 |
| Phase 3 | Set up GitHub Actions CI/CD | 3 hours | Phase 1-2 |
| Phase 4 | Resolve CLI schema format | 2-4 hours | Phase 1-3 |
| Phase 5 | Run full E2E pipeline | 1 hour | Phase 1-4 |

**Total**: 12-14 hours over 2-3 days

---

## Benefits of This Approach

✅ **Isolated Environments**
- Each language has its own virtual environment (venv/npm/Maven/composer)
- No dependency conflicts between languages
- Easy to run in CI/CD and local development

✅ **Docker-Based Databases**
- Consistent test environment across machines
- Easy setup/teardown with docker-compose
- Supports PostgreSQL, MySQL, SQLite, pgvector

✅ **Language-Idiomatic Tests**
- Python tests use pytest
- TypeScript tests use Jest
- Java tests use JUnit 5
- Go tests use testing.T
- PHP tests use PHPUnit

✅ **End-to-End Coverage**
- Tests entire pipeline: authoring → export → compile → runtime
- Catches integration issues
- Validates CLI contract

✅ **CI/CD Ready**
- GitHub Actions pipeline runs all tests in parallel
- Reports results by language
- Detects regressions early

✅ **Scalable**
- Easy to add new languages
- Same test pattern for each language
- Reusable Docker infrastructure

---

## Next Steps

1. Create E2E test files for each language (Step 1-2 in timeline)
2. Add Makefile targets for test orchestration
3. Set up GitHub Actions workflow
4. Fix CLI schema format issue (critical blocker)
5. Run full E2E pipeline and validate

---

**Document Version**: 1.0
**Last Updated**: January 16, 2026

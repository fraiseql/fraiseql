# Contributing to FraiseQL Java

Thank you for your interest in contributing to FraiseQL Java! We welcome contributions of all kinds - code, documentation, bug reports, and feature requests.

## Table of Contents

1. [Code of Conduct](#code-of-conduct)
2. [Getting Started](#getting-started)
3. [Development Setup](#development-setup)
4. [Making Changes](#making-changes)
5. [Submitting Changes](#submitting-changes)
6. [Coding Standards](#coding-standards)
7. [Testing](#testing)
8. [Documentation](#documentation)

## Code of Conduct

This project is committed to providing a welcoming and inspiring community for all. Please be respectful and constructive in all interactions. Harassment or discrimination will not be tolerated.

## Getting Started

### Prerequisites

- Java 17 or higher
- Maven 3.8.1 or higher
- Git

### Initial Setup

```bash
# Clone the repository
git clone https://github.com/fraiseql/fraiseql.git
cd fraiseql/fraiseql-java

# Build the project
mvn clean compile

# Run tests
mvn test
```

## Development Setup

### IDE Setup

#### IntelliJ IDEA

1. Open IntelliJ IDEA
2. Select "Open" and navigate to the `fraiseql-java` directory
3. Maven dependencies will auto-download
4. Mark `src/main/java` as Sources Root
5. Mark `src/test/java` as Test Sources Root

#### Eclipse

1. Open Eclipse
2. File → Import → Existing Maven Projects
3. Select the `fraiseql-java` directory
4. Finish the import

#### VS Code

1. Install extensions:
   - Extension Pack for Java
   - Maven for Java
   - Test Runner for Java

2. Open the `fraiseql-java` folder

### Build and Run Commands

```bash
# Clean build
mvn clean

# Compile
mvn compile

# Run tests
mvn test

# Run specific test
mvn test -Dtest=Phase2CoreTest

# Run specific test method
mvn test -Dtest=Phase2CoreTest#testSchemaRegistration

# Build JAR
mvn package

# Install locally
mvn install

# Generate JavaDoc
mvn javadoc:javadoc

# View test coverage
mvn jacoco:report
```

## Making Changes

### Creating a Feature Branch

Always create a feature branch for your changes:

```bash
git checkout -b feature/your-feature-name
```

Use descriptive branch names:

- `feature/support-enum-types`
- `fix/invalid-type-validation`
- `docs/update-api-guide`
- `test/add-edge-case-tests`

### What to Modify

**Core Classes** (in `src/main/java/com/fraiseql/core/`):

- `GraphQLType.java` - Annotation attributes
- `GraphQLField.java` - Field annotation attributes
- `TypeConverter.java` - Type conversion logic
- `SchemaRegistry.java` - Schema storage and retrieval
- `FraiseQL.java` - Public API
- `SchemaValidator.java` - Validation rules
- `SchemaCache.java` - Caching strategy
- `PerformanceMonitor.java` - Metrics collection

**Tests** (in `src/test/java/com/fraiseql/core/`):

- Always add tests for new features
- Update existing tests when changing behavior
- Follow the existing test structure

**Documentation**:

- Update `INSTALL.md` for setup changes
- Update `API_GUIDE.md` for API changes
- Update `EXAMPLES.md` for new examples
- Update `CHANGELOG.md` for notable changes
- Add JavaDoc to new public methods

**Configuration**:

- `pom.xml` - For dependency or build changes

## Submitting Changes

### Commit Messages

Use clear, descriptive commit messages:

```bash
git commit -m "feat(validation): Add support for interface types

- Implement InterfaceInfo class
- Update validator to check interface fields
- Add 5 test cases for interface validation
- Update API_GUIDE.md with interface examples

Closes #123"
```

**Commit Message Format**:

```
<type>(<scope>): <subject>

<body>

<footer>
```

**Types**:

- `feat` - New feature
- `fix` - Bug fix
- `refactor` - Code refactoring without feature change
- `test` - Test addition or modification
- `docs` - Documentation changes
- `chore` - Build, dependencies, or tooling

**Scopes**:

- `annotations` - @GraphQLType, @GraphQLField
- `types` - Type system and conversion
- `registry` - SchemaRegistry
- `validation` - SchemaValidator
- `cache` - SchemaCache
- `monitoring` - PerformanceMonitor
- `api` - FraiseQL public API
- `formatting` - JSON schema formatting
- `test` - Test infrastructure

### Submitting a Pull Request

1. Push your branch to GitHub:

   ```bash
   git push -u origin feature/your-feature-name
   ```

2. Open a Pull Request with:
   - Clear title describing the change
   - Description of what changed and why
   - Link to any related issues
   - Screenshots for UI changes (if applicable)

3. PR Template:

   ```markdown
   ## Description
   Brief description of the change

   ## Type of Change
   - [ ] Bug fix
   - [ ] New feature
   - [ ] Breaking change
   - [ ] Documentation update

   ## Testing
   - [ ] Added unit tests
   - [ ] Added integration tests
   - [ ] All tests pass

   ## Checklist
   - [ ] Code follows style guidelines
   - [ ] JavaDoc added for public API
   - [ ] CHANGELOG.md updated
   - [ ] No new warnings

   Closes #<issue-number>
   ```

### Pull Request Review

- At least one maintainer approval required
- Tests must pass (GitHub Actions)
- No regressions in existing tests
- Code coverage maintained or improved

## Coding Standards

### Style Guide

Follow these Java conventions:

```java
// Class names: PascalCase
public class SchemaValidator { }

// Method names: camelCase
public void recordOperation(String name, long duration) { }

// Constant names: UPPER_CASE
private static final int MAX_CACHE_SIZE = 1000;

// Variable names: camelCase
String typeName = "User";

// Indentation: 4 spaces (no tabs)
if (condition) {
    doSomething();
}

// Line length: Keep under 120 characters
public String generateLongMethodNameThatExplainsWhatItDoes(
    String parameter1,
    String parameter2
) {
    return parameter1 + parameter2;
}
```

### Documentation Standards

All public classes and methods must have JavaDoc:

```java
/**
 * Brief one-line description.
 *
 * Longer description explaining the purpose, behavior, and any important notes.
 * Can span multiple lines and include examples.
 *
 * @param paramName description of the parameter
 * @return description of the return value
 * @throws ExceptionType explanation of when this exception is thrown
 *
 * @example
 * // Example usage
 * var result = method(param);
 */
public ReturnType method(String paramName) throws ExceptionType {
    // implementation
}
```

### Code Organization

```java
public class ClassName {
    // 1. Constants (public static final)
    private static final int CONSTANT = 100;

    // 2. Static variables (private static)
    private static volatile ClassName instance;

    // 3. Instance variables (private)
    private final String field1;
    private volatile int field2;

    // 4. Constructors
    private ClassName() { }

    public ClassName(String field1) {
        this.field1 = field1;
    }

    // 5. Public methods (alphabetical)
    public void methodA() { }
    public void methodB() { }

    // 6. Protected methods (alphabetical)
    protected void protectedMethod() { }

    // 7. Private methods (alphabetical)
    private void privateMethod() { }

    // 8. Inner classes/interfaces
    public static class InnerClass { }
}
```

### Thread Safety

- Use `ConcurrentHashMap` for shared mutable maps
- Use `volatile` for shared mutable fields
- Document thread-safety guarantees
- Use private final fields where possible

```java
// Thread-safe map
private final Map<String, Value> cache = new ConcurrentHashMap<>();

// Thread-safe counter
private volatile long count = 0;

// Immutable field
private final String name;
```

### Error Handling

```java
// Always provide context in error messages
if (type == null) {
    throw new IllegalArgumentException(
        "Type cannot be null. Ensure the class is annotated with @GraphQLType"
    );
}

// Use specific exceptions
if (fieldCount == 0) {
    throw new IllegalStateException(
        "Type '" + typeName + "' must have at least one field"
    );
}
```

## Testing

### Test Structure

```java
public class FeatureTest {
    @BeforeEach
    public void setUp() {
        // Reset shared state
        FraiseQL.clear();
        SchemaCache.getInstance().clear();
    }

    @Test
    public void testHappyPath() {
        // Arrange
        @GraphQLType
        class User { @GraphQLField public int id; }

        // Act
        FraiseQL.registerType(User.class);

        // Assert
        assertNotNull(SchemaRegistry.getInstance().getType("User"));
    }

    @Test
    public void testEdgeCase() {
        // Test boundary conditions
    }

    @Test
    public void testErrorHandling() {
        // Test exception cases
    }
}
```

### Test Coverage Requirements

- Minimum 80% code coverage for new code
- All public methods must have tests
- All error paths must be tested
- Include both happy path and error cases

### Running Tests

```bash
# All tests
mvn test

# Specific test class
mvn test -Dtest=Phase2CoreTest

# Specific test method
mvn test -Dtest=Phase2CoreTest#testSchemaRegistration

# With detailed output
mvn test -X

# Generate coverage report
mvn jacoco:report
# View at: target/site/jacoco/index.html
```

## Documentation

### Updating Documentation

When making changes, update:

1. **CHANGELOG.md** - Add entry under "Unreleased" section
2. **API_GUIDE.md** - Update API documentation
3. **EXAMPLES.md** - Add/update examples
4. **INSTALL.md** - Update setup if needed
5. **JavaDoc** - Update code documentation

### Documentation Format

- Use Markdown for `.md` files
- Keep lines under 100 characters
- Use code blocks for examples
- Include table of contents for long docs

## Release Process

Only maintainers can create releases:

```bash
# Update version in pom.xml
mvn versions:set -DnewVersion=2.0.1

# Run full test suite
mvn clean test

# Build release artifacts
mvn clean package

# Tag release
git tag -a v2.0.1 -m "Release version 2.0.1"

# Push to Maven Central
mvn deploy -P release

# Push tags to GitHub
git push origin v2.0.1
```

## Help and Questions

- **Issues**: <https://github.com/fraiseql/fraiseql/issues>
- **Discussions**: <https://github.com/fraiseql/fraiseql/discussions>
- **Email**: <team@fraiseql.com>
- **Discord**: <https://discord.gg/fraiseql>

## Recognition

Contributors will be recognized in:

- Release notes
- GitHub contributors page
- Project documentation

Thank you for contributing to FraiseQL Java!

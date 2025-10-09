# FraiseQL-RS: Rust PyO3 Module - TDD Implementation Plan

**Project**: Ultra-fast GraphQL JSON transformation in Rust
**Goal**: 10-50x performance improvement over Python
**Methodology**: Phased TDD (RED → GREEN → REFACTOR → QA)

---

## Executive Summary

Build a Rust PyO3 module (`fraiseql-rs`) that handles:
1. snake_case → camelCase conversion (SIMD optimized)
2. JSON parsing and transformation (zero-copy)
3. `__typename` injection
4. Nested array resolution (`list[CustomType]`)
5. Nested object resolution

Replace:
- CamelForge (PostgreSQL complexity)
- Python field resolution (slow)
- Manual nested array handling

Achieve:
- 1-2ms response times for complex queries with nested arrays
- 10-50x faster than current Python implementation
- Database-agnostic solution

---

## PHASES

### Phase 1: Project Setup & Basic Infrastructure (POC)
**Objective**: Create working Rust PyO3 module that Python can import

#### TDD Cycle 1.1: Module Creation
1. **RED**: Write Python test that imports `fraiseql_rs`
   - Test file: `tests/integration/rust/test_module_import.py`
   - Expected failure: `ModuleNotFoundError: No module named 'fraiseql_rs'`

2. **GREEN**: Create minimal Rust module
   - Files: `fraiseql_rs/Cargo.toml`, `fraiseql_rs/src/lib.rs`
   - Minimal PyO3 setup
   - Build with maturin

3. **REFACTOR**: Project structure
   - Proper directory layout
   - Build scripts
   - Development tooling

4. **QA**: Verify phase completion
   - [ ] Module imports successfully
   - [ ] Builds on Linux
   - [ ] Basic CI setup

#### TDD Cycle 1.2: Version & Metadata
1. **RED**: Test module has correct metadata
2. **GREEN**: Add `__version__`, `__author__` exports
3. **REFACTOR**: Clean metadata system
4. **QA**: Documentation generated

---

### Phase 2: Snake to CamelCase Conversion
**Objective**: Implement fast snake_case → camelCase transformation

#### TDD Cycle 2.1: Basic Conversion
1. **RED**: Write test for simple snake_case conversion
   - Test: `to_camel_case("user_name")` → `"userName"`
   - Expected failure: Function doesn't exist

2. **GREEN**: Implement basic conversion
   - Rust function: `to_camel_case(s: &str) -> String`
   - Handle underscore splitting
   - Capitalize after underscore

3. **REFACTOR**: Optimize implementation
   - Pre-allocate string capacity
   - Avoid unnecessary allocations
   - Add inline hints

4. **QA**: Verify performance
   - [ ] 10x faster than Python
   - [ ] Handles edge cases
   - [ ] Memory efficient

#### TDD Cycle 2.2: Batch Conversion
1. **RED**: Test batch key transformation
   - Test: Transform dict keys in bulk
   - Expected: Process all keys at once

2. **GREEN**: Implement batch API
   - Function: `transform_keys_camel_case(keys: Vec<String>)`

3. **REFACTOR**: SIMD optimization
   - Use `smartstring` or similar
   - Vectorize where possible

4. **QA**: Benchmark suite
   - [ ] Compare vs Python
   - [ ] Memory profiling
   - [ ] Edge case testing

---

### Phase 3: JSON Parsing & Object Transformation
**Objective**: Parse JSON and transform object keys

#### TDD Cycle 3.1: JSON Parsing
1. **RED**: Test JSON parsing
   - Test: `parse_json('{"user_name": "John"}')` → dict
   - Expected failure: Function doesn't exist

2. **GREEN**: Implement JSON parsing
   - Use `serde_json::Value`
   - Parse to Rust structures

3. **REFACTOR**: Zero-copy optimization
   - Use `&str` instead of `String` where possible
   - Minimize allocations

4. **QA**: Performance validation
   - [ ] Faster than Python json module
   - [ ] Handles large JSON
   - [ ] Error handling

#### TDD Cycle 3.2: Object Key Transformation
1. **RED**: Test transforming JSON object keys
   - Test: `{"user_name": "John"}` → `{"userName": "John"}`
   - Expected failure: Keys not transformed

2. **GREEN**: Implement key transformation
   - Walk JSON object
   - Transform each key

3. **REFACTOR**: Clean API
   - Single function call
   - Options struct for configuration

4. **QA**: Integration testing
   - [ ] Nested objects work
   - [ ] Arrays preserved
   - [ ] Primitives unchanged

---

### Phase 4: __typename Injection
**Objective**: Add GraphQL `__typename` field to objects

#### TDD Cycle 4.1: Basic Typename Injection
1. **RED**: Test __typename addition
   - Test: Add `__typename: "User"` to object
   - Expected failure: Field not added

2. **GREEN**: Implement typename injection
   - Function: `inject_typename(obj, type_name)`

3. **REFACTOR**: Schema-aware injection
   - Use schema registry
   - Type-safe API

4. **QA**: Verify correctness
   - [ ] Typename added correctly
   - [ ] Doesn't overwrite existing
   - [ ] Works with nested objects

---

### Phase 5: Nested Array Resolution
**Objective**: Handle `list[CustomType]` with proper transformation

#### TDD Cycle 5.1: Schema Registry
1. **RED**: Test schema registration
   - Test: Register `User` type with nested `posts: list[Post]`
   - Expected failure: No schema system

2. **GREEN**: Implement schema registry
   - Struct: `SchemaInfo` with nested type info
   - Registration API

3. **REFACTOR**: Type-safe schema system
   - Builder pattern
   - Validation

4. **QA**: Schema validation
   - [ ] Types register correctly
   - [ ] Nested relationships tracked
   - [ ] Thread-safe

#### TDD Cycle 5.2: Recursive Array Transformation
1. **RED**: Test nested array transformation
   - Test: User with posts array, each post transformed
   - Expected failure: Arrays not recursively processed

2. **GREEN**: Implement recursive transformation
   - Function: `transform_recursive(value, schema)`
   - Handle arrays of objects

3. **REFACTOR**: Performance optimization
   - Minimize recursion overhead
   - Parallel processing for large arrays

4. **QA**: Complex structures
   - [ ] Multi-level nesting works
   - [ ] Performance scales
   - [ ] Memory efficient

---

### Phase 6: Complete Integration & Benchmarking
**Objective**: Full FraiseQL integration with production-ready quality

#### TDD Cycle 6.1: Python Integration
1. **RED**: Test FraiseQL integration
   - Test: Use in actual FraiseQL query
   - Expected: Works end-to-end

2. **GREEN**: Integration layer
   - Python wrapper functions
   - Error handling

3. **REFACTOR**: Clean API
   - Pythonic interface
   - Good error messages

4. **QA**: Real-world testing
   - [ ] Works with FraiseQL benchmark
   - [ ] All tests pass
   - [ ] Performance meets goals

#### TDD Cycle 6.2: Error Handling
1. **RED**: Test error scenarios
   - Test: Invalid JSON, null values, etc.
   - Expected: Proper Python exceptions

2. **GREEN**: Comprehensive error handling
   - Rust error types
   - Convert to Python exceptions

3. **REFACTOR**: Error message quality
   - Helpful messages
   - Stack traces preserved

4. **QA**: Error coverage
   - [ ] All error paths tested
   - [ ] No panics
   - [ ] Graceful degradation

---

## Success Criteria

### Performance Targets
- [ ] Simple field transformation: < 0.1ms (100x faster than Python)
- [ ] Complex query with nested arrays: 1-2ms (10-20x faster)
- [ ] Memory usage: < 2x JSON string size
- [ ] Zero-copy where possible

### Quality Targets
- [ ] 95%+ test coverage (Rust)
- [ ] 100% integration tests passing (Python)
- [ ] No unsafe code (or justified & documented)
- [ ] Benchmarks vs Python baseline
- [ ] Documentation complete

### Production Targets
- [ ] PyPI wheels (Linux, macOS, Windows)
- [ ] CI/CD pipeline
- [ ] Semantic versioning
- [ ] Changelog maintained

---

## Technology Stack

### Rust Dependencies
```toml
[dependencies]
pyo3 = { version = "0.21", features = ["extension-module"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
```

### Build Tools
- `maturin` - Build PyO3 modules
- `cargo-nextest` - Fast test runner
- `criterion` - Benchmarking

### CI/CD
- GitHub Actions
- Cross-compilation for wheels
- Automated testing

---

## Project Structure

```
fraiseql/
├── fraiseql_rs/                  # Rust module
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs               # Main module
│   │   ├── camel_case.rs        # camelCase conversion
│   │   ├── transformer.rs       # JSON transformation
│   │   ├── schema.rs            # Schema registry
│   │   └── error.rs             # Error types
│   ├── benches/
│   │   └── benchmark.rs         # Criterion benchmarks
│   └── tests/
│       └── integration_test.rs  # Rust tests
├── src/fraiseql/
│   └── rust_transformer.py      # Python wrapper
└── tests/
    └── integration/rust/
        ├── test_module_import.py
        ├── test_camel_case.py
        ├── test_transformer.py
        └── test_nested_arrays.py
```

---

## Development Workflow

### Each TDD Cycle:
1. **RED**: Write failing test
   ```bash
   uv run pytest tests/integration/rust/test_xxx.py::test_feature -v
   # Expected: FAILED
   ```

2. **GREEN**: Minimal implementation
   ```bash
   cd fraiseql_rs && cargo test
   maturin develop
   uv run pytest tests/integration/rust/test_xxx.py::test_feature -v
   # Expected: PASSED
   ```

3. **REFACTOR**: Improve code quality
   ```bash
   cargo clippy -- -D warnings
   cargo fmt
   uv run pytest tests/integration/rust/
   ```

4. **QA**: Comprehensive validation
   ```bash
   uv run pytest tests/integration/rust/ --cov
   cargo bench
   ```

---

## Phases Timeline

- **Phase 1**: 2-4 hours (POC)
- **Phase 2**: 4-6 hours (camelCase)
- **Phase 3**: 4-6 hours (JSON transformation)
- **Phase 4**: 2-3 hours (__typename)
- **Phase 5**: 6-8 hours (nested arrays)
- **Phase 6**: 4-6 hours (production ready)

**Total**: 22-33 hours (3-5 days)

---

## Current Status

- [ ] Phase 1: Project Setup & Basic Infrastructure
- [ ] Phase 2: Snake to CamelCase Conversion
- [ ] Phase 3: JSON Parsing & Object Transformation
- [ ] Phase 4: __typename Injection
- [ ] Phase 5: Nested Array Resolution
- [ ] Phase 6: Complete Integration & Benchmarking

---

**Next Step**: Begin Phase 1, TDD Cycle 1.1 - Create first failing test for module import

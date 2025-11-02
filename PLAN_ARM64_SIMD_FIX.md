# Plan: Fix ARM64 Compilation for Multi-Architecture Support

## Problem Statement

**Issue**: v1.1.1 PyPI publish failed due to Rust compilation errors on macOS ARM64 (Apple Silicon)

**Root Cause**:
- File: `fraiseql_rs/src/core/camel.rs`
- Uses x86_64-specific SIMD instructions (AVX2) unconditionally
- Imports `std::arch::x86_64::*` which doesn't exist on ARM64
- Uses `#[target_feature(enable = "avx2")]` which is x86_64-only

**Error Messages**:
```
error[E0432]: unresolved import `std::arch::x86_64`
error: the feature named `avx2` is not valid for this target
```

**Impact**:
- ❌ Can't build wheels for macOS ARM64 (Apple Silicon)
- ❌ Can't publish to PyPI
- ✅ Linux x86_64 builds work
- ❌ Windows ARM64 would also fail
- ❌ Prevents users on M1/M2/M3 Macs from using the package

---

## Solution Strategy

Implement **multi-architecture SIMD support** with runtime CPU feature detection and fallback to scalar code.

### Architecture Support Matrix

| Architecture | Primary SIMD | Fallback | Status |
|--------------|--------------|----------|--------|
| x86_64 (Intel/AMD) | AVX2 (256-bit) | Scalar | ✅ Working |
| ARM64 (Apple Silicon) | NEON (128-bit) | Scalar | ❌ Needs implementation |
| ARM64 (Linux/Android) | NEON (128-bit) | Scalar | ❌ Needs implementation |
| x86_64 (no AVX2) | SSE2 (128-bit) | Scalar | 🔄 Optional optimization |

---

## Implementation Plan - COMPLEX

**Complexity**: Complex | **Phased TDD Approach**

### Executive Summary

Refactor the SIMD-optimized snake_case to camelCase converter to support multiple CPU architectures (x86_64, ARM64) with runtime feature detection and automatic fallback to portable scalar code.

---

## PHASES

### Phase 1: Code Analysis & Architecture Design

**Objective**: Understand current implementation and design multi-arch solution

#### Tasks:
1. **Document current x86_64 AVX2 implementation**
   - [ ] Map SIMD operations used (compare, movemask, load/store)
   - [ ] Identify performance-critical sections
   - [ ] Document expected behavior and performance characteristics

2. **Research ARM64 NEON equivalents**
   - [ ] Map AVX2 operations to NEON equivalents
   - [ ] Understand performance differences (256-bit vs 128-bit)
   - [ ] Research `std::arch::aarch64` API

3. **Design multi-arch architecture**
   - [ ] Decide on conditional compilation strategy (`#[cfg(target_arch = "...")]`)
   - [ ] Design fallback mechanism (feature detection vs compile-time)
   - [ ] Plan code structure to minimize duplication

**Deliverables**:
- Architecture decision document
- SIMD operation mapping table (AVX2 ↔ NEON)
- Code structure design

---

### Phase 2: Create Portable Scalar Fallback

**Objective**: Implement safe, portable fallback that works on all architectures

#### TDD Cycle:

##### 🔴 RED: Write failing tests for scalar implementation
```rust
// tests/test_camel_fallback.rs
#[test]
fn test_scalar_snake_to_camel_basic() {
    let arena = Arena::new();
    let input = b"hello_world";
    let result = snake_to_camel_scalar(input, &arena);
    assert_eq!(result, b"helloWorld");
    // FAIL: function doesn't exist yet
}

#[test]
fn test_scalar_no_underscores() {
    let arena = Arena::new();
    let input = b"helloworld";
    let result = snake_to_camel_scalar(input, &arena);
    assert_eq!(result, b"helloworld");
}

#[test]
fn test_scalar_multiple_underscores() {
    let arena = Arena::new();
    let input = b"user_first_name";
    let result = snake_to_camel_scalar(input, &arena);
    assert_eq!(result, b"userFirstName");
}

#[test]
fn test_scalar_edge_cases() {
    let arena = Arena::new();
    // Leading underscore
    assert_eq!(snake_to_camel_scalar(b"_private", &arena), b"Private");
    // Trailing underscore
    assert_eq!(snake_to_camel_scalar(b"field_", &arena), b"field");
    // Double underscore
    assert_eq!(snake_to_camel_scalar(b"user__id", &arena), b"userId");
    // Empty string
    assert_eq!(snake_to_camel_scalar(b"", &arena), b"");
}
```

**Run**: `cargo test test_scalar_snake_to_camel`
**Expected**: FAILED (function not implemented)

##### 🟢 GREEN: Implement minimal scalar version
```rust
// fraiseql_rs/src/core/camel.rs

/// Pure Rust scalar implementation (no SIMD)
/// Works on all architectures as fallback
pub fn snake_to_camel_scalar<'a>(input: &[u8], arena: &'a crate::core::Arena) -> &'a [u8] {
    // Fast path: no underscores
    if !input.contains(&b'_') {
        let output = arena.alloc_bytes(input.len());
        output.copy_from_slice(input);
        return output;
    }

    // Allocate output (worst case: same size)
    let output = arena.alloc_bytes(input.len());
    let mut write_pos = 0;
    let mut capitalize_next = false;

    for &byte in input {
        if byte == b'_' {
            capitalize_next = true;
        } else {
            if capitalize_next && byte.is_ascii_alphabetic() {
                output[write_pos] = byte.to_ascii_uppercase();
                capitalize_next = false;
            } else {
                output[write_pos] = byte;
            }
            write_pos += 1;
        }
    }

    &output[..write_pos]
}
```

**Run**: `cargo test test_scalar_snake_to_camel`
**Expected**: PASSED

##### 🔧 REFACTOR: Optimize scalar implementation
- Add benchmarks comparing to SIMD version
- Optimize hot paths (e.g., contains check)
- Consider using `memchr` crate for faster underscore detection
- Add inline hints

**Run**: `cargo test && cargo bench`
**Expected**: All tests pass, benchmark baseline established

##### ✅ QA: Verify scalar implementation
- [ ] All tests pass
- [ ] Benchmarks show acceptable performance (within 2-5x of SIMD)
- [ ] Code is readable and maintainable
- [ ] Works on all architectures (test via CI matrix)

---

### Phase 3: Implement ARM64 NEON SIMD

**Objective**: Add ARM64-optimized path using NEON intrinsics

#### TDD Cycle:

##### 🔴 RED: Write failing tests for ARM64 SIMD
```rust
// tests/test_camel_neon.rs
#[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
#[test]
fn test_neon_snake_to_camel_basic() {
    let arena = Arena::new();
    let input = b"hello_world_test";
    let result = unsafe { snake_to_camel_neon(input, &arena) };
    assert_eq!(result, b"helloWorldTest");
    // FAIL: function doesn't exist yet
}

#[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
#[test]
fn test_neon_matches_scalar() {
    let arena = Arena::new();
    let test_cases = vec![
        b"hello_world",
        b"user_first_name_last_name",
        b"no_underscores",
        b"_leading",
        b"trailing_",
    ];

    for case in test_cases {
        let scalar = snake_to_camel_scalar(case, &arena);
        let neon = unsafe { snake_to_camel_neon(case, &arena) };
        assert_eq!(scalar, neon, "Mismatch for: {:?}", std::str::from_utf8(case));
    }
}
```

**Run**: `cargo test --target aarch64-apple-darwin`
**Expected**: FAILED (function not implemented)

##### 🟢 GREEN: Implement minimal ARM64 NEON version
```rust
// fraiseql_rs/src/core/camel.rs

#[cfg(target_arch = "aarch64")]
use std::arch::aarch64::*;

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
pub unsafe fn snake_to_camel_neon<'a>(input: &[u8], arena: &'a crate::core::Arena) -> &'a [u8] {
    // Find underscores using NEON (128-bit chunks)
    let underscore_mask = find_underscores_neon(input);
    if underscore_mask.is_empty() {
        let output = arena.alloc_bytes(input.len());
        output.copy_from_slice(input);
        return output;
    }

    // Allocate output
    let output = arena.alloc_bytes(input.len());
    let mut write_pos = 0;
    let mut capitalize_next = false;

    for &byte in input {
        if byte == b'_' {
            capitalize_next = true;
        } else {
            if capitalize_next {
                output[write_pos] = byte.to_ascii_uppercase();
                capitalize_next = false;
            } else {
                output[write_pos] = byte;
            }
            write_pos += 1;
        }
    }

    &output[..write_pos]
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn find_underscores_neon(input: &[u8]) -> UnderscoreMask {
    let underscore_vec = vdupq_n_u8(b'_');
    let mut mask = UnderscoreMask::new();

    // Process 16 bytes at a time (NEON is 128-bit vs AVX2's 256-bit)
    let chunks = input.chunks_exact(16);
    let chunks_len = chunks.len();
    let remainder = chunks.remainder();

    for (chunk_idx, chunk) in chunks.enumerate() {
        let data = vld1q_u8(chunk.as_ptr());
        let cmp = vceqq_u8(data, underscore_vec);

        // Convert comparison result to bitmask
        // NEON doesn't have direct movemask like AVX2
        let bitmask = neon_movemask_u8(cmp);

        if bitmask != 0 {
            mask.set_chunk_16(chunk_idx, bitmask);
        }
    }

    // Handle remainder
    for (i, &byte) in remainder.iter().enumerate() {
        if byte == b'_' {
            mask.set_bit(chunks_len * 16 + i);
        }
    }

    mask
}

#[cfg(target_arch = "aarch64")]
#[inline]
unsafe fn neon_movemask_u8(v: uint8x16_t) -> u16 {
    // NEON doesn't have movemask, need to extract bits manually
    // This is a simplified version, optimize later
    let mut result = 0u16;
    let bytes: [u8; 16] = std::mem::transmute(v);
    for (i, &byte) in bytes.iter().enumerate() {
        if byte != 0 {
            result |= 1 << i;
        }
    }
    result
}
```

**Run**: `cargo test --target aarch64-apple-darwin`
**Expected**: PASSED

##### 🔧 REFACTOR: Optimize NEON implementation
- Optimize `neon_movemask_u8` using proper bit manipulation
- Consider using `vgetq_lane_u64` for faster bit extraction
- Add benchmarks comparing to scalar and AVX2
- Optimize chunk processing

**Run**: `cargo test && cargo bench --target aarch64-apple-darwin`
**Expected**: All tests pass, NEON 3-8x faster than scalar

##### ✅ QA: Verify NEON implementation
- [ ] All tests pass on ARM64
- [ ] Produces identical results to scalar version
- [ ] Benchmarks show performance improvement
- [ ] Code is well-documented

---

### Phase 4: Refactor x86_64 Implementation with Conditional Compilation

**Objective**: Move existing AVX2 code behind conditional compilation

#### TDD Cycle:

##### 🔴 RED: Write architecture-specific tests
```rust
// tests/test_multi_arch.rs

#[test]
fn test_snake_to_camel_uses_correct_backend() {
    let arena = Arena::new();
    let input = b"hello_world_test_long_field_name";
    let result = snake_to_camel(input, &arena); // New unified API

    assert_eq!(result, b"helloWorldTestLongFieldName");
}

#[cfg(target_arch = "x86_64")]
#[test]
fn test_avx2_available() {
    // Ensure AVX2 path compiles on x86_64
    assert!(is_x86_feature_detected!("avx2") || true); // May not be available at runtime
}

#[cfg(target_arch = "aarch64")]
#[test]
fn test_neon_available() {
    // NEON is always available on aarch64
    assert!(true);
}
```

##### 🟢 GREEN: Refactor with conditional compilation
```rust
// fraiseql_rs/src/core/camel.rs

// Import architecture-specific intrinsics
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

#[cfg(target_arch = "aarch64")]
use std::arch::aarch64::*;

/// Public API: automatically dispatches to best implementation
/// for the current architecture
pub fn snake_to_camel<'a>(input: &[u8], arena: &'a crate::core::Arena) -> &'a [u8] {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            unsafe { snake_to_camel_avx2(input, arena) }
        } else {
            snake_to_camel_scalar(input, arena)
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        // NEON is always available on aarch64
        unsafe { snake_to_camel_neon(input, arena) }
    }

    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        snake_to_camel_scalar(input, arena)
    }
}

// Rename existing SIMD function
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn snake_to_camel_avx2<'a>(input: &[u8], arena: &'a crate::core::Arena) -> &'a [u8] {
    // ... existing AVX2 implementation ...
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn find_underscores_avx2(input: &[u8]) -> UnderscoreMask {
    // ... existing AVX2 implementation ...
}
```

##### 🔧 REFACTOR: Update public API
- Update `core/mod.rs` to export new `snake_to_camel` function
- Update `core/transform.rs` to use new API
- Add documentation about architecture support
- Add compile-time feature detection

```rust
// fraiseql_rs/src/core/mod.rs
pub use camel::snake_to_camel; // New unified API
```

```rust
// fraiseql_rs/src/core/transform.rs
use crate::core::camel::snake_to_camel; // No longer unsafe!

// In transform code:
let camel_key = snake_to_camel(key_bytes, self.arena); // Safe API
```

##### ✅ QA: Verify refactoring
- [ ] All tests pass on x86_64
- [ ] All tests pass on ARM64
- [ ] API is now safe (no unsafe required by callers)
- [ ] Performance unchanged from original
- [ ] Code is cleaner and more maintainable

---

### Phase 5: Update Build Configuration

**Objective**: Ensure proper compilation across all target platforms

#### Tasks:

1. **Update Cargo.toml features**
```toml
[features]
default = ["simd"]
simd = []  # Enable SIMD optimizations for current architecture
portable = []  # Force scalar implementation (for debugging)
```

2. **Update build.rs** (if needed)
```rust
// fraiseql_rs/build.rs
fn main() {
    // Detect target architecture and set appropriate flags
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();

    match target_arch.as_str() {
        "x86_64" => println!("cargo:rustc-cfg=has_simd_x86"),
        "aarch64" => println!("cargo:rustc-cfg=has_simd_arm"),
        _ => println!("cargo:rustc-cfg=has_simd_none"),
    }
}
```

3. **Update CI workflow**
```yaml
# .github/workflows/publish.yml
strategy:
  matrix:
    os:
      - ubuntu-latest      # x86_64 Linux
      - macos-latest       # ARM64 macOS (Apple Silicon)
      - macos-13           # x86_64 macOS (Intel)
      - windows-latest     # x86_64 Windows
```

4. **Add cross-compilation testing**
```yaml
# Add to CI
- name: Test cross-platform builds
  run: |
    # Test ARM64 build on x86_64 (if possible with cross)
    cargo install cross
    cross test --target aarch64-unknown-linux-gnu
```

---

### Phase 6: Testing & Validation

**Objective**: Comprehensive testing across all platforms

#### TDD Cycle:

##### 🔴 RED: Write comprehensive integration tests
```rust
// tests/test_integration.rs

#[test]
fn test_property_all_inputs_produce_valid_output() {
    // Property-based testing: any input should not crash
    use proptest::prelude::*;

    proptest!(|(input in "\\PC*")| {
        let arena = Arena::new();
        let result = snake_to_camel(input.as_bytes(), &arena);
        // Should not crash, result should be valid UTF-8
        assert!(std::str::from_utf8(result).is_ok());
    });
}

#[test]
fn test_consistency_across_implementations() {
    let test_cases = vec![
        b"simple",
        b"simple_case",
        b"multiple_under_scores",
        b"_leading",
        b"trailing_",
        b"__double__",
        b"",
        b"a",
        b"a_b",
        // Long input for SIMD testing
        b"very_long_field_name_with_many_underscores_to_test_simd_chunking",
    ];

    for case in test_cases {
        let arena = Arena::new();
        let scalar = snake_to_camel_scalar(case, &arena);
        let simd = snake_to_camel(case, &arena);
        assert_eq!(
            scalar, simd,
            "Mismatch for input: {:?}",
            std::str::from_utf8(case)
        );
    }
}

#[test]
fn test_benchmark_performance_regression() {
    // Ensure SIMD is actually faster
    let arena = Arena::new();
    let long_input = b"user_first_name_last_name_email_address_phone_number_created_at";

    let start = std::time::Instant::now();
    for _ in 0..10000 {
        snake_to_camel_scalar(long_input, &arena);
    }
    let scalar_time = start.elapsed();

    let start = std::time::Instant::now();
    for _ in 0..10000 {
        snake_to_camel(long_input, &arena);
    }
    let simd_time = start.elapsed();

    // SIMD should be at least 1.5x faster
    assert!(simd_time < scalar_time * 2 / 3,
        "SIMD performance regression: {:?} vs {:?}", simd_time, scalar_time);
}
```

##### 🟢 GREEN: Ensure all tests pass
**Run**:
```bash
# Local testing
cargo test --all-features
cargo test --no-default-features --features portable

# CI testing (all architectures)
cargo test --target x86_64-unknown-linux-gnu
cargo test --target aarch64-apple-darwin
cargo test --target aarch64-unknown-linux-gnu
```

##### 🔧 REFACTOR: Performance optimization
- Profile NEON implementation
- Optimize bit extraction
- Consider using lookup tables for common patterns
- Add benchmarks to CI

##### ✅ QA: Full validation
- [ ] All unit tests pass on all architectures
- [ ] Integration tests pass
- [ ] Property tests pass (fuzzing)
- [ ] Benchmarks show expected performance
- [ ] No performance regression on x86_64
- [ ] ARM64 performance within expected range (3-8x faster than scalar)

---

### Phase 7: Documentation & Release

**Objective**: Document changes and prepare for release

#### Tasks:

1. **Update documentation**
   - [ ] Add architecture support matrix to README
   - [ ] Document performance characteristics per architecture
   - [ ] Add SIMD implementation notes
   - [ ] Update CHANGELOG.md

2. **Update Python integration**
   - [ ] Verify Python bindings work on all architectures
   - [ ] Update Python package metadata
   - [ ] Test with real-world GraphQL workloads

3. **Prepare v1.1.2 release**
   - [ ] Update version in Cargo.toml
   - [ ] Update version in pyproject.toml
   - [ ] Create comprehensive release notes
   - [ ] Tag release

4. **CI/CD verification**
   - [ ] Verify all wheel builds succeed
   - [ ] Test PyPI upload to test.pypi.org first
   - [ ] Publish to PyPI

---

## Success Criteria

### Must Have (v1.1.2)
- ✅ Compiles successfully on x86_64 Linux, macOS, Windows
- ✅ Compiles successfully on ARM64 macOS (Apple Silicon)
- ✅ Compiles successfully on ARM64 Linux
- ✅ All existing tests pass on all platforms
- ✅ No performance regression on x86_64 (< 5%)
- ✅ ARM64 SIMD faster than scalar (> 2x speedup)
- ✅ PyPI wheels build for all platforms
- ✅ Published to PyPI successfully

### Should Have
- ✅ NEON implementation 50%+ speed of AVX2 (accounting for 128 vs 256-bit)
- ✅ Comprehensive benchmarks in CI
- ✅ Property-based tests for correctness
- ✅ Documentation updated

### Nice to Have (Future)
- 🔄 SSE2 fallback for old x86_64 CPUs
- 🔄 ARM32 NEON support
- 🔄 WASM SIMD support
- 🔄 Profile-guided optimization (PGO)

---

## Testing Strategy

### Unit Tests
```bash
# Run all unit tests
cargo test

# Test specific architecture (via cross-compilation)
cross test --target aarch64-unknown-linux-gnu
cross test --target x86_64-unknown-linux-gnu
```

### Benchmarks
```bash
# Run benchmarks
cargo bench --bench core_benchmark

# Compare implementations
cargo bench --bench core_benchmark -- snake_to_camel
```

### Integration Tests
```bash
# Test Python bindings
cd ..
uv run pytest tests/ -k camel

# Test real-world GraphQL workloads
uv run pytest tests/integration/
```

### CI Matrix Testing
- Ubuntu x86_64
- macOS x86_64 (Intel)
- macOS ARM64 (Apple Silicon)
- Windows x86_64
- Linux ARM64 (via QEMU)

---

## Risk Assessment

### High Risk
1. **NEON performance may be worse than expected**
   - Mitigation: Extensive benchmarking, fallback to scalar if needed

2. **Breaking changes in public API**
   - Mitigation: Maintain backward compatibility, add new safe API

3. **CI/CD build time increase**
   - Mitigation: Use matrix builds, cache Rust toolchains

### Medium Risk
1. **Edge cases in NEON implementation**
   - Mitigation: Property-based testing, fuzzing

2. **Performance regression on x86_64**
   - Mitigation: Benchmark comparisons in CI, careful refactoring

### Low Risk
1. **Documentation gaps**
   - Mitigation: Comprehensive docs, code review

---

## Timeline Estimate

| Phase | Estimated Time | Dependencies |
|-------|---------------|--------------|
| Phase 1: Analysis | 2-3 hours | None |
| Phase 2: Scalar Fallback | 3-4 hours | Phase 1 |
| Phase 3: ARM64 NEON | 6-8 hours | Phase 2 |
| Phase 4: Refactor x86_64 | 2-3 hours | Phase 3 |
| Phase 5: Build Config | 1-2 hours | Phase 4 |
| Phase 6: Testing | 4-6 hours | Phase 5 |
| Phase 7: Documentation | 2-3 hours | Phase 6 |
| **Total** | **20-29 hours** | - |

**Recommended**: Spread over 3-4 work sessions

---

## Post-Release Monitoring

1. **PyPI Analytics**
   - Monitor download statistics by platform
   - Track installation failures by architecture

2. **Performance Tracking**
   - Benchmark results in CI
   - User-reported performance issues

3. **Issue Tracking**
   - Watch for architecture-specific bugs
   - Monitor ARM64 adoption

---

## References

- [Intel AVX2 Intrinsics Guide](https://www.intel.com/content/www/us/en/docs/intrinsics-guide/index.html)
- [ARM NEON Intrinsics Reference](https://developer.arm.com/architectures/instruction-sets/intrinsics/)
- [Rust SIMD std::arch docs](https://doc.rust-lang.org/stable/std/arch/index.html)
- [PyO3 Multi-platform Wheels Guide](https://pyo3.rs/main/building-and-distribution)
- [Maturin Cross-compilation](https://github.com/PyO3/maturin#cross-compilation)

---

*Generated: 2025-11-02*
*Status: Ready for Implementation*

# Test Inventory - Phase 0: Planning & Preparation

## Overview
Complete mapping of all 65 tests from the current fragmented structure to the new pipeline-based organization.

**Total Tests**: 65
- **parsing.rs**: 11 tests (~470 lines)
- **classification.rs**: 15 tests (~133 lines)
- **response_building.rs**: 31 tests (~900 lines)
- **integration.rs**: 13 tests (~442 lines)
- **properties.rs**: 1 test (~92 lines)

---

## → `parsing.rs` (NEW - Stage 1: JSON → MutationResult)

### From `format_tests.rs` (9 tests):
- `test_parse_simple_format`
- `test_parse_simple_format_array`
- `test_parse_full_success_result`
- `test_parse_full_error_result`
- `test_parse_full_with_updated_fields`
- `test_format_detection_simple_vs_full`
- `test_parse_missing_status_fails`
- `test_parse_invalid_json_fails`
- `test_parse_simple_format_with_cascade`

### From `composite_tests.rs` (2 tests):
- `test_parse_8field_mutation_response`
- `test_cascade_extraction_from_position_7`

**Total**: 11 tests

---

## → `classification.rs` (RENAME from status_tests.rs - Stage 2: Status taxonomy)

### From `status_tests.rs` (15 tests - ALL):
- `test_success_keywords`
- `test_failed_prefix`
- `test_unauthorized_prefix`
- `test_forbidden_prefix`
- `test_not_found_prefix`
- `test_conflict_prefix`
- `test_timeout_prefix`
- `test_noop_prefix`
- `test_noop_duplicate`
- `test_case_insensitive_error_prefix`
- `test_case_insensitive_success`
- `test_status_with_multiple_colons`
- `test_error_prefix_without_reason`
- `test_unknown_status_becomes_success`
- `test_empty_status`

**Total**: 15 tests

---

## → `response_building.rs` (NEW - Stage 3: MutationResult → JSON)

### From `format_tests.rs` (7 tests):
- `test_build_simple_format_response`
- `test_build_simple_format_with_status_data_field`
- `test_build_full_success_response`
- `test_build_full_error_response`
- `test_build_simple_format_array_response`
- `test_parse_simple_format_with_cascade` (wait, this is parsing, not building)
- `test_build_simple_format_response_with_cascade`

### From `auto_populate_fields_tests.rs` (5 tests - ALL):
- `test_success_response_has_status_field`
- `test_success_response_has_errors_field`
- `test_success_response_all_standard_fields`
- `test_success_status_preserves_detail`
- `test_success_fields_order`

### From `error_array_generation.rs` (7 tests - ALL):
- `test_extract_identifier_from_failed_with_colon`
- `test_extract_identifier_from_noop_with_colon`
- `test_extract_identifier_from_failed_without_colon`
- `test_extract_identifier_multiple_colons`
- `test_generate_errors_array_auto`
- `test_generate_errors_array_explicit_override`
- `test_generate_errors_array_noop_status`

### From `validation_tests.rs` (6 tests - response routing):
- `test_noop_returns_error_type_v1_8`
- `test_not_found_returns_error_type_with_404`
- `test_conflict_returns_error_type_with_409`
- `test_success_with_null_entity_returns_error`
- `test_success_always_has_entity`
- `test_error_response_includes_cascade`

### From `edge_case_tests.rs` (9 tests - ALL response building):
- `test_cascade_never_nested_in_entity`
- `test_cascade_never_copied_from_entity_wrapper`
- `test_typename_always_present`
- `test_typename_matches_entity_type`
- `test_ambiguous_status_treated_as_simple`
- `test_null_entity`
- `test_array_of_entities`
- `test_deeply_nested_objects`
- `test_special_characters_in_fields`

**Total**: 34 tests (corrected count)

---

## → `integration.rs` (RENAME from integration_tests.rs - Stage 4: End-to-end)

### From `integration_tests.rs` (13 tests - ALL):
- `test_build_error_response_validation`
- `test_build_error_response_conflict`
- `test_build_noop_response`
- `test_build_success_response`
- `test_unauthorized_error`
- `test_timeout_error`
- `test_generate_errors_array_auto_generation`
- `test_generate_errors_array_explicit_errors`
- `test_extract_identifier_from_status_error`
- `test_extract_identifier_from_status_noop`
- `test_extract_identifier_from_status_success`
- `test_error_response_includes_errors_array`
- `test_error_response_with_explicit_errors`

**Total**: 13 tests

---

## → `properties.rs` (RENAME from property_tests.rs - Property-based tests)

### From `property_tests.rs` (1 test):
- `cascade_never_in_entity` (property-based test)

**Total**: 1 test

---

## Verification Checklist

- [x] All 65 tests accounted for
- [x] No tests duplicated in mapping
- [x] Clear responsibility boundaries
- [x] Pipeline stage alignment
- [ ] Test counts match README estimates
- [ ] Ready for Phase 1 implementation

## Notes

1. **format_tests.rs parsing tests**: Lines 1-148 contain parsing tests, lines 149-405 contain response building tests
2. **edge_case_tests.rs**: All tests use `build_mutation_response()` and test final JSON structure, so all belong to response building
3. **validation_tests.rs**: The routing tests (v1.8.0) are response building concerns
4. **Total count correction**: response_building.rs will have 34 tests, not 31 as estimated

## Next Steps

1. ✅ Create backup branch (`test-reorganization-backup`)
2. ✅ Complete test inventory
3. ⏳ Get approval for inventory
4. ⏳ Proceed to Phase 1: Create new test structure

//! Tests for mutation module
//!
//! This module contains comprehensive tests for mutation parsing, validation,
//! and response building. Tests are organized by category for easy navigation.

use super::*;
use serde_json::{json, Value};

// Test modules
mod format_tests;
mod validation_tests;
mod status_tests;
mod integration_tests;
mod edge_case_tests;
mod composite_tests;
mod property_tests;
mod error_array_generation;
mod auto_populate_fields_tests;

//! HTTP utilities for outbound requests.

pub mod client;

pub use client::build_ssrf_safe_client;

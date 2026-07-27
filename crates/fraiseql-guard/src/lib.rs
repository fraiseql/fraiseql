//! The FraiseQL workspace's shared safety guards.
//!
//! Two things live here, because the guards that matter combine them: an
//! outbound-request guard is only as good as its answer to "am I in production?",
//! and every insecure-mode escape hatch in the product is exactly
//! `bypass_requested() && !is_production()`.
//!
//! - [`net`] — the single outbound-address guard. Which IP addresses and hostnames a request may
//!   target.
//! - [`deployment`] — the single answer to whether this process is running in production, and
//!   therefore whether a development escape hatch may be honoured.
//!
//! # Why one crate
//!
//! Both concerns previously had many implementations that disagreed. The
//! workspace carried **eight** hand-rolled address predicates (#776, #802) and
//! **two** production detectors reading the same environment variable with
//! opposite defaults (#836). Each duplicate was individually reasonable and
//! collectively exploitable: a bypass refused by one crate was honoured by its
//! neighbour.
//!
//! This crate depends on `std` only, so it sits at the bottom of the dependency
//! graph and every crate can reach it. Nothing here may grow a dependency that
//! would make it unreachable from a leaf crate.

#![forbid(unsafe_code)]

pub mod deployment;
pub mod net;

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
//! - [`kafka`] — endpoint parsing and the plaintext refusal for Kafka, shared by the CDC outbox
//!   sink and the subscription transport (#1102). A transport whose own wire format cannot express
//!   transport security needs one answer to "may this be plaintext?", not one per caller.
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
//! This crate sits at the bottom of the dependency graph so every crate can
//! reach it: `std`, plus the `tracing` facade, which every leaf crate in the
//! workspace already depends on and which is what lets a refused escape hatch be
//! reported from the one place that decides it (#882). Nothing here may grow a
//! dependency that would make it unreachable from a leaf crate.

#![forbid(unsafe_code)]

pub mod deployment;
pub mod kafka;
pub mod net;

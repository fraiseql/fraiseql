//! Deno host operation declarations for FraiseQL guest functions.
//!
//! The primary op — `fraiseql_log` — is implemented directly in `executor.rs`
//! via the `#[op2(fast)]` attribute so it can share the `LogCollector` state
//! that is already wired into `OpState` there.
//!
//! Future host I/O ops (`query`, `http_request`, `storage_get/put`, `auth_context`,
//! `env_var`) will be added here as the async bridge from `HostContext` to
//! Deno's event loop is implemented.

#[cfg(test)]
mod tests {
    #[test]
    fn test_ops_module_compiles() {
        // Smoke-test: this module should always compile.
    }
}

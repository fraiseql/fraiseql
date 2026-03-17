#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Need at least 9 bytes: u32 max_requests, u64 window_secs, rest = key
    if data.len() < 9 {
        return;
    }
    let max_requests = u32::from_le_bytes(data[0..4].try_into().unwrap_or([0; 4]));
    let window_secs = u64::from_le_bytes(data[1..9].try_into().unwrap_or([0; 8]));
    let key = std::str::from_utf8(&data[9..]).unwrap_or("fuzz_key");

    let config = fraiseql_core::validation::rate_limiting::ValidationRateLimitingConfig {
        enabled: true,
        validation_errors_max_requests: max_requests,
        validation_errors_window_secs: window_secs,
        depth_errors_max_requests: max_requests,
        depth_errors_window_secs: window_secs,
        complexity_errors_max_requests: max_requests,
        complexity_errors_window_secs: window_secs,
        malformed_errors_max_requests: max_requests,
        malformed_errors_window_secs: window_secs,
        async_validation_errors_max_requests: max_requests,
        async_validation_errors_window_secs: window_secs,
    };
    let limiter =
        fraiseql_core::validation::rate_limiting::ValidationRateLimiter::new(config);
    // Must never panic on arbitrary key/config combinations
    let _ = limiter.check_validation_errors(key);
    let _ = limiter.check_depth_errors(key);
    let _ = limiter.check_complexity_errors(key);
    let _ = limiter.check_malformed_errors(key);
});

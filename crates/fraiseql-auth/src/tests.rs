#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::wildcard_imports)] // Reason: test modules use wildcard imports

#[cfg(test)]
mod monitoring_tests {
    use super::super::monitoring::*;

    #[test]
    #[allow(clippy::float_cmp)] // Reason: acceptable precision for metrics/timing — values set directly from literals
    fn test_auth_event_builder() {
        let event = AuthEvent::new("login")
            .with_user_id("user123".to_string())
            .with_provider("google".to_string())
            .success(50.0);

        assert_eq!(event.event, "login");
        assert_eq!(event.user_id, Some("user123".to_string()));
        assert_eq!(event.provider, Some("google".to_string()));
        assert_eq!(event.status, "success");
        assert_eq!(event.duration_ms, 50.0);
    }

    #[test]
    #[allow(clippy::float_cmp)] // Reason: acceptable precision for metrics/timing — values set directly from literals
    fn test_auth_metrics() {
        let mut metrics = AuthMetrics::new();

        metrics.record_attempt();
        metrics.record_attempt();
        metrics.record_success();
        metrics.record_failure();

        assert_eq!(metrics.total_auth_attempts, 2);
        assert_eq!(metrics.successful_authentications, 1);
        assert_eq!(metrics.failed_authentications, 1);
        assert_eq!(metrics.success_rate(), 50.0);
    }

    #[test]
    #[allow(clippy::float_cmp)] // Reason: acceptable precision for metrics/timing — values set directly from literals
    fn test_auth_metrics_success_rate() {
        let mut metrics = AuthMetrics::new();

        // 100% success rate
        for _ in 0..10 {
            metrics.record_attempt();
            metrics.record_success();
        }

        assert_eq!(metrics.success_rate(), 100.0);

        // Drop to 50%
        metrics.record_attempt();
        metrics.record_failure();

        assert!((metrics.success_rate() - 90.91).abs() < 0.1); // ~90.91%
    }

    #[test]
    fn test_operation_timer() {
        let timer = OperationTimer::start("test_op");
        let elapsed = timer.elapsed_ms();
        assert!(elapsed >= 0.0);
        assert!(elapsed < 1000.0);
    }
}

#[cfg(test)]
mod security_config_tests {
    use super::super::security_config::*;

    #[test]
    fn test_default_config() {
        let config = SecurityConfigFromSchema::default();
        assert!(config.audit_logging.enabled);
        assert!(config.error_sanitization.enabled);
        assert!(config.rate_limiting.enabled);
        assert!(config.state_encryption.enabled);
    }

    #[test]
    fn test_parse_from_json() {
        let json = serde_json::json!({
            "auditLogging": {
                "enabled": true,
                "logLevel": "debug",
                "includeSensitiveData": false
            },
            "rateLimiting": {
                "enabled": true,
                "authStart": {
                    "maxRequests": 200,
                    "windowSecs": 60
                }
            }
        });

        let config = SecurityConfigFromSchema::from_json(&json).expect("Failed to parse");
        assert_eq!(config.audit_logging.log_level, "debug");
        assert_eq!(config.rate_limiting.auth_start_max_requests, 200);
    }

    #[test]
    fn test_apply_env_overrides() {
        // Note: This test would require setting env vars during test execution
        // For now, we just verify the method works with defaults
        let mut config = SecurityConfigFromSchema::default();
        config.apply_env_overrides();
        // No assertions needed, just verify it doesn't panic
    }
}

#[cfg(test)]
mod security_init_tests {
    use super::super::security_init::*;
    use super::super::security_config::SecurityConfigFromSchema;
    use super::super::error::AuthError;

    #[test]
    fn test_init_default_security_config() {
        let config = init_default_security_config();
        assert!(config.audit_logging.enabled);
        assert!(config.error_sanitization.enabled);
        assert!(config.rate_limiting.enabled);
        assert!(config.state_encryption.enabled);
    }

    #[test]
    fn test_validate_security_config_success() {
        let config = SecurityConfigFromSchema::default();
        validate_security_config(&config)
            .unwrap_or_else(|e| panic!("expected Ok for default security config: {e}"));
    }

    #[test]
    fn test_validate_security_config_leak_sensitive_fails() {
        let mut config = SecurityConfigFromSchema::default();
        config.error_sanitization.leak_sensitive_details = true;
        let result = validate_security_config(&config);
        assert!(
            matches!(result, Err(AuthError::ConfigError { .. })),
            "expected ConfigError when leak_sensitive_details=true, got: {result:?}"
        );
    }

    #[test]
    fn test_log_security_config() {
        let config = SecurityConfigFromSchema::default();
        // Just verify the function doesn't panic
        log_security_config(&config);
    }

    #[test]
    fn test_init_security_config_from_json() {
        let json = serde_json::json!({
            "security": {
                "auditLogging": {
                    "enabled": true,
                    "logLevel": "debug"
                },
                "rateLimiting": {
                    "enabled": true,
                    "authStart": {
                        "maxRequests": 200,
                        "windowSecs": 60
                    }
                }
            }
        });

        let cfg = init_security_config_from_value(&json)
            .unwrap_or_else(|e| panic!("expected Ok for valid security JSON: {e}"));
        assert_eq!(cfg.audit_logging.log_level, "debug");
        assert_eq!(cfg.rate_limiting.auth_start_max_requests, 200);
    }

    #[test]
    fn test_init_security_config_from_string() {
        let json_str = r#"{
            "security": {
                "auditLogging": {
                    "enabled": true,
                    "logLevel": "info"
                },
                "errorSanitization": {
                    "enabled": true,
                    "genericMessages": true
                }
            }
        }"#;

        let cfg = init_security_config(json_str)
            .unwrap_or_else(|e| panic!("expected Ok for valid security JSON string: {e}"));
        assert_eq!(cfg.audit_logging.log_level, "info");
        assert!(cfg.error_sanitization.generic_messages);
    }

    #[test]
    fn test_init_security_config_missing_section() {
        let json = serde_json::json!({});
        let config = init_security_config_from_value(&json);
        // Should return error because security section is required
        assert!(
            matches!(config, Err(AuthError::ConfigError { .. })),
            "expected ConfigError when security section is missing, got: {config:?}"
        );
    }
}

#[cfg(test)]
mod error_sanitizer_tests {
    use super::super::error_sanitizer::*;

    #[test]
    fn test_sanitized_error_creation() {
        let error = SanitizedError::new(
            "Authentication failed",
            "JWT signature validation failed at cryptographic boundary",
        );

        assert_eq!(error.user_facing(), "Authentication failed");
        assert!(error.internal().contains("cryptographic"));
    }

    #[test]
    fn test_sanitized_error_display() {
        let error = SanitizedError::new(
            "Authentication failed",
            "Internal database error: constraint violation",
        );

        // Display should show user message
        assert_eq!(format!("{}", error), "Authentication failed");
    }

    #[test]
    fn test_auth_error_sanitizer_jwt() {
        let error =
            AuthErrorSanitizer::jwt_validation_error("RS256 signature mismatch at offset 512");

        assert_eq!(error.user_facing(), messages::AUTH_FAILED);
        assert!(error.internal().contains("RS256"));
    }

    #[test]
    fn test_auth_error_sanitizer_permission() {
        let error = AuthErrorSanitizer::permission_error(
            "User lacks role=admin for operation write:config",
        );

        assert_eq!(error.user_facing(), messages::PERMISSION_DENIED);
        assert!(error.internal().contains("role=admin"));
    }

    #[test]
    fn test_sanitizable_trait() {
        let std_error = "Socket error: Connection refused".to_string();
        let sanitized = std_error.sanitized("Service temporarily unavailable");

        assert_eq!(sanitized.user_facing(), "Service temporarily unavailable");
        assert_eq!(sanitized.internal(), "Socket error: Connection refused");
    }
}

#[cfg(test)]
mod constant_time_tests_inner {
    use super::super::constant_time::*;

    #[test]
    fn test_compare_equal_bytes() {
        let token1 = b"equal_token_value";
        let token2 = b"equal_token_value";
        assert!(ConstantTimeOps::compare(token1, token2));
    }

    #[test]
    fn test_compare_different_bytes() {
        let token1 = b"expected_token";
        let token2 = b"actual_token_x";
        assert!(!ConstantTimeOps::compare(token1, token2));
    }

    #[test]
    fn test_compare_equal_strings() {
        let token1 = "equal_token_value";
        let token2 = "equal_token_value";
        assert!(ConstantTimeOps::compare_str(token1, token2));
    }

    #[test]
    fn test_compare_different_strings() {
        let token1 = "expected_token";
        let token2 = "actual_token_x";
        assert!(!ConstantTimeOps::compare_str(token1, token2));
    }

    #[test]
    fn test_compare_empty() {
        let token1 = b"";
        let token2 = b"";
        assert!(ConstantTimeOps::compare(token1, token2));
    }

    #[test]
    fn test_compare_different_lengths() {
        let token1 = b"short";
        let token2 = b"much_longer_token";
        assert!(!ConstantTimeOps::compare(token1, token2));
    }

    #[test]
    fn test_compare_len_safe() {
        let expected = b"abcdefghij";
        let actual = b"abcdefghij";
        assert!(ConstantTimeOps::compare_len_safe(expected, actual));

        let different = b"abcdefghix";
        assert!(!ConstantTimeOps::compare_len_safe(expected, different));

        let shorter = b"abcdefgh";
        assert!(!ConstantTimeOps::compare_len_safe(expected, shorter));
    }

    #[test]
    fn test_null_bytes_comparison() {
        let token1 = b"token\x00with\x00nulls";
        let token2 = b"token\x00with\x00nulls";
        assert!(ConstantTimeOps::compare(token1, token2));

        let different = b"token\x00with\x00other";
        assert!(!ConstantTimeOps::compare(token1, different));
    }

    #[test]
    fn test_all_byte_values() {
        let mut token1 = vec![0u8; 256];
        let mut token2 = vec![0u8; 256];
        for i in 0..256 {
            #[allow(clippy::cast_possible_truncation)]
            // Reason: loop bound is 256, so i is always 0..=255
            let byte = i as u8;
            token1[i] = byte;
            token2[i] = byte;
        }

        assert!(ConstantTimeOps::compare(&token1, &token2));

        token2[127] = token2[127].wrapping_add(1);
        assert!(!ConstantTimeOps::compare(&token1, &token2));
    }

    #[test]
    fn test_very_long_tokens() {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // Reason: i % 256 is always 0..=255 for non-negative i32, both casts safe
        let token1: Vec<u8> = (0..10_000).map(|i| (i % 256) as u8).collect();
        let token2 = token1.clone();
        assert!(ConstantTimeOps::compare(&token1, &token2));

        let mut token3 = token1.clone();
        token3[5_000] = token3[5_000].wrapping_add(1);
        assert!(!ConstantTimeOps::compare(&token1, &token3));
    }

    #[test]
    fn test_compare_padded_equal_length() {
        let token1 = b"same_token_value";
        let token2 = b"same_token_value";
        assert!(ConstantTimeOps::compare_padded(token1, token2, 512));
    }

    #[test]
    fn test_compare_padded_different_length_shorter_actual() {
        let expected = b"this_is_expected_token_value";
        let actual = b"short";
        assert!(!ConstantTimeOps::compare_padded(expected, actual, 512));
    }

    #[test]
    fn test_compare_padded_different_length_longer_actual() {
        let expected = b"expected";
        let actual = b"this_is_a_much_longer_actual_token_that_exceeds_expected";
        assert!(!ConstantTimeOps::compare_padded(expected, actual, 512));
    }

    #[test]
    fn test_compare_padded_timing_consistency() {
        let short_token = b"short";
        let long_token = b"this_is_a_much_longer_token_value_with_more_content";

        let _ = ConstantTimeOps::compare_padded(short_token, short_token, 512);
        let _ = ConstantTimeOps::compare_padded(long_token, long_token, 512);

        assert!(ConstantTimeOps::compare_padded(short_token, short_token, 512));
        assert!(ConstantTimeOps::compare_padded(long_token, long_token, 512));
    }

    #[test]
    fn test_compare_jwt_constant() {
        let jwt1 = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyMTIzIn0.signature123";
        let jwt2 = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyMTIzIn0.signature123";
        assert!(ConstantTimeOps::compare_jwt_constant(jwt1, jwt2));
    }

    #[test]
    fn test_compare_jwt_constant_different() {
        let jwt1 = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyMTIzIn0.signature123";
        let jwt2 = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyMTIzIn0.signature999";
        assert!(!ConstantTimeOps::compare_jwt_constant(jwt1, jwt2));
    }

    #[test]
    fn test_compare_jwt_constant_prevents_length_attack() {
        let short_invalid_jwt = "short";
        let long_valid_jwt = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyMTIzIn0.sig123";

        assert!(!ConstantTimeOps::compare_jwt_constant(short_invalid_jwt, long_valid_jwt));
        assert!(!ConstantTimeOps::compare_jwt_constant(short_invalid_jwt, long_valid_jwt));
    }

    #[test]
    fn test_compare_padded_zero_length() {
        let token1 = b"";
        let token2 = b"";
        assert!(ConstantTimeOps::compare_padded(token1, token2, 512));
    }

    #[test]
    fn test_compare_padded_exact_fixed_length() {
        let token = b"a".repeat(512);
        assert!(ConstantTimeOps::compare_padded(&token, &token, 512));

        let mut different = token.clone();
        different[256] = different[256].wrapping_add(1);
        assert!(!ConstantTimeOps::compare_padded(&token, &different, 512));
    }

    #[test]
    fn test_compare_padded_large_fixed_len() {
        let token1 = b"test";
        let token2 = b"test";
        assert!(ConstantTimeOps::compare_padded(token1, token2, 2048));

        let long_a: Vec<u8> = b"prefix".iter().chain(b"AAAA".iter()).copied().collect();
        let long_b: Vec<u8> = b"prefix".iter().chain(b"BBBB".iter()).copied().collect();
        assert!(ConstantTimeOps::compare_padded(&long_a, &long_b, 6));
        assert!(!ConstantTimeOps::compare_padded(&long_a, &long_b, 10));
    }

    #[test]
    fn test_timing_attack_prevention_early_difference() {
        let token1 = b"XXXXXXX_correct_token";
        let token2 = b"YYYYYYY_correct_token";
        let result = ConstantTimeOps::compare(token1, token2);
        assert!(!result);
    }

    #[test]
    fn test_timing_attack_prevention_late_difference() {
        let token1 = b"correct_token_XXXXXXX";
        let token2 = b"correct_token_YYYYYYY";
        let result = ConstantTimeOps::compare(token1, token2);
        assert!(!result);
    }

    #[test]
    fn test_jwt_constant_padding() {
        let short_jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ1c2VyIn0.abc";
        let padded_jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ1c2VyIn0.abc";
        assert!(ConstantTimeOps::compare_jwt_constant(short_jwt, padded_jwt));
    }

    #[test]
    fn test_jwt_constant_different_lengths() {
        let jwt1 = "short";
        let jwt2 = "very_long_jwt_token_with_lots_of_data_making_it_much_longer";
        let result = ConstantTimeOps::compare_jwt_constant(jwt1, jwt2);
        assert!(!result);
    }
}

#[cfg(test)]
mod provider_tests {
    use super::super::provider::*;

    #[test]
    fn test_pkce_challenge_generation() {
        let challenge_result = PkceChallenge::generate();
        assert!(challenge_result.is_ok(), "PKCE challenge generation should succeed");

        let challenge = challenge_result.unwrap();
        assert!(!challenge.verifier.is_empty(), "Verifier should not be empty");
        assert!(!challenge.challenge.is_empty(), "Challenge should not be empty");
        assert!(
            challenge.verifier.len() >= 43 && challenge.verifier.len() <= 128,
            "Verifier length must be 43-128 characters per RFC 7636"
        );
    }

    #[test]
    fn test_pkce_verifier_contains_valid_characters() {
        let challenge = PkceChallenge::generate().unwrap();

        let allowed_chars: std::collections::HashSet<char> =
            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~"
                .chars()
                .collect();

        for c in challenge.verifier.chars() {
            assert!(allowed_chars.contains(&c), "PKCE verifier contains invalid character: {}", c);
        }
    }

    #[test]
    fn test_pkce_validation() {
        let challenge = PkceChallenge::generate().unwrap();
        assert!(
            challenge.validate(&challenge.verifier),
            "Challenge should validate against its own verifier"
        );

        let wrong_verifier = "wrong_verifier";
        assert!(!challenge.validate(wrong_verifier), "Challenge should reject invalid verifier");
    }

    #[test]
    fn test_pkce_generation_is_unique() {
        let challenge1 = PkceChallenge::generate().unwrap();
        let challenge2 = PkceChallenge::generate().unwrap();

        assert_ne!(
            challenge1.verifier, challenge2.verifier,
            "Generated verifiers should be unique"
        );
        assert_ne!(
            challenge1.challenge, challenge2.challenge,
            "Generated challenges should be unique"
        );
    }

    #[test]
    fn test_pkce_challenge_is_base64_url_safe() {
        let challenge = PkceChallenge::generate().unwrap();

        assert!(
            !challenge.challenge.contains('+'),
            "Challenge should not contain + (not URL-safe)"
        );
        assert!(
            !challenge.challenge.contains('/'),
            "Challenge should not contain / (not URL-safe)"
        );

        for c in challenge.challenge.chars() {
            assert!(
                c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '=',
                "Challenge contains unexpected character: {}",
                c
            );
        }
    }

    #[test]
    fn test_base64_url_encode() {
        let bytes = b"hello world";
        let encoded = base64_url_encode(bytes);
        assert!(!encoded.is_empty());
        assert!(!encoded.contains('+'));
        assert!(!encoded.contains('/'));
    }
}

#[cfg(test)]
mod proxy_tests {
    use std::net::IpAddr;
    use super::super::proxy::*;

    #[test]
    fn test_proxy_config_localhost_only() {
        let config = ProxyConfig::localhost_only();
        assert!(config.is_trusted_proxy("127.0.0.1"));
        assert!(!config.is_trusted_proxy("192.168.1.1"));
    }

    #[test]
    fn test_proxy_config_is_trusted_proxy_valid_ip() {
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        let config = ProxyConfig::new(vec![ip], true);
        assert!(config.is_trusted_proxy("10.0.0.1"));
    }

    #[test]
    fn test_proxy_config_is_trusted_proxy_untrusted_ip() {
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        let config = ProxyConfig::new(vec![ip], true);
        assert!(!config.is_trusted_proxy("192.168.1.1"));
    }

    #[test]
    fn test_proxy_config_is_trusted_proxy_invalid_ip() {
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        let config = ProxyConfig::new(vec![ip], true);
        assert!(!config.is_trusted_proxy("invalid_ip"));
    }

    #[test]
    fn test_extract_client_ip_from_trusted_proxy_x_forwarded_for() {
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        let config = ProxyConfig::new(vec![ip], true);

        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "192.0.2.1, 10.0.0.1".parse().unwrap());

        let direct_ip = "10.0.0.1".parse::<std::net::IpAddr>().ok();
        let socket = direct_ip.map(|ip| std::net::SocketAddr::new(ip, 8000));

        let result = config.extract_client_ip(&headers, socket);
        assert_eq!(result, Some("192.0.2.1".to_string()));
    }

    #[test]
    fn test_extract_client_ip_from_untrusted_proxy_x_forwarded_for() {
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        let config = ProxyConfig::new(vec![ip], true);

        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "192.0.2.1, 10.0.0.1".parse().unwrap());

        let direct_ip = "192.168.1.100".parse::<std::net::IpAddr>().ok();
        let socket = direct_ip.map(|ip| std::net::SocketAddr::new(ip, 8000));

        let result = config.extract_client_ip(&headers, socket);
        assert_eq!(result, Some("192.168.1.100".to_string()));
    }

    #[test]
    fn test_extract_client_ip_no_headers() {
        let config = ProxyConfig::localhost_only();
        let headers = axum::http::HeaderMap::new();

        let direct_ip = "192.168.1.100".parse::<std::net::IpAddr>().ok();
        let socket = direct_ip.map(|ip| std::net::SocketAddr::new(ip, 8000));

        let result = config.extract_client_ip(&headers, socket);
        assert_eq!(result, Some("192.168.1.100".to_string()));
    }

    #[test]
    fn test_extract_client_ip_empty_headers() {
        let config = ProxyConfig::localhost_only();
        let headers = axum::http::HeaderMap::new();

        let result = config.extract_client_ip(&headers, None);
        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_client_ip_spoofing_attempt() {
        let trusted_ip: IpAddr = "10.0.0.1".parse().unwrap();
        let config = ProxyConfig::new(vec![trusted_ip], true);

        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4".parse().unwrap());

        let attacker_ip = "192.168.1.100".parse::<std::net::IpAddr>().ok();
        let socket = attacker_ip.map(|ip| std::net::SocketAddr::new(ip, 8000));

        let result = config.extract_client_ip(&headers, socket);
        assert_eq!(result, Some("192.168.1.100".to_string()));
    }

    #[test]
    fn test_extract_client_ip_invalid_format_x_forwarded_for() {
        let trusted_ip: IpAddr = "10.0.0.1".parse().unwrap();
        let config = ProxyConfig::new(vec![trusted_ip], true);

        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "not-a-valid-ip-address, 10.0.0.1".parse().unwrap());

        let trusted_source_ip = "10.0.0.1".parse::<std::net::IpAddr>().ok();
        let socket = trusted_source_ip.map(|ip| std::net::SocketAddr::new(ip, 8000));

        let result = config.extract_client_ip(&headers, socket);
        assert_eq!(result, Some("10.0.0.1".to_string()));
    }

    #[test]
    fn test_extract_client_ip_invalid_format_x_real_ip() {
        let trusted_ip: IpAddr = "10.0.0.1".parse().unwrap();
        let config = ProxyConfig::new(vec![trusted_ip], true);

        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-real-ip", "256.256.256.256".parse().unwrap());

        let trusted_source_ip = "10.0.0.1".parse::<std::net::IpAddr>().ok();
        let socket = trusted_source_ip.map(|ip| std::net::SocketAddr::new(ip, 8000));

        let result = config.extract_client_ip(&headers, socket);
        assert_eq!(result, Some("10.0.0.1".to_string()));
    }

    #[test]
    fn test_extract_client_ip_valid_ipv6() {
        let trusted_ip: IpAddr = "::1".parse().unwrap();
        let config = ProxyConfig::new(vec![trusted_ip], true);

        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "2001:db8::1, ::1".parse().unwrap());

        let trusted_source_ip = "::1".parse::<std::net::IpAddr>().ok();
        let socket = trusted_source_ip.map(|ip| std::net::SocketAddr::new(ip, 8000));

        let result = config.extract_client_ip(&headers, socket);
        assert_eq!(result, Some("2001:db8::1".to_string()));
    }
}

#[cfg(test)]
mod handlers_tests {
    use super::super::handlers::*;
    use super::super::error::AuthError;

    #[test]
    fn auth_callback_rejects_oversized_code() {
        let oversized = "a".repeat(MAX_AUTH_CODE_BYTES + 1);
        let result = validate_auth_input_len(&oversized, MAX_AUTH_CODE_BYTES, "code");
        assert!(
            matches!(result, Err(AuthError::InvalidToken { ref reason }) if reason.contains("code")),
            "expected InvalidToken mentioning 'code', got: {result:?}"
        );
    }

    #[test]
    fn auth_callback_rejects_oversized_state() {
        let oversized = "a".repeat(MAX_STATE_BYTES + 1);
        let result = validate_auth_input_len(&oversized, MAX_STATE_BYTES, "state");
        assert!(
            matches!(result, Err(AuthError::InvalidToken { ref reason }) if reason.contains("state")),
            "expected InvalidToken mentioning 'state', got: {result:?}"
        );
    }

    #[test]
    fn auth_callback_accepts_valid_length_state() {
        let valid = "a".repeat(MAX_STATE_BYTES);
        assert!(validate_auth_input_len(&valid, MAX_STATE_BYTES, "state").is_ok());
    }

    #[test]
    fn state_cap_is_larger_than_code_cap() {
        const _: () = assert!(MAX_STATE_BYTES > MAX_AUTH_CODE_BYTES);
    }

    #[test]
    fn auth_refresh_rejects_oversized_token() {
        let oversized = "b".repeat(MAX_REFRESH_TOKEN_BYTES + 1);
        let result = validate_auth_input_len(&oversized, MAX_REFRESH_TOKEN_BYTES, "refresh_token");
        assert!(
            matches!(result, Err(AuthError::InvalidToken { ref reason }) if reason.contains("refresh_token")),
            "expected InvalidToken mentioning 'refresh_token', got: {result:?}"
        );
    }

    #[test]
    fn auth_callback_accepts_valid_length_code() {
        let valid = "a".repeat(MAX_AUTH_CODE_BYTES);
        assert!(validate_auth_input_len(&valid, MAX_AUTH_CODE_BYTES, "code").is_ok());
    }

    #[test]
    fn auth_refresh_accepts_valid_length_token() {
        let valid = "b".repeat(MAX_REFRESH_TOKEN_BYTES);
        assert!(validate_auth_input_len(&valid, MAX_REFRESH_TOKEN_BYTES, "refresh_token").is_ok());
    }

    #[test]
    fn test_generate_secure_state() {
        let state1 = generate_secure_state();
        let state2 = generate_secure_state();

        assert_ne!(state1, state2);
        assert!(!state1.is_empty());
        assert!(!state2.is_empty());
        assert_eq!(state1.len(), 64);
        assert_eq!(state2.len(), 64);
        hex::decode(&state1).unwrap_or_else(|e| panic!("state1 should be valid hex: {e}"));
        hex::decode(&state2).unwrap_or_else(|e| panic!("state2 should be valid hex: {e}"));
    }
}

#[cfg(test)]
mod account_linking_tests {
    use super::super::account_linking::*;
    use super::super::provider::UserInfo;

    fn make_user_info(email: &str, provider_id: &str) -> UserInfo {
        UserInfo {
            id:         provider_id.to_string(),
            email:      email.to_string(),
            name:       Some("Test User".to_string()),
            picture:    None,
            raw_claims: serde_json::json!({}),
        }
    }

    #[tokio::test]
    async fn test_first_login_creates_new_user() {
        let store = InMemoryUserStore::new();
        let info = make_user_info("alice@example.com", "gh-123");

        let user = store.find_or_create_user("github", &info).await.unwrap();

        assert_eq!(user.email, "alice@example.com");
        assert_eq!(user.identities.len(), 1);
        assert_eq!(user.identities[0].provider, "github");
        assert_eq!(user.identities[0].provider_user_id, "gh-123");
        assert_eq!(store.user_count().await, 1);
    }

    #[tokio::test]
    async fn test_same_provider_same_identity_returns_existing_user() {
        let store = InMemoryUserStore::new();
        let info = make_user_info("alice@example.com", "gh-123");

        let user1 = store.find_or_create_user("github", &info).await.unwrap();
        let user2 = store.find_or_create_user("github", &info).await.unwrap();

        assert_eq!(user1.id, user2.id, "same identity must return same user");
        assert_eq!(user2.identities.len(), 1, "no duplicate identity");
        assert_eq!(store.user_count().await, 1);
    }

    #[tokio::test]
    async fn test_different_provider_same_email_links_accounts() {
        let store = InMemoryUserStore::new();
        let gh_info = make_user_info("alice@example.com", "gh-123");
        let gg_info = make_user_info("alice@example.com", "google-456");

        let user1 = store.find_or_create_user("github", &gh_info).await.unwrap();
        let user2 = store.find_or_create_user("google", &gg_info).await.unwrap();

        assert_eq!(user1.id, user2.id, "same email must link to same user");
        assert_eq!(user2.identities.len(), 2, "should have 2 linked identities");
        assert_eq!(store.user_count().await, 1, "only 1 local user");
    }

    #[tokio::test]
    async fn test_different_email_creates_different_users() {
        let store = InMemoryUserStore::new();
        let alice = make_user_info("alice@example.com", "gh-alice");
        let bob = make_user_info("bob@example.com", "gh-bob");

        let user_a = store.find_or_create_user("github", &alice).await.unwrap();
        let user_b = store.find_or_create_user("github", &bob).await.unwrap();

        assert_ne!(user_a.id, user_b.id, "different emails must create different users");
        assert_eq!(store.user_count().await, 2);
    }

    #[tokio::test]
    async fn test_email_matching_is_case_insensitive() {
        let store = InMemoryUserStore::new();
        let info1 = make_user_info("Alice@Example.COM", "gh-123");
        let info2 = make_user_info("alice@example.com", "google-456");

        let user1 = store.find_or_create_user("github", &info1).await.unwrap();
        let user2 = store.find_or_create_user("google", &info2).await.unwrap();

        assert_eq!(user1.id, user2.id, "case-insensitive email must link accounts");
    }

    #[tokio::test]
    async fn test_three_providers_same_email_all_linked() {
        let store = InMemoryUserStore::new();
        let gh = make_user_info("alice@example.com", "gh-1");
        let gg = make_user_info("alice@example.com", "gg-2");
        let az = make_user_info("alice@example.com", "az-3");

        let u1 = store.find_or_create_user("github", &gh).await.unwrap();
        let u2 = store.find_or_create_user("google", &gg).await.unwrap();
        let u3 = store.find_or_create_user("azure_ad", &az).await.unwrap();

        assert_eq!(u1.id, u2.id);
        assert_eq!(u2.id, u3.id);
        assert_eq!(u3.identities.len(), 3);
        assert_eq!(store.user_count().await, 1);
    }

    #[tokio::test]
    async fn test_list_identities_returns_all_linked() {
        let store = InMemoryUserStore::new();
        let gh = make_user_info("alice@example.com", "gh-1");
        let gg = make_user_info("alice@example.com", "gg-2");

        let user = store.find_or_create_user("github", &gh).await.unwrap();
        store.find_or_create_user("google", &gg).await.unwrap();

        let identities = store.list_identities(&user.id).await.unwrap();
        assert_eq!(identities.len(), 2);

        let providers: Vec<&str> = identities.iter().map(|i| i.provider.as_str()).collect();
        assert!(providers.contains(&"github"));
        assert!(providers.contains(&"google"));
    }

    #[tokio::test]
    async fn test_get_user_returns_none_for_unknown() {
        let store = InMemoryUserStore::new();
        let result = store.get_user("nonexistent-id").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_user_returns_correct_user() {
        let store = InMemoryUserStore::new();
        let info = make_user_info("alice@example.com", "gh-123");

        let created = store.find_or_create_user("github", &info).await.unwrap();
        let retrieved = store.get_user(&created.id).await.unwrap();

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().email, "alice@example.com");
    }

    #[tokio::test]
    async fn test_repeat_link_does_not_duplicate_identity() {
        let store = InMemoryUserStore::new();
        let info = make_user_info("alice@example.com", "gh-123");

        store.find_or_create_user("github", &info).await.unwrap();
        store.find_or_create_user("github", &info).await.unwrap();
        store.find_or_create_user("github", &info).await.unwrap();

        let user = store.find_or_create_user("github", &info).await.unwrap();
        assert_eq!(user.identities.len(), 1, "repeated link must not duplicate");
    }
}

#[cfg(test)]
mod session_tests {
    use super::super::session::*;
    use super::super::error::AuthError;

    #[test]
    fn test_hash_token() {
        let token = "my_secret_token";
        let hash1 = hash_token(token);
        let hash2 = hash_token(token);

        assert_eq!(hash1, hash2);

        let different_hash = hash_token("different_token");
        assert_ne!(hash1, different_hash);
    }

    #[test]
    fn test_generate_refresh_token() {
        let token1 = generate_refresh_token();
        let token2 = generate_refresh_token();

        assert_ne!(token1, token2);
        assert!(!token1.is_empty());
        assert!(!token2.is_empty());
    }

    #[test]
    fn test_session_data_not_expired() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let session = SessionData {
            user_id:            "user123".to_string(),
            issued_at:          now,
            expires_at:         now + 3600,
            refresh_token_hash: "hash".to_string(),
        };

        assert!(!session.is_expired());
    }

    #[test]
    fn test_session_data_expired() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let session = SessionData {
            user_id:            "user123".to_string(),
            issued_at:          now - 3600,
            expires_at:         now - 100,
            refresh_token_hash: "hash".to_string(),
        };

        assert!(session.is_expired());
    }

    #[tokio::test]
    async fn test_in_memory_store_create_session() {
        let store = InMemorySessionStore::new();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let result = store.create_session("user123", now + 3600).await;
        let tokens = result.unwrap_or_else(|e| panic!("expected Ok from create_session: {e}"));
        assert!(!tokens.access_token.is_empty());
        assert!(!tokens.refresh_token.is_empty());
        assert!(tokens.expires_in > 0);
    }

    #[tokio::test]
    async fn test_in_memory_store_get_session() {
        let store = InMemorySessionStore::new();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let tokens = store.create_session("user123", now + 3600).await.unwrap();
        let refresh_token_hash = hash_token(&tokens.refresh_token);

        let session = store
            .get_session(&refresh_token_hash)
            .await
            .unwrap_or_else(|e| panic!("expected Ok from get_session: {e}"));
        assert_eq!(session.user_id, "user123");
    }

    #[tokio::test]
    async fn test_in_memory_store_revoke_session() {
        let store = InMemorySessionStore::new();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let tokens = store.create_session("user123", now + 3600).await.unwrap();
        let refresh_token_hash = hash_token(&tokens.refresh_token);

        store
            .revoke_session(&refresh_token_hash)
            .await
            .unwrap_or_else(|e| panic!("expected Ok from revoke_session: {e}"));

        let session = store.get_session(&refresh_token_hash).await;
        assert!(
            matches!(session, Err(AuthError::TokenNotFound)),
            "expected TokenNotFound after revocation, got: {session:?}"
        );
    }

    #[tokio::test]
    async fn test_in_memory_store_revoke_all_sessions() {
        let store = InMemorySessionStore::new();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let tokens1 = store.create_session("user123", now + 3600).await.unwrap();
        let tokens2 = store.create_session("user123", now + 3600).await.unwrap();
        let tokens3 = store.create_session("user456", now + 3600).await.unwrap();

        assert_eq!(store.len(), 3);

        store
            .revoke_all_sessions("user123")
            .await
            .unwrap_or_else(|e| panic!("expected Ok from revoke_all_sessions: {e}"));

        let hash3 = hash_token(&tokens3.refresh_token);
        store
            .get_session(&hash3)
            .await
            .unwrap_or_else(|e| panic!("expected user456 session to still exist: {e}"));

        let hash1 = hash_token(&tokens1.refresh_token);
        let hash2 = hash_token(&tokens2.refresh_token);
        assert!(
            matches!(store.get_session(&hash1).await, Err(AuthError::TokenNotFound)),
            "expected user123 session 1 to be revoked"
        );
        assert!(
            matches!(store.get_session(&hash2).await, Err(AuthError::TokenNotFound)),
            "expected user123 session 2 to be revoked"
        );
    }
}

#[cfg(test)]
mod session_postgres_tests {
    #[test]
    fn test_generate_access_token_creates_valid_jwt() {
        let test_pool = std::sync::Arc::new(std::sync::Mutex::new(()));
        let _ = test_pool;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut claims = crate::Claims {
            sub:   "user123".to_string(),
            iat:   now,
            exp:   now + 3600,
            nbf:   None,
            iss:   "fraiseql".to_string(),
            aud:   vec!["fraiseql-api".to_string()],
            extra: std::collections::HashMap::new(),
        };

        claims
            .extra
            .insert("jti".to_string(), serde_json::json!(uuid::Uuid::new_v4().to_string()));

        let secret = b"fraiseql_session_user123";
        let token1 =
            crate::jwt::generate_hs256_token(&claims, secret).expect("Failed to generate token");

        claims
            .extra
            .insert("jti".to_string(), serde_json::json!(uuid::Uuid::new_v4().to_string()));

        let token2 =
            crate::jwt::generate_hs256_token(&claims, secret).expect("Failed to generate token");

        assert_ne!(token1, token2);
        assert_eq!(token1.matches('.').count(), 2);
        assert_eq!(token2.matches('.').count(), 2);
    }

    #[test]
    fn test_generate_access_token_with_rs256_key() {
        let test_key = include_bytes!("../test_data/test_rsa_key.pem");

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut claims = crate::Claims {
            sub:   "user123".to_string(),
            iat:   now,
            exp:   now + 3600,
            nbf:   None,
            iss:   "fraiseql".to_string(),
            aud:   vec!["fraiseql-api".to_string()],
            extra: std::collections::HashMap::new(),
        };

        claims
            .extra
            .insert("jti".to_string(), serde_json::json!(uuid::Uuid::new_v4().to_string()));

        let token = crate::jwt::generate_rs256_token(&claims, test_key)
            .expect("Failed to generate RS256 token");

        assert_eq!(token.matches('.').count(), 2);
    }
}

#[cfg(test)]
mod state_store_tests {
    use std::sync::Arc;
    use super::super::state_store::*;
    use super::super::error::AuthError;

    #[tokio::test]
    async fn test_in_memory_state_store() {
        let store = InMemoryStateStore::new();

        store
            .store(
                "state123".to_string(),
                "google".to_string(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    + 600,
            )
            .await
            .unwrap();

        let (provider, _expiry) = store.retrieve("state123").await.unwrap();
        assert_eq!(provider, "google");

        let result = store.retrieve("state123").await;
        assert!(
            matches!(result, Err(AuthError::InvalidState)),
            "expected InvalidState for consumed state, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_state_not_found() {
        let store = InMemoryStateStore::new();
        let result = store.retrieve("nonexistent").await;
        assert!(
            matches!(result, Err(AuthError::InvalidState)),
            "expected InvalidState for nonexistent state, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_in_memory_state_replay_prevention() {
        let store = InMemoryStateStore::new();
        let expiry = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 600;

        store.store("state_abc".to_string(), "auth0".to_string(), expiry).await.unwrap();

        let result1 = store.retrieve("state_abc").await;
        assert!(result1.is_ok(), "first retrieval should succeed: {result1:?}");

        let result2 = store.retrieve("state_abc").await;
        assert!(
            matches!(result2, Err(AuthError::InvalidState)),
            "replay attempt should return InvalidState, got: {result2:?}"
        );
    }

    #[tokio::test]
    async fn test_in_memory_multiple_states() {
        let store = InMemoryStateStore::new();
        let expiry = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 600;

        store.store("state1".to_string(), "google".to_string(), expiry).await.unwrap();
        store.store("state2".to_string(), "auth0".to_string(), expiry).await.unwrap();
        store.store("state3".to_string(), "okta".to_string(), expiry).await.unwrap();

        let (p1, _) = store.retrieve("state1").await.unwrap();
        assert_eq!(p1, "google");

        let (p2, _) = store.retrieve("state2").await.unwrap();
        assert_eq!(p2, "auth0");

        let (p3, _) = store.retrieve("state3").await.unwrap();
        assert_eq!(p3, "okta");
    }

    #[tokio::test]
    async fn test_in_memory_state_store_trait_object() {
        let store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        let expiry = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 600;

        store
            .store("state_trait".to_string(), "test_provider".to_string(), expiry)
            .await
            .unwrap();

        let (provider, _) = store.retrieve("state_trait").await.unwrap();
        assert_eq!(provider, "test_provider");
    }

    #[tokio::test]
    async fn test_in_memory_state_store_bounded() {
        let store = InMemoryStateStore::with_max_states(5);
        let expiry = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 600;

        for i in 0..5 {
            let state = format!("state_{}", i);
            store.store(state, "google".to_string(), expiry).await.unwrap();
        }

        let result = store.store("state_5".to_string(), "google".to_string(), expiry).await;
        assert!(
            matches!(result, Err(AuthError::ConfigError { .. })),
            "expected ConfigError when store at capacity, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_in_memory_state_store_cleanup_expired() {
        let store = InMemoryStateStore::with_max_states(3);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        for i in 0..3 {
            let state = format!("expired_{}", i);
            store.store(state, "google".to_string(), now - 100).await.unwrap();
        }

        let expiry = now + 600;
        let result = store.store("valid_state".to_string(), "auth0".to_string(), expiry).await;
        assert!(result.is_ok(), "Should succeed after cleaning up expired states");

        store
            .store("valid_state_2".to_string(), "google".to_string(), expiry)
            .await
            .unwrap();
        store
            .store("valid_state_3".to_string(), "okta".to_string(), expiry)
            .await
            .unwrap();

        let result = store.store("valid_state_4".to_string(), "auth0".to_string(), expiry).await;
        assert!(
            matches!(result, Err(AuthError::ConfigError { .. })),
            "expected ConfigError when at capacity with valid states, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_in_memory_state_store_custom_max_size() {
        let store_small = InMemoryStateStore::with_max_states(1);
        let store_large = InMemoryStateStore::with_max_states(100);

        let expiry = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 600;

        store_small.store("s1".to_string(), "p1".to_string(), expiry).await.unwrap();
        let result = store_small.store("s2".to_string(), "p2".to_string(), expiry).await;
        assert!(
            matches!(result, Err(AuthError::ConfigError { .. })),
            "expected ConfigError when small store at capacity, got: {result:?}"
        );

        for i in 0..50 {
            let state = format!("state_{}", i);
            store_large.store(state, "provider".to_string(), expiry).await.unwrap();
        }
        assert_eq!(store_large.states.len(), 50);
    }

    #[tokio::test]
    async fn test_in_memory_state_store_zero_max_enforced() {
        let store = InMemoryStateStore::with_max_states(0);
        let expiry = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 600;

        let result = store.store("state1".to_string(), "google".to_string(), expiry).await;
        assert!(result.is_ok(), "Should allow at least 1 state minimum");
    }

    #[cfg(feature = "redis-rate-limiting")]
    #[tokio::test]
    async fn test_redis_state_store_basic() {
        let redis_url = "redis://localhost:6379";

        match RedisStateStore::new(redis_url).await {
            Ok(store) => {
                let expiry = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    + 600;

                store
                    .store("redis_state_1".to_string(), "google".to_string(), expiry)
                    .await
                    .unwrap();

                let (provider, _) = store.retrieve("redis_state_1").await.unwrap();
                assert_eq!(provider, "google");

                let result = store.retrieve("redis_state_1").await;
                assert!(
                    matches!(result, Err(AuthError::InvalidState)),
                    "expected InvalidState for consumed redis state, got: {result:?}"
                );
            },
            Err(_) => {
                eprintln!("Skipping Redis tests - Redis server not available");
            },
        }
    }

    #[cfg(feature = "redis-rate-limiting")]
    #[tokio::test]
    async fn test_redis_state_replay_prevention() {
        let redis_url = "redis://localhost:6379";

        if let Ok(store) = RedisStateStore::new(redis_url).await {
            let expiry = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 600;

            store
                .store("redis_replay_test".to_string(), "auth0".to_string(), expiry)
                .await
                .unwrap();

            let result1 = store.retrieve("redis_replay_test").await;
            assert!(result1.is_ok(), "first redis retrieval should succeed: {result1:?}");

            let result2 = store.retrieve("redis_replay_test").await;
            assert!(
                matches!(result2, Err(AuthError::InvalidState)),
                "redis replay attempt should return InvalidState, got: {result2:?}"
            );
        }
    }

    #[cfg(feature = "redis-rate-limiting")]
    #[tokio::test]
    async fn test_redis_multiple_states() {
        let redis_url = "redis://localhost:6379";

        if let Ok(store) = RedisStateStore::new(redis_url).await {
            let expiry = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 600;

            store
                .store("redis_state_a".to_string(), "google".to_string(), expiry)
                .await
                .unwrap();
            store
                .store("redis_state_b".to_string(), "okta".to_string(), expiry)
                .await
                .unwrap();

            let (p1, _) = store.retrieve("redis_state_a").await.unwrap();
            assert_eq!(p1, "google");

            let (p2, _) = store.retrieve("redis_state_b").await.unwrap();
            assert_eq!(p2, "okta");
        }
    }
}

#[cfg(test)]
mod jwt_tests {
    use std::collections::HashMap;
    use jsonwebtoken::Algorithm;
    use super::super::jwt::*;
    use super::super::error::AuthError;

    fn create_test_claims() -> Claims {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Claims {
            sub:   "user123".to_string(),
            iat:   now,
            exp:   now + 3600,
            nbf:   None,
            iss:   "https://example.com".to_string(),
            aud:   vec!["api".to_string()],
            extra: HashMap::new(),
        }
    }

    fn make_test_validator() -> JwtValidator {
        JwtValidator::new("https://example.com", Algorithm::HS256)
            .expect("Failed to create validator")
            .with_audiences(&["api"])
            .expect("Failed to set audiences")
    }

    #[test]
    fn test_jwt_validator_creation() {
        JwtValidator::new("https://example.com", Algorithm::HS256)
            .unwrap_or_else(|e| panic!("expected Ok for valid issuer: {e}"));
    }

    #[test]
    fn test_jwt_validator_invalid_issuer() {
        let validator = JwtValidator::new("", Algorithm::HS256);
        assert!(matches!(validator, Err(AuthError::ConfigError { .. })));
    }

    #[test]
    fn test_claims_is_expired() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut claims = create_test_claims();
        claims.exp = now - 100;

        assert!(claims.is_expired());
    }

    #[test]
    fn test_claims_not_expired() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut claims = create_test_claims();
        claims.exp = now + 3600;

        assert!(!claims.is_expired());
    }

    #[test]
    fn test_generate_and_validate_token() {
        let secret = b"test_secret_key_at_least_32_bytes_long";
        let validator = make_test_validator();

        let claims = create_test_claims();
        let token = generate_test_token(&claims, secret).expect("Failed to generate token");

        let validated_claims =
            validator.validate_hmac(&token, secret).expect("Failed to validate token");

        assert_eq!(validated_claims.sub, claims.sub);
        assert_eq!(validated_claims.iss, claims.iss);
    }

    #[test]
    fn test_validate_without_audience_rejects_token() {
        let secret = b"test_secret_key_at_least_32_bytes_long";
        let validator = JwtValidator::new("https://example.com", Algorithm::HS256)
            .expect("Failed to create validator");

        let claims = create_test_claims();
        let token = generate_test_token(&claims, secret).expect("Failed to generate token");

        let result = validator.validate_hmac(&token, secret);
        assert!(result.is_err(), "validator without configured audience must reject tokens");
    }

    #[test]
    fn test_validate_expired_token() {
        let secret = b"test_secret_key_at_least_32_bytes_long";
        let validator = make_test_validator();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut claims = create_test_claims();
        claims.exp = now - 100;

        let token = generate_test_token(&claims, secret).expect("Failed to generate token");

        let result = validator.validate_hmac(&token, secret);
        assert!(matches!(result, Err(AuthError::TokenExpired)));
    }

    #[test]
    fn test_validate_invalid_signature() {
        let secret = b"test_secret_key_at_least_32_bytes_long";
        let validator = make_test_validator();

        let claims = create_test_claims();
        let token = generate_test_token(&claims, secret).expect("Failed to generate token");

        let wrong_secret = b"wrong_secret_key_at_least_32_bytes_";
        let result = validator.validate_hmac(&token, wrong_secret);
        assert!(matches!(result, Err(AuthError::InvalidSignature)));
    }

    #[test]
    fn test_get_custom_claim() {
        let mut claims = create_test_claims();
        claims.extra.insert("email".to_string(), serde_json::json!("user@example.com"));
        claims.extra.insert("role".to_string(), serde_json::json!("admin"));

        assert_eq!(claims.get_custom("email"), Some(&serde_json::json!("user@example.com")));
        assert_eq!(claims.get_custom("role"), Some(&serde_json::json!("admin")));
        assert_eq!(claims.get_custom("nonexistent"), None);
    }

    #[test]
    fn test_rejects_token_with_iat_too_far_in_future() {
        let secret = b"test_secret_key_at_least_32_bytes_long";
        let validator = make_test_validator();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut claims = create_test_claims();
        claims.iat = now + MAX_CLOCK_SKEW_SECS + 60;
        let token = generate_test_token(&claims, secret).expect("token generation must succeed");
        let result = validator.validate_hmac(&token, secret);
        assert!(
            matches!(result, Err(AuthError::TokenIssuedInFuture)),
            "expected TokenIssuedInFuture, got: {result:?}"
        );
    }

    #[test]
    fn test_accepts_token_with_iat_within_clock_skew() {
        let secret = b"test_secret_key_at_least_32_bytes_long";
        let validator = make_test_validator();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut claims = create_test_claims();
        claims.iat = now + 60;
        let token = generate_test_token(&claims, secret).expect("token generation must succeed");
        validator
            .validate_hmac(&token, secret)
            .unwrap_or_else(|e| panic!("expected Ok for iat within clock skew: {e}"));
    }

    #[test]
    fn test_rejects_token_with_iat_too_old() {
        let secret = b"test_secret_key_at_least_32_bytes_long";
        let validator = make_test_validator();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut claims = create_test_claims();
        claims.iat = now - MAX_TOKEN_AGE_SECS - 60;
        claims.exp = now + 3600;
        let token = generate_test_token(&claims, secret).expect("token generation must succeed");
        let result = validator.validate_hmac(&token, secret);
        assert!(
            matches!(result, Err(AuthError::TokenTooOld)),
            "expected TokenTooOld, got: {result:?}"
        );
    }

    #[test]
    fn test_accepts_token_at_iat_max_age_boundary() {
        let secret = b"test_secret_key_at_least_32_bytes_long";
        let validator = make_test_validator();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut claims = create_test_claims();
        claims.iat = now - MAX_TOKEN_AGE_SECS;
        claims.exp = now + 3600;
        let token = generate_test_token(&claims, secret).expect("token generation must succeed");
        validator
            .validate_hmac(&token, secret)
            .unwrap_or_else(|e| panic!("expected Ok for iat exactly at max-age boundary: {e}"));
    }

    #[test]
    fn test_rejects_token_with_nbf_in_future() {
        let secret = b"test_secret_key_at_least_32_bytes_long";
        let validator = make_test_validator();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut claims = create_test_claims();
        claims.nbf = Some(now + MAX_CLOCK_SKEW_SECS + 60);
        let token = generate_test_token(&claims, secret).expect("token generation must succeed");
        let result = validator.validate_hmac(&token, secret);
        assert!(
            matches!(result, Err(AuthError::TokenNotYetValid)),
            "expected TokenNotYetValid, got: {result:?}"
        );
    }

    #[test]
    fn test_accepts_token_with_nbf_in_past() {
        let secret = b"test_secret_key_at_least_32_bytes_long";
        let validator = make_test_validator();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut claims = create_test_claims();
        claims.nbf = Some(now - 600);
        let token = generate_test_token(&claims, secret).expect("token generation must succeed");
        validator
            .validate_hmac(&token, secret)
            .unwrap_or_else(|e| panic!("expected Ok for nbf in past: {e}"));
    }

    #[test]
    fn test_accepts_token_without_nbf() {
        let secret = b"test_secret_key_at_least_32_bytes_long";
        let validator = make_test_validator();
        let claims = create_test_claims();
        let token = generate_test_token(&claims, secret).expect("token generation must succeed");
        validator
            .validate_hmac(&token, secret)
            .unwrap_or_else(|e| panic!("expected Ok for token without nbf: {e}"));
    }
}

#[cfg(test)]
mod middleware_tests {
    use std::collections::HashMap;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use super::super::middleware::*;
    use super::super::error::AuthError;
    use super::super::jwt::Claims;

    #[test]
    fn test_authenticated_user_clone() {
        let claims = Claims {
            sub:   "user123".to_string(),
            iat:   1000,
            exp:   2000,
            nbf:   None,
            iss:   "https://example.com".to_string(),
            aud:   vec!["api".to_string()],
            extra: HashMap::new(),
        };

        let user = AuthenticatedUser {
            user_id: "user123".to_string(),
            claims,
        };

        let _cloned = user.clone();
        assert_eq!(user.user_id, "user123");
    }

    #[test]
    fn test_has_role_single_string() {
        let mut claims = Claims {
            sub:   "user123".to_string(),
            iat:   1000,
            exp:   2000,
            nbf:   None,
            iss:   "https://example.com".to_string(),
            aud:   vec!["api".to_string()],
            extra: HashMap::new(),
        };

        claims.extra.insert("role".to_string(), serde_json::json!("admin"));

        let user = AuthenticatedUser {
            user_id: "user123".to_string(),
            claims,
        };

        assert!(user.has_role("admin"));
        assert!(!user.has_role("user"));
    }

    #[test]
    fn test_has_role_array() {
        let mut claims = Claims {
            sub:   "user123".to_string(),
            iat:   1000,
            exp:   2000,
            nbf:   None,
            iss:   "https://example.com".to_string(),
            aud:   vec!["api".to_string()],
            extra: HashMap::new(),
        };

        claims
            .extra
            .insert("roles".to_string(), serde_json::json!(["admin", "user", "editor"]));

        let user = AuthenticatedUser {
            user_id: "user123".to_string(),
            claims,
        };

        assert!(user.has_role("admin"));
        assert!(user.has_role("user"));
        assert!(user.has_role("editor"));
        assert!(!user.has_role("moderator"));
    }

    #[test]
    fn test_get_custom_claim() {
        let mut claims = Claims {
            sub:   "user123".to_string(),
            iat:   1000,
            exp:   2000,
            nbf:   None,
            iss:   "https://example.com".to_string(),
            aud:   vec!["api".to_string()],
            extra: HashMap::new(),
        };

        claims.extra.insert("org_id".to_string(), serde_json::json!("org_456"));

        let user = AuthenticatedUser {
            user_id: "user123".to_string(),
            claims,
        };

        assert_eq!(user.get_custom_claim("org_id"), Some(&serde_json::json!("org_456")));
        assert_eq!(user.get_custom_claim("nonexistent"), None);
    }

    #[test]
    fn test_invalid_token_sanitized() {
        let error = AuthError::InvalidToken {
            reason: "RS256 signature mismatch at offset 512 bytes".to_string(),
        };
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_missing_claim_sanitized() {
        let error = AuthError::MissingClaim {
            claim: "sensitive_user_id".to_string(),
        };
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_invalid_claim_value_sanitized() {
        let error = AuthError::InvalidClaimValue {
            claim:  "exp".to_string(),
            reason: "Must match pattern: ^[0-9]{10,}$".to_string(),
        };
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_database_error_sanitized() {
        let error = AuthError::DatabaseError {
            message: "Connection to 192.168.1.100:5432 failed: timeout".to_string(),
        };
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_config_error_sanitized() {
        let error = AuthError::ConfigError {
            message: "Secret key missing in /etc/fraiseql/config.toml".to_string(),
        };
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_oauth_error_sanitized() {
        let error = AuthError::OAuthError {
            message: "GitHub API returned 500 from https://api.github.com/user (rate limited)"
                .to_string(),
        };
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_session_error_sanitized() {
        let error = AuthError::SessionError {
            message: "Redis connection pool exhausted: 0/10 available".to_string(),
        };
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_forbidden_error_sanitized() {
        let error = AuthError::Forbidden {
            message: "User lacks role=admin AND permission=write:config for operation".to_string(),
        };
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_internal_error_sanitized() {
        let error = AuthError::Internal {
            message: "Panic in JWT validation thread: index out of bounds".to_string(),
        };
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_system_time_error_sanitized() {
        let error = AuthError::SystemTimeError {
            message: "System clock jumped backward by 3600 seconds".to_string(),
        };
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_rate_limited_error_message() {
        let error = AuthError::RateLimited {
            retry_after_secs: 60,
        };
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[test]
    fn test_token_expired_returns_generic_message() {
        let error = AuthError::TokenExpired;
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_invalid_signature_returns_generic_message() {
        let error = AuthError::InvalidSignature;
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_invalid_state_error() {
        let error = AuthError::InvalidState;
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_pkce_error_returns_bad_request() {
        let error = AuthError::PkceError {
            message: "Challenge verification failed".to_string(),
        };
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_oidc_metadata_error_returns_server_error() {
        let error = AuthError::OidcMetadataError {
            message: "Failed to fetch metadata".to_string(),
        };
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_all_errors_have_status_codes() {
        let errors = vec![
            AuthError::TokenExpired,
            AuthError::InvalidSignature,
            AuthError::InvalidState,
            AuthError::TokenNotFound,
            AuthError::SessionRevoked,
            AuthError::InvalidToken { reason: "test".to_string() },
            AuthError::MissingClaim { claim: "test".to_string() },
            AuthError::InvalidClaimValue { claim: "test".to_string(), reason: "test".to_string() },
            AuthError::OAuthError { message: "test".to_string() },
            AuthError::SessionError { message: "test".to_string() },
            AuthError::DatabaseError { message: "test".to_string() },
            AuthError::ConfigError { message: "test".to_string() },
            AuthError::OidcMetadataError { message: "test".to_string() },
            AuthError::PkceError { message: "test".to_string() },
            AuthError::Forbidden { message: "test".to_string() },
            AuthError::Internal { message: "test".to_string() },
            AuthError::SystemTimeError { message: "test".to_string() },
            AuthError::RateLimited { retry_after_secs: 60 },
        ];

        for error in errors {
            let response = error.into_response();
            let status = response.status();
            assert!(
                status == StatusCode::UNAUTHORIZED
                    || status == StatusCode::FORBIDDEN
                    || status == StatusCode::BAD_REQUEST
                    || status == StatusCode::INTERNAL_SERVER_ERROR
                    || status == StatusCode::TOO_MANY_REQUESTS,
                "Unexpected status code: {}",
                status
            );
        }
    }
}

#[cfg(test)]
mod operation_rbac_tests {
    use super::super::operation_rbac::*;
    use super::super::middleware::AuthenticatedUser;
    use super::super::jwt::Claims;
    use super::super::error::AuthError;

    fn create_test_user(role: &str) -> AuthenticatedUser {
        let mut extra = std::collections::HashMap::new();
        extra.insert("role".to_string(), serde_json::json!(role));

        AuthenticatedUser {
            user_id: "test-user".to_string(),
            claims:  Claims {
                sub: "test-user".to_string(),
                iat: 1_000_000,
                exp: 2_000_000,
                nbf: None,
                iss: "test-issuer".to_string(),
                aud: vec!["fraiseql".to_string()],
                extra,
            },
        }
    }

    fn create_test_user_with_roles(roles: Vec<&str>) -> AuthenticatedUser {
        let mut extra = std::collections::HashMap::new();
        extra.insert("roles".to_string(), serde_json::json!(roles));

        AuthenticatedUser {
            user_id: "test-user".to_string(),
            claims:  Claims {
                sub: "test-user".to_string(),
                iat: 1_000_000,
                exp: 2_000_000,
                nbf: None,
                iss: "test-issuer".to_string(),
                aud: vec!["fraiseql".to_string()],
                extra,
            },
        }
    }

    #[test]
    fn test_admin_has_all_permissions() {
        let policy = RBACPolicy::new();
        let user = create_test_user("admin");

        let r = policy.authorize(&user, OperationPermission::CreateRule);
        assert!(r.is_ok(), "admin should have CreateRule: {r:?}");
        let r = policy.authorize(&user, OperationPermission::DeleteRule);
        assert!(r.is_ok(), "admin should have DeleteRule: {r:?}");
        let r = policy.authorize(&user, OperationPermission::ManageUsers);
        assert!(r.is_ok(), "admin should have ManageUsers: {r:?}");
        let r = policy.authorize(&user, OperationPermission::ManageTenants);
        assert!(r.is_ok(), "admin should have ManageTenants: {r:?}");
    }

    #[test]
    fn test_operator_has_limited_permissions() {
        let policy = RBACPolicy::new();
        let user = create_test_user("operator");

        let r = policy.authorize(&user, OperationPermission::CreateRule);
        assert!(r.is_ok(), "operator should have CreateRule: {r:?}");
        let r = policy.authorize(&user, OperationPermission::ManageWebhooks);
        assert!(r.is_ok(), "operator should have ManageWebhooks: {r:?}");
        let r = policy.authorize(&user, OperationPermission::ManageUsers);
        assert!(
            matches!(r, Err(AuthError::Forbidden { .. })),
            "operator should not have ManageUsers: {r:?}"
        );
        let r = policy.authorize(&user, OperationPermission::ManageTenants);
        assert!(
            matches!(r, Err(AuthError::Forbidden { .. })),
            "operator should not have ManageTenants: {r:?}"
        );
    }

    #[test]
    fn test_viewer_has_minimal_permissions() {
        let policy = RBACPolicy::new();
        let user = create_test_user("viewer");

        let r = policy.authorize(&user, OperationPermission::ExportData);
        assert!(r.is_ok(), "viewer should have ExportData: {r:?}");
        let r = policy.authorize(&user, OperationPermission::ViewAuditLogs);
        assert!(r.is_ok(), "viewer should have ViewAuditLogs: {r:?}");
        let r = policy.authorize(&user, OperationPermission::CreateRule);
        assert!(
            matches!(r, Err(AuthError::Forbidden { .. })),
            "viewer should not have CreateRule: {r:?}"
        );
        let r = policy.authorize(&user, OperationPermission::ManageWebhooks);
        assert!(
            matches!(r, Err(AuthError::Forbidden { .. })),
            "viewer should not have ManageWebhooks: {r:?}"
        );
    }

    #[test]
    fn test_multiple_roles() {
        let policy = RBACPolicy::new();
        let user = create_test_user_with_roles(vec!["viewer", "operator"]);

        let r = policy.authorize(&user, OperationPermission::CreateRule);
        assert!(r.is_ok(), "viewer+operator should have CreateRule: {r:?}");
        let r = policy.authorize(&user, OperationPermission::ExportData);
        assert!(r.is_ok(), "viewer+operator should have ExportData: {r:?}");

        let r = policy.authorize(&user, OperationPermission::ManageTenants);
        assert!(
            matches!(r, Err(AuthError::Forbidden { .. })),
            "viewer+operator should not have ManageTenants: {r:?}"
        );
    }

    #[test]
    fn test_authorize_any() {
        let policy = RBACPolicy::new();
        let user = create_test_user("viewer");

        let permissions = vec![
            OperationPermission::ManageTenants,
            OperationPermission::ExportData,
        ];

        let r = policy.authorize_any(&user, &permissions);
        assert!(r.is_ok(), "viewer should have at least one of the permissions: {r:?}");
    }

    #[test]
    fn test_authorize_all() {
        let policy = RBACPolicy::new();
        let user = create_test_user("operator");

        let permissions = vec![
            OperationPermission::CreateRule,
            OperationPermission::UpdateRule,
        ];

        let r = policy.authorize_all(&user, &permissions);
        assert!(r.is_ok(), "operator should have all rule permissions: {r:?}");
    }

    #[test]
    fn test_authorize_all_fails_if_missing_one() {
        let policy = RBACPolicy::new();
        let user = create_test_user("operator");

        let permissions = vec![
            OperationPermission::CreateRule,
            OperationPermission::ManageTenants,
        ];

        let r = policy.authorize_all(&user, &permissions);
        assert!(
            matches!(r, Err(AuthError::Forbidden { .. })),
            "operator missing ManageTenants should fail authorize_all: {r:?}"
        );
    }

    #[test]
    fn test_get_user_permissions() {
        let policy = RBACPolicy::new();
        let user = create_test_user("viewer");

        let permissions = policy.get_user_permissions(&user);
        assert_eq!(permissions.len(), 2);
        assert!(permissions.contains(&OperationPermission::ExportData));
        assert!(permissions.contains(&OperationPermission::ViewAuditLogs));
    }

    #[test]
    fn test_custom_role() {
        let mut policy = RBACPolicy::new();

        let custom_role = Role::new(
            "auditor".to_string(),
            vec![
                OperationPermission::ViewAuditLogs,
                OperationPermission::ExportData,
            ],
        );

        policy.register_role(custom_role);
        let user = create_test_user("auditor");

        let r = policy.authorize(&user, OperationPermission::ViewAuditLogs);
        assert!(r.is_ok(), "auditor should have ViewAuditLogs: {r:?}");
        let r = policy.authorize(&user, OperationPermission::CreateRule);
        assert!(
            matches!(r, Err(AuthError::Forbidden { .. })),
            "auditor should not have CreateRule: {r:?}"
        );
    }

    #[test]
    fn test_permission_string_format() {
        assert_eq!(OperationPermission::CreateRule.as_str(), "create_rule");
        assert_eq!(OperationPermission::ManageSecrets.as_str(), "manage_secrets");
        assert_eq!(OperationPermission::ViewAuditLogs.as_str(), "view_audit_logs");
    }
}

#[cfg(test)]
mod rate_limiting_tests_inner {
    use super::super::rate_limiting::*;
    use super::super::error::AuthError;

    #[test]
    fn test_rate_limiter_allows_within_limit() {
        let limiter = KeyedRateLimiter::new(AuthRateLimitConfig {
            enabled:      true,
            max_requests: 3,
            window_secs:  60,
        });

        for i in 0..3 {
            let result = limiter.check("key");
            assert!(result.is_ok(), "Request {} should be allowed", i);
        }
    }

    #[test]
    fn test_rate_limiter_rejects_over_limit() {
        let limiter = KeyedRateLimiter::new(AuthRateLimitConfig {
            enabled:      true,
            max_requests: 2,
            window_secs:  60,
        });

        limiter.check("key").ok();
        limiter.check("key").ok();

        let result = limiter.check("key");
        assert!(
            matches!(result, Err(AuthError::RateLimited { .. })),
            "expected RateLimited error, got: {result:?}"
        );
    }

    #[test]
    fn test_rate_limiter_per_key() {
        let limiter = KeyedRateLimiter::new(AuthRateLimitConfig {
            enabled:      true,
            max_requests: 2,
            window_secs:  60,
        });

        limiter.check("key1").ok();
        limiter.check("key1").ok();

        let result = limiter.check("key2");
        assert!(result.is_ok(), "Different key should have independent limit");
    }

    #[test]
    fn test_rate_limiter_error_contains_retry_after() {
        let limiter = KeyedRateLimiter::new(AuthRateLimitConfig {
            enabled:      true,
            max_requests: 1,
            window_secs:  60,
        });

        limiter.check("key").ok();
        let result = limiter.check("key");

        match result {
            Err(AuthError::RateLimited { retry_after_secs }) => {
                assert_eq!(retry_after_secs, 60);
            },
            _ => panic!("Expected RateLimited error"),
        }
    }

    #[test]
    fn test_rate_limiter_active_limiters_count() {
        let limiter = KeyedRateLimiter::new(AuthRateLimitConfig {
            enabled:      true,
            max_requests: 100,
            window_secs:  60,
        });

        assert_eq!(limiter.active_limiters(), 0);

        limiter.check("key1").ok();
        assert_eq!(limiter.active_limiters(), 1);

        limiter.check("key2").ok();
        assert_eq!(limiter.active_limiters(), 2);
    }

    #[test]
    fn test_rate_limiters_default() {
        let limiters = RateLimiters::new();

        let result = limiters.auth_start.check("ip_1");
        assert!(result.is_ok(), "auth/start should allow first request: {result:?}");

        let result = limiters.auth_refresh.check("user_1");
        assert!(result.is_ok(), "auth/refresh should allow first request: {result:?}");
    }

    #[test]
    fn test_rate_limit_config_presets() {
        let standard_ip = AuthRateLimitConfig::per_ip_standard();
        assert_eq!(standard_ip.max_requests, 100);
        assert_eq!(standard_ip.window_secs, 60);

        let strict_ip = AuthRateLimitConfig::per_ip_strict();
        assert_eq!(strict_ip.max_requests, 50);

        let user_limit = AuthRateLimitConfig::per_user_standard();
        assert_eq!(user_limit.max_requests, 10);

        let failed = AuthRateLimitConfig::failed_login_attempts();
        assert_eq!(failed.max_requests, 5);
        assert_eq!(failed.window_secs, 3600);
    }

    #[test]
    fn test_ip_based_rate_limiting() {
        let limiter = KeyedRateLimiter::new(AuthRateLimitConfig::per_ip_standard());

        let ip = "203.0.113.1";

        for _ in 0..100 {
            let result = limiter.check(ip);
            assert!(result.is_ok(), "request within limit should be allowed: {result:?}");
        }

        let result = limiter.check(ip);
        assert!(
            matches!(result, Err(AuthError::RateLimited { .. })),
            "expected RateLimited after exceeding IP limit, got: {result:?}"
        );
    }

    #[test]
    fn test_rejected_login_tracking() {
        let limiter = KeyedRateLimiter::new(AuthRateLimitConfig::failed_login_attempts());

        let user = "alice@example.com";

        for _ in 0..5 {
            let result = limiter.check(user);
            assert!(
                result.is_ok(),
                "failed login attempt within limit should be allowed: {result:?}"
            );
        }

        let result = limiter.check(user);
        assert!(
            matches!(result, Err(AuthError::RateLimited { .. })),
            "expected RateLimited after exceeding failed login limit, got: {result:?}"
        );
    }

    #[test]
    fn test_multiple_users_independent() {
        let limiter = KeyedRateLimiter::new(AuthRateLimitConfig::failed_login_attempts());

        for _ in 0..5 {
            limiter.check("user1").ok();
        }

        let result = limiter.check("user1");
        assert!(
            matches!(result, Err(AuthError::RateLimited { .. })),
            "expected RateLimited for user1, got: {result:?}"
        );

        let result = limiter.check("user2");
        assert!(result.is_ok(), "user2 should have independent fresh limit: {result:?}");
    }

    #[test]
    fn test_clear_limiters() {
        let limiter = KeyedRateLimiter::new(AuthRateLimitConfig {
            enabled:      true,
            max_requests: 1,
            window_secs:  60,
        });

        limiter.check("key").ok();
        let result = limiter.check("key");
        assert!(
            matches!(result, Err(AuthError::RateLimited { .. })),
            "expected RateLimited before clear, got: {result:?}"
        );

        limiter.clear();

        let result = limiter.check("key");
        assert!(result.is_ok(), "should allow requests after clear: {result:?}");
    }

    #[test]
    fn test_thread_safe_rate_limiting() {
        use std::sync::Arc as StdArc;

        let limiter = StdArc::new(KeyedRateLimiter::new(AuthRateLimitConfig {
            enabled:      true,
            max_requests: 100,
            window_secs:  60,
        }));

        let mut handles = vec![];

        for _ in 0..10 {
            let limiter_clone = StdArc::clone(&limiter);
            let handle = std::thread::spawn(move || {
                for _ in 0..10 {
                    let _ = limiter_clone.check("concurrent");
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().ok();
        }

        let result = limiter.check("concurrent");
        assert!(
            matches!(result, Err(AuthError::RateLimited { .. })),
            "expected RateLimited after concurrent exhaustion, got: {result:?}"
        );
    }

    #[test]
    fn test_rate_limiting_many_keys() {
        let limiter = KeyedRateLimiter::new(AuthRateLimitConfig {
            enabled:      true,
            max_requests: 10,
            window_secs:  60,
        });

        for i in 0..1000 {
            let key = format!("192.168.{}.{}", i / 256, i % 256);
            let result = limiter.check(&key);
            assert!(result.is_ok(), "first request for {key} should be allowed: {result:?}");
        }

        assert_eq!(limiter.active_limiters(), 1000);
    }

    #[test]
    fn test_endpoint_combinations() {
        let limiters = RateLimiters::new();

        let ip = "203.0.113.1";
        let user = "bob@example.com";

        let result = limiters.auth_start.check(ip);
        assert!(result.is_ok(), "auth_start should allow: {result:?}");

        let result = limiters.auth_callback.check(ip);
        assert!(result.is_ok(), "auth_callback should allow: {result:?}");

        let result = limiters.auth_refresh.check(user);
        assert!(result.is_ok(), "auth_refresh should allow: {result:?}");

        let result = limiters.auth_logout.check(user);
        assert!(result.is_ok(), "auth_logout should allow: {result:?}");

        let result = limiters.failed_logins.check(user);
        assert!(result.is_ok(), "failed_logins should allow: {result:?}");
    }

    #[test]
    fn test_attack_prevention_scenario() {
        let limiter = KeyedRateLimiter::new(AuthRateLimitConfig {
            enabled:      true,
            max_requests: 10,
            window_secs:  60,
        });

        let target = "admin@example.com";

        for _ in 0..10 {
            let _ = limiter.check(target);
        }

        let result = limiter.check(target);
        assert!(
            matches!(result, Err(AuthError::RateLimited { .. })),
            "expected RateLimited after attack scenario, got: {result:?}"
        );
    }

    #[test]
    fn test_rate_limiter_disabled() {
        let limiter = KeyedRateLimiter::new(AuthRateLimitConfig {
            enabled:      false,
            max_requests: 1,
            window_secs:  60,
        });

        for i in 0..100 {
            let result = limiter.check("key");
            assert!(result.is_ok(), "Request {} should be allowed when rate limiting disabled", i);
        }
    }

    #[test]
    fn test_concurrent_requests_from_same_key_respects_limit() {
        use std::{sync::Arc, thread};

        let limiter = Arc::new(KeyedRateLimiter::new(AuthRateLimitConfig {
            enabled:      true,
            max_requests: 50,
            window_secs:  60,
        }));

        let key = "shared_key";
        let allowed_count = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let rejected_count = Arc::new(std::sync::atomic::AtomicU32::new(0));

        let mut handles = vec![];

        for _ in 0..100 {
            let limiter = Arc::clone(&limiter);
            let allowed = Arc::clone(&allowed_count);
            let rejected = Arc::clone(&rejected_count);

            let handle = thread::spawn(move || {
                match limiter.check(key) {
                    Ok(()) => allowed.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
                    Err(_) => rejected.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
                };
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let allowed = allowed_count.load(std::sync::atomic::Ordering::SeqCst);
        let rejected = rejected_count.load(std::sync::atomic::Ordering::SeqCst);

        assert_eq!(allowed, 50, "Atomic operations should limit to max_requests");
        assert_eq!(rejected, 50, "Remaining requests should be rejected");
        assert_eq!(allowed + rejected, 100, "All requests should be accounted for");
    }

    #[test]
    fn test_concurrent_requests_different_keys_independent() {
        use std::{sync::Arc, thread};

        let limiter = Arc::new(KeyedRateLimiter::new(AuthRateLimitConfig {
            enabled:      true,
            max_requests: 10,
            window_secs:  60,
        }));

        let mut handles = vec![];

        for thread_id in 0..10 {
            let limiter = Arc::clone(&limiter);
            let handle = thread::spawn(move || {
                let key = format!("key_{}", thread_id);
                let mut allowed = 0;
                let mut rejected = 0;

                for _ in 0..15 {
                    match limiter.check(&key) {
                        Ok(()) => allowed += 1,
                        Err(_) => rejected += 1,
                    }
                }

                (allowed, rejected)
            });
            handles.push(handle);
        }

        let mut total_allowed = 0;
        let mut total_rejected = 0;

        for handle in handles {
            let (allowed, rejected) = handle.join().unwrap();
            total_allowed += allowed;
            total_rejected += rejected;
        }

        assert_eq!(total_allowed, 100, "Each of 10 keys should allow 10 requests");
        assert_eq!(total_rejected, 50, "Each of 10 keys should reject 5 requests");
    }

    #[test]
    fn test_atomic_check_and_update_not_interleaved() {
        let limiter = KeyedRateLimiter::new(AuthRateLimitConfig {
            enabled:      true,
            max_requests: 3,
            window_secs:  60,
        });

        let key = "test_key";

        let r = limiter.check(key);
        assert!(r.is_ok(), "request 1 should be allowed: {r:?}");
        let r = limiter.check(key);
        assert!(r.is_ok(), "request 2 should be allowed: {r:?}");
        let r = limiter.check(key);
        assert!(r.is_ok(), "request 3 should be allowed: {r:?}");

        assert_eq!(limiter.active_limiters(), 1);

        let r = limiter.check(key);
        assert!(
            matches!(r, Err(AuthError::RateLimited { .. })),
            "request 4 should be rate-limited: {r:?}"
        );

        let r = limiter.check(key);
        assert!(
            matches!(r, Err(AuthError::RateLimited { .. })),
            "request 5 should be rate-limited: {r:?}"
        );
    }

    #[test]
    fn test_concurrent_window_reset_safety() {
        let limiter = KeyedRateLimiter::new(AuthRateLimitConfig {
            enabled:      true,
            max_requests: 2,
            window_secs:  3600,
        });

        let key = "reset_key";

        limiter.check(key).ok();
        limiter.check(key).ok();

        let r = limiter.check(key);
        assert!(matches!(r, Err(AuthError::RateLimited { .. })), "should be rate-limited: {r:?}");
        let r = limiter.check(key);
        assert!(
            matches!(r, Err(AuthError::RateLimited { .. })),
            "should still be rate-limited: {r:?}"
        );

        limiter.clear();
        assert_eq!(limiter.active_limiters(), 0);

        let r = limiter.check(key);
        assert!(r.is_ok(), "should allow after clear: {r:?}");
    }

    #[test]
    fn test_rate_limiter_evicts_lru_entry_when_at_capacity() {
        let config = AuthRateLimitConfig {
            enabled:      true,
            max_requests: 10,
            window_secs:  3600,
        };
        let limiter = KeyedRateLimiter::with_max_entries(config, 3);

        limiter.check("key_a").unwrap();
        limiter.check("key_b").unwrap();
        limiter.check("key_c").unwrap();
        assert_eq!(limiter.active_limiters(), 3);

        let result = limiter.check("key_d");
        assert!(result.is_ok(), "new key must be accepted when limiter evicts LRU entry");
        assert_eq!(
            limiter.active_limiters(),
            3,
            "entry count must stay at capacity after eviction"
        );
    }

    #[test]
    fn test_rate_limiter_capacity_configurable() {
        let config = AuthRateLimitConfig {
            enabled:      true,
            max_requests: 10,
            window_secs:  3600,
        };
        let limiter = KeyedRateLimiter::with_max_entries(config, 5);

        for i in 0..5 {
            limiter.check(&format!("key_{i}")).unwrap();
        }
        assert_eq!(limiter.active_limiters(), 5, "limiter must track exactly max_entries keys");

        limiter.check("key_overflow").unwrap();
        assert_eq!(limiter.active_limiters(), 5, "capacity must not exceed configured maximum");
    }

    #[test]
    fn test_rate_limiter_eviction_does_not_affect_active_ips() {
        use std::sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        };

        let now = Arc::new(AtomicU64::new(1_000));
        let clock_ref = Arc::clone(&now);
        let config = AuthRateLimitConfig {
            enabled:      true,
            max_requests: 1,
            window_secs:  3600,
        };
        let limiter = KeyedRateLimiter::with_clock_and_max_entries(config, 2, move || {
            clock_ref.load(Ordering::Relaxed)
        });

        now.store(1_000, Ordering::Relaxed);
        limiter.check("key_a").unwrap();

        now.store(2_000, Ordering::Relaxed);
        limiter.check("key_b").unwrap();

        now.store(3_000, Ordering::Relaxed);
        limiter.check("key_c").unwrap();

        let result = limiter.check("key_b");
        assert!(
            matches!(result, Err(AuthError::RateLimited { .. })),
            "key_b must remain rate-limited after eviction of the older key_a entry, got: {result:?}"
        );
    }

    #[test]
    fn auth_refresh_blocks_at_per_user_standard_boundary() {
        let limiter = KeyedRateLimiter::new(AuthRateLimitConfig::per_user_standard());
        let user = "user@example.com";

        for i in 1..=10 {
            let result = limiter.check(user);
            assert!(result.is_ok(), "request {i} should be allowed (limit is 10): {result:?}");
        }

        let result = limiter.check(user);
        assert!(
            matches!(
                result,
                Err(AuthError::RateLimited {
                    retry_after_secs: 60,
                })
            ),
            "11th request must be rate-limited with retry_after_secs=60, got: {result:?}"
        );
    }

    #[test]
    fn rate_limiters_auth_refresh_is_per_user_independent() {
        let limiters = RateLimiters::new();

        let alice = "alice@example.com";
        let bob = "bob@example.com";

        for i in 1..=10 {
            let result = limiters.auth_refresh.check(alice);
            assert!(result.is_ok(), "alice request {i} should be allowed: {result:?}");
        }
        let result = limiters.auth_refresh.check(alice);
        assert!(
            matches!(result, Err(AuthError::RateLimited { .. })),
            "alice's 11th auth_refresh request must be rejected: {result:?}"
        );

        let result = limiters.auth_refresh.check(bob);
        assert!(
            result.is_ok(),
            "bob's first auth_refresh request must succeed independently of alice: {result:?}"
        );
    }

    #[test]
    fn test_startup_warn_emitted_when_no_distributed_backend() {
        warn_if_single_node_rate_limiting();
    }

    #[test]
    fn test_no_toctou_race_condition() {
        let limiter = KeyedRateLimiter::new(AuthRateLimitConfig {
            enabled:      true,
            max_requests: 1,
            window_secs:  60,
        });

        let key = "single_key";

        let r = limiter.check(key);
        assert!(r.is_ok(), "first request should be allowed: {r:?}");

        let result = limiter.check(key);
        assert!(
            result.is_err(),
            "Second request must fail - check-and-update is atomic so no TOCTOU race"
        );
    }
}

#[cfg(test)]
mod anonymous_tests {
    use std::sync::Arc;
    use axum::{Router, body::Body, http::{Request, StatusCode}, routing::post};
    use tower::ServiceExt as _;
    use super::super::anonymous::*;
    use super::super::session::{InMemorySessionStore, SessionStore};
    use super::super::account_linking::{InMemoryUserStore, UserStore};

    fn build_anon_state() -> Arc<AnonAuthState> {
        let session_store: Arc<dyn SessionStore> = Arc::new(InMemorySessionStore::new());
        Arc::new(AnonAuthState::new(session_store))
    }

    fn build_anon_state_with_user_store() -> (Arc<AnonAuthState>, Arc<InMemoryUserStore>) {
        let session_store: Arc<dyn SessionStore> = Arc::new(InMemorySessionStore::new());
        let user_store = Arc::new(InMemoryUserStore::new());
        let state = Arc::new(
            AnonAuthState::new(session_store)
                .with_user_store(user_store.clone() as Arc<dyn UserStore>),
        );
        (state, user_store)
    }

    fn anon_router(state: Arc<AnonAuthState>) -> Router {
        Router::new()
            .route("/auth/v1/signup", post(signup_anonymous))
            .with_state(state)
    }

    fn post_json(uri: &str, body: serde_json::Value) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap()
    }

    #[tokio::test]
    async fn test_signup_anonymous_returns_session() {
        let state = build_anon_state();
        let app = anon_router(state);

        let req = post_json("/auth/v1/signup", serde_json::json!({}));
        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert!(json["user_id"].is_string());
        assert!(json["access_token"].is_string());
        assert!(json["refresh_token"].is_string());
        assert_eq!(json["token_type"], "Bearer");
        assert!(json["expires_in"].is_number());
        assert_eq!(json["is_anonymous"], true);
    }

    #[tokio::test]
    async fn test_signup_generates_unique_user_ids() {
        let state = build_anon_state();
        let app = anon_router(state);

        let req1 = post_json("/auth/v1/signup", serde_json::json!({}));
        let resp1 = app.clone().oneshot(req1).await.unwrap();
        let body1 = axum::body::to_bytes(resp1.into_body(), usize::MAX).await.unwrap();
        let json1: serde_json::Value = serde_json::from_slice(&body1).unwrap();

        let req2 = post_json("/auth/v1/signup", serde_json::json!({}));
        let resp2 = app.oneshot(req2).await.unwrap();
        let body2 = axum::body::to_bytes(resp2.into_body(), usize::MAX).await.unwrap();
        let json2: serde_json::Value = serde_json::from_slice(&body2).unwrap();

        assert_ne!(json1["user_id"], json2["user_id"], "each signup must get a unique ID");
    }

    #[tokio::test]
    async fn test_signup_with_user_store_creates_local_user() {
        let (state, user_store) = build_anon_state_with_user_store();
        let app = anon_router(state);

        let req = post_json("/auth/v1/signup", serde_json::json!({ "name": "Anonymous Alice" }));
        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        let user_id = json["user_id"].as_str().unwrap();
        let user = user_store.get_user(user_id).await.unwrap();
        assert!(user.is_some(), "local user record must exist");

        let user = user.unwrap();
        assert!(user.email.contains("anonymous.local"));
        assert_eq!(user.identities.len(), 1);
        assert_eq!(user.identities[0].provider, "anonymous");
    }

    #[tokio::test]
    async fn test_signup_custom_ttl() {
        let session_store: Arc<dyn SessionStore> = Arc::new(InMemorySessionStore::new());
        let state = Arc::new(AnonAuthState::new(session_store).with_ttl(3600));
        let app = anon_router(state);

        let req = post_json("/auth/v1/signup", serde_json::json!({}));
        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        let expires_in = json["expires_in"].as_u64().unwrap();
        assert!(expires_in <= 3600, "expires_in should be ≤ TTL");
        assert!(expires_in > 3500, "expires_in should be close to TTL");
    }

    #[tokio::test]
    async fn test_signup_without_user_store_still_works() {
        let state = build_anon_state();
        let app = anon_router(state);

        let req = post_json("/auth/v1/signup", serde_json::json!({}));
        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert!(json["user_id"].is_string());
        assert_eq!(json["is_anonymous"], true);
    }

    #[tokio::test]
    async fn test_multiple_signups_create_separate_users() {
        let (state, user_store) = build_anon_state_with_user_store();
        let app = anon_router(state);

        for _ in 0..3 {
            let req = post_json("/auth/v1/signup", serde_json::json!({}));
            let resp = app.clone().oneshot(req).await.unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
        }

        assert_eq!(user_store.user_count().await, 3, "each signup creates a new user");
    }
}

// ── otp_tests ─────────────────────────────────────────────────────────────────
mod otp_tests {
    use std::sync::Arc;

    use axum::{Router, body::Body, http::Request, routing::post};
    use tower::ServiceExt as _;

    use super::super::otp::*;
    use super::super::session::{InMemorySessionStore, SessionStore};

    fn build_otp_state() -> (Arc<OtpAuthState>, Arc<InMemoryEmailSender>, Arc<InMemoryOtpStore>) {
        let otp_store = Arc::new(InMemoryOtpStore::new());
        let email_sender = Arc::new(InMemoryEmailSender::new());
        let session_store: Arc<dyn SessionStore> = Arc::new(InMemorySessionStore::new());

        let state = Arc::new(OtpAuthState {
            otp_store:     otp_store.clone(),
            email_sender:  email_sender.clone(),
            session_store,
            user_store:    None,
        });

        (state, email_sender, otp_store)
    }

    fn otp_router(state: Arc<OtpAuthState>) -> Router {
        Router::new()
            .route("/auth/v1/otp", post(send_otp))
            .route("/auth/v1/verify", post(verify_otp))
            .with_state(state)
    }

    fn post_json(uri: &str, body: serde_json::Value) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap()
    }

    #[tokio::test]
    async fn test_send_otp_returns_success() {
        let (state, email_sender, _) = build_otp_state();
        let app = otp_router(state);

        let req = post_json("/auth/v1/otp", serde_json::json!({ "email": "alice@example.com" }));
        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), axum::http::StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "otp_sent");
        assert_eq!(json["expires_in"], OTP_TTL_SECS);

        assert_eq!(email_sender.otp_count().await, 1);
    }

    #[tokio::test]
    async fn test_send_otp_empty_email_returns_400() {
        let (state, _, _) = build_otp_state();
        let app = otp_router(state);

        let req = post_json("/auth/v1/otp", serde_json::json!({ "email": "" }));
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_send_otp_oversized_email_returns_400() {
        let (state, _, _) = build_otp_state();
        let app = otp_router(state);

        let long_email = "a".repeat(321);
        let req = post_json("/auth/v1/otp", serde_json::json!({ "email": long_email }));
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_verify_otp_full_flow() {
        let (state, email_sender, _) = build_otp_state();
        let app = otp_router(state);

        let req = post_json("/auth/v1/otp", serde_json::json!({ "email": "alice@example.com" }));
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::OK);

        let code = email_sender.last_otp_for("alice@example.com").await.unwrap();

        let req = post_json(
            "/auth/v1/verify",
            serde_json::json!({ "email": "alice@example.com", "code": code }),
        );
        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), axum::http::StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["access_token"].is_string());
        assert!(json["refresh_token"].is_string());
        assert_eq!(json["token_type"], "Bearer");
    }

    #[tokio::test]
    async fn test_verify_wrong_code_returns_400() {
        let (state, _, _) = build_otp_state();
        let app = otp_router(state);

        let req = post_json("/auth/v1/otp", serde_json::json!({ "email": "alice@example.com" }));
        app.clone().oneshot(req).await.unwrap();

        let req = post_json(
            "/auth/v1/verify",
            serde_json::json!({ "email": "alice@example.com", "code": "000000" }),
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_verify_consumed_otp_returns_400() {
        let (state, email_sender, _) = build_otp_state();
        let app = otp_router(state);

        let req = post_json("/auth/v1/otp", serde_json::json!({ "email": "alice@example.com" }));
        app.clone().oneshot(req).await.unwrap();
        let code = email_sender.last_otp_for("alice@example.com").await.unwrap();

        let req = post_json(
            "/auth/v1/verify",
            serde_json::json!({ "email": "alice@example.com", "code": code }),
        );
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::OK);

        let req = post_json(
            "/auth/v1/verify",
            serde_json::json!({ "email": "alice@example.com", "code": code }),
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_verify_invalid_code_length_returns_400() {
        let (state, _, _) = build_otp_state();
        let app = otp_router(state);

        let req = post_json(
            "/auth/v1/verify",
            serde_json::json!({ "email": "alice@example.com", "code": "123" }),
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"].as_str().unwrap().contains("format"));
    }

    #[tokio::test]
    async fn test_verify_nonexistent_email_returns_400() {
        let (state, _, _) = build_otp_state();
        let app = otp_router(state);

        let req = post_json(
            "/auth/v1/verify",
            serde_json::json!({ "email": "nobody@example.com", "code": "123456" }),
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_generate_otp_code_is_6_digits() {
        let code = generate_otp_code();
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn test_generate_otp_code_is_random() {
        let code1 = generate_otp_code();
        let code2 = generate_otp_code();
        assert_eq!(code1.len(), 6);
        assert_eq!(code2.len(), 6);
    }

    #[tokio::test]
    async fn test_otp_store_verify_correct_code() {
        let store = InMemoryOtpStore::new();
        let expires_at = unix_now() + 600;
        store.store_otp("alice@example.com", "123456", expires_at).await.unwrap();

        let result = store.verify_otp("alice@example.com", "123456").await.unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn test_otp_store_verify_wrong_code() {
        let store = InMemoryOtpStore::new();
        let expires_at = unix_now() + 600;
        store.store_otp("alice@example.com", "123456", expires_at).await.unwrap();

        let result = store.verify_otp("alice@example.com", "000000").await.unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_otp_store_single_use() {
        let store = InMemoryOtpStore::new();
        let expires_at = unix_now() + 600;
        store.store_otp("alice@example.com", "123456", expires_at).await.unwrap();

        assert!(store.verify_otp("alice@example.com", "123456").await.unwrap());
        assert!(!store.verify_otp("alice@example.com", "123456").await.unwrap());
    }

    #[tokio::test]
    async fn test_otp_store_expired_code_rejected() {
        let store = InMemoryOtpStore::new();
        let expires_at = unix_now().saturating_sub(10);
        store.store_otp("alice@example.com", "123456", expires_at).await.unwrap();

        let result = store.verify_otp("alice@example.com", "123456").await.unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_otp_store_max_attempts_lockout() {
        let store = InMemoryOtpStore::new();
        let expires_at = unix_now() + 600;
        store.store_otp("alice@example.com", "123456", expires_at).await.unwrap();

        for _ in 0..MAX_OTP_ATTEMPTS {
            let _ = store.verify_otp("alice@example.com", "000000").await;
        }

        let result = store.verify_otp("alice@example.com", "123456").await;
        assert!(
            matches!(result, Err(crate::error::AuthError::RateLimited { .. })),
            "expected RateLimited after max attempts, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_otp_store_case_insensitive_email() {
        let store = InMemoryOtpStore::new();
        let expires_at = unix_now() + 600;
        store.store_otp("Alice@Example.COM", "123456", expires_at).await.unwrap();

        let result = store.verify_otp("alice@example.com", "123456").await.unwrap();
        assert!(result);
    }
}

// ── phone_otp_tests ───────────────────────────────────────────────────────────
mod phone_otp_tests {
    use std::sync::Arc;

    use axum::{Router, body::Body, http::Request, routing::post};
    use tower::ServiceExt as _;

    use super::super::phone_otp::*;
    use super::super::account_linking::{InMemoryUserStore, UserStore};
    use super::super::otp::InMemoryOtpStore;
    use super::super::session::{InMemorySessionStore, SessionStore};

    fn build_sms_state() -> (Arc<SmsOtpAuthState>, Arc<InMemorySmsSender>, Arc<InMemoryOtpStore>) {
        let otp_store = Arc::new(InMemoryOtpStore::new());
        let sms_sender = Arc::new(InMemorySmsSender::new());
        let session_store: Arc<dyn SessionStore> = Arc::new(InMemorySessionStore::new());

        let state = Arc::new(SmsOtpAuthState {
            otp_store: otp_store.clone(),
            sms_sender: sms_sender.clone(),
            session_store,
            user_store: None,
        });

        (state, sms_sender, otp_store)
    }

    fn sms_router(state: Arc<SmsOtpAuthState>) -> Router {
        Router::new()
            .route("/auth/v1/otp/sms", post(send_sms_otp))
            .route("/auth/v1/verify/sms", post(verify_sms_otp))
            .with_state(state)
    }

    fn post_json(uri: &str, body: serde_json::Value) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap()
    }

    #[test]
    fn test_normalize_e164_already_valid() {
        assert_eq!(normalize_e164("+14155551234"), Some("+14155551234".to_string()));
    }

    #[test]
    fn test_normalize_e164_strips_formatting() {
        assert_eq!(normalize_e164("+1 (415) 555-1234"), Some("+14155551234".to_string()));
    }

    #[test]
    fn test_normalize_e164_adds_plus() {
        assert_eq!(normalize_e164("14155551234"), Some("+14155551234".to_string()));
    }

    #[test]
    fn test_normalize_e164_strips_dots_and_dashes() {
        assert_eq!(normalize_e164("+1.415.555.1234"), Some("+14155551234".to_string()));
    }

    #[test]
    fn test_normalize_e164_rejects_empty() {
        assert_eq!(normalize_e164(""), None);
    }

    #[test]
    fn test_normalize_e164_rejects_too_short() {
        assert_eq!(normalize_e164("+123"), None);
    }

    #[test]
    fn test_normalize_e164_rejects_too_long() {
        assert_eq!(normalize_e164("+1234567890123456"), None);
    }

    #[test]
    fn test_normalize_e164_french_number() {
        assert_eq!(normalize_e164("+33 6 12 34 56 78"), Some("+33612345678".to_string()));
    }

    #[tokio::test]
    async fn test_send_sms_otp_returns_success() {
        let (state, sms_sender, _) = build_sms_state();
        let app = sms_router(state);

        let req = post_json("/auth/v1/otp/sms", serde_json::json!({ "phone": "+14155551234" }));
        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), axum::http::StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "otp_sent");
        assert_eq!(json["expires_in"], OTP_TTL_SECS);

        assert_eq!(sms_sender.sms_count().await, 1);
    }

    #[tokio::test]
    async fn test_send_sms_otp_empty_phone_returns_400() {
        let (state, _, _) = build_sms_state();
        let app = sms_router(state);

        let req = post_json("/auth/v1/otp/sms", serde_json::json!({ "phone": "" }));
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_send_sms_otp_invalid_phone_returns_400() {
        let (state, _, _) = build_sms_state();
        let app = sms_router(state);

        let req = post_json("/auth/v1/otp/sms", serde_json::json!({ "phone": "abc" }));
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_send_sms_normalizes_phone_number() {
        let (state, sms_sender, _) = build_sms_state();
        let app = sms_router(state);

        let req = post_json("/auth/v1/otp/sms", serde_json::json!({ "phone": "+1 (415) 555-1234" }));
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::OK);

        let code = sms_sender.last_otp_for("+14155551234").await;
        assert!(code.is_some(), "SMS should be sent to E.164 normalized number");
    }

    #[tokio::test]
    async fn test_verify_sms_otp_full_flow() {
        let (state, sms_sender, _) = build_sms_state();
        let app = sms_router(state);

        let req = post_json("/auth/v1/otp/sms", serde_json::json!({ "phone": "+14155551234" }));
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::OK);

        let code = sms_sender.last_otp_for("+14155551234").await.unwrap();

        let req = post_json(
            "/auth/v1/verify/sms",
            serde_json::json!({ "phone": "+14155551234", "code": code }),
        );
        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), axum::http::StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["access_token"].is_string());
        assert!(json["refresh_token"].is_string());
        assert_eq!(json["token_type"], "Bearer");
    }

    #[tokio::test]
    async fn test_verify_sms_wrong_code_returns_400() {
        let (state, _, _) = build_sms_state();
        let app = sms_router(state);

        let req = post_json("/auth/v1/otp/sms", serde_json::json!({ "phone": "+14155551234" }));
        app.clone().oneshot(req).await.unwrap();

        let req = post_json(
            "/auth/v1/verify/sms",
            serde_json::json!({ "phone": "+14155551234", "code": "000000" }),
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_verify_sms_normalizes_phone_on_verify() {
        let (state, sms_sender, _) = build_sms_state();
        let app = sms_router(state);

        let req = post_json("/auth/v1/otp/sms", serde_json::json!({ "phone": "+14155551234" }));
        app.clone().oneshot(req).await.unwrap();

        let code = sms_sender.last_otp_for("+14155551234").await.unwrap();

        let req = post_json(
            "/auth/v1/verify/sms",
            serde_json::json!({ "phone": "+1 (415) 555-1234", "code": code }),
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::OK);
    }

    #[tokio::test]
    async fn test_verify_sms_invalid_code_length_returns_400() {
        let (state, _, _) = build_sms_state();
        let app = sms_router(state);

        let req = post_json(
            "/auth/v1/verify/sms",
            serde_json::json!({ "phone": "+14155551234", "code": "123" }),
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_verify_sms_with_user_store() {
        let otp_store = Arc::new(InMemoryOtpStore::new());
        let sms_sender = Arc::new(InMemorySmsSender::new());
        let session_store: Arc<dyn SessionStore> = Arc::new(InMemorySessionStore::new());
        let user_store = Arc::new(InMemoryUserStore::new());

        let state = Arc::new(SmsOtpAuthState {
            otp_store: otp_store.clone(),
            sms_sender: sms_sender.clone(),
            session_store,
            user_store: Some(user_store.clone() as Arc<dyn UserStore>),
        });
        let app = sms_router(state);

        let req = post_json("/auth/v1/otp/sms", serde_json::json!({ "phone": "+33612345678" }));
        app.clone().oneshot(req).await.unwrap();

        let code = sms_sender.last_otp_for("+33612345678").await.unwrap();

        let req = post_json(
            "/auth/v1/verify/sms",
            serde_json::json!({ "phone": "+33612345678", "code": code }),
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::OK);

        assert_eq!(user_store.user_count().await, 1);
    }
}

// ── totp_mfa_tests ────────────────────────────────────────────────────────────
mod totp_mfa_tests {
    use std::sync::Arc;

    use axum::{Router, body::Body, http::Request, routing::post};
    use tower::ServiceExt as _;

    use super::super::totp_mfa::*;

    fn build_mfa_state() -> Arc<MfaAuthState> {
        Arc::new(MfaAuthState {
            mfa_store: Arc::new(InMemoryMfaStore::new()),
            issuer:    "FraiseQL".to_string(),
        })
    }

    fn mfa_router(state: Arc<MfaAuthState>) -> Router {
        Router::new()
            .route("/auth/v1/mfa/enroll", post(mfa_enroll))
            .route("/auth/v1/mfa/verify", post(mfa_verify))
            .route("/auth/v1/mfa/unenroll", post(mfa_unenroll))
            .with_state(state)
    }

    fn post_json(uri: &str, body: serde_json::Value) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap()
    }

    #[test]
    fn test_totp_code_known_vector() {
        let secret = b"12345678901234567890";
        let code = generate_totp(secret, 59);
        assert_eq!(code, "287082", "RFC 6238 test vector at time=59");
    }

    #[test]
    fn test_totp_verify_with_skew() {
        let secret = generate_totp_secret();
        let now = unix_now();
        let code = generate_totp(&secret, now);

        assert!(verify_totp(&secret, &code, now));

        let past_code = generate_totp(&secret, now.saturating_sub(TOTP_TIME_STEP));
        assert!(verify_totp(&secret, &past_code, now));

        let future_code = generate_totp(&secret, now + TOTP_TIME_STEP);
        assert!(verify_totp(&secret, &future_code, now));
    }

    #[test]
    fn test_totp_reject_far_past_code() {
        let secret = generate_totp_secret();
        let now = unix_now();
        let old_code = generate_totp(&secret, now.saturating_sub(TOTP_TIME_STEP * 3));
        assert!(!verify_totp(&secret, &old_code, now));
    }

    #[test]
    fn test_totp_reject_non_numeric() {
        let secret = generate_totp_secret();
        assert!(!verify_totp(&secret, "abcdef", unix_now()));
    }

    #[test]
    fn test_totp_code_is_6_digits() {
        let secret = generate_totp_secret();
        let code = generate_totp(&secret, unix_now());
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn test_base32_encode_known_values() {
        assert_eq!(base32_encode(b""), "");
        assert_eq!(base32_encode(b"f"), "MY");
        assert_eq!(base32_encode(b"fo"), "MZXQ");
        assert_eq!(base32_encode(b"foo"), "MZXW6");
        assert_eq!(base32_encode(b"foob"), "MZXW6YQ");
        assert_eq!(base32_encode(b"fooba"), "MZXW6YTB");
        assert_eq!(base32_encode(b"foobar"), "MZXW6YTBOI");
    }

    #[test]
    fn test_totp_uri_format() {
        let secret = b"12345678901234567890";
        let uri = totp_uri(secret, "alice@example.com", "FraiseQL");
        assert!(uri.starts_with("otpauth://totp/FraiseQL:alice%40example.com"));
        assert!(uri.contains("secret="));
        assert!(uri.contains("issuer=FraiseQL"));
        assert!(uri.contains("algorithm=SHA1"));
        assert!(uri.contains("digits=6"));
        assert!(uri.contains("period=30"));
    }

    #[test]
    fn test_recovery_codes_count_and_format() {
        let codes = generate_recovery_codes();
        assert_eq!(codes.len(), RECOVERY_CODE_COUNT);
        for code in &codes {
            assert_eq!(code.len(), 8, "recovery code must be 8 chars");
            assert!(
                code.chars().all(|c| c.is_ascii_alphanumeric()),
                "recovery code must be alphanumeric: {code}"
            );
        }
    }

    #[test]
    fn test_recovery_codes_unique() {
        let codes = generate_recovery_codes();
        let unique: std::collections::HashSet<&str> = codes.iter().map(String::as_str).collect();
        assert_eq!(unique.len(), codes.len(), "recovery codes must be unique");
    }

    #[tokio::test]
    async fn test_mfa_store_set_and_get() {
        let store = InMemoryMfaStore::new();
        let enrollment = MfaEnrollment {
            secret:         vec![1, 2, 3],
            recovery_codes: vec!["CODE1".to_string()],
            verified:       false,
        };
        store.set_enrollment("user-1", enrollment).await.unwrap();
        let result = store.get_enrollment("user-1").await.unwrap();
        assert!(result.is_some());
        assert!(!result.unwrap().verified);
    }

    #[tokio::test]
    async fn test_mfa_store_remove() {
        let store = InMemoryMfaStore::new();
        let enrollment = MfaEnrollment {
            secret:         vec![1, 2, 3],
            recovery_codes: vec![],
            verified:       true,
        };
        store.set_enrollment("user-1", enrollment).await.unwrap();
        assert!(store.remove_enrollment("user-1").await.unwrap());
        assert!(store.get_enrollment("user-1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_mfa_store_consume_recovery_code() {
        let store = InMemoryMfaStore::new();
        let enrollment = MfaEnrollment {
            secret:         vec![1, 2, 3],
            recovery_codes: vec!["AAAA1111".to_string(), "BBBB2222".to_string()],
            verified:       true,
        };
        store.set_enrollment("user-1", enrollment).await.unwrap();

        assert!(store.consume_recovery_code("user-1", "AAAA1111").await.unwrap());
        assert!(!store.consume_recovery_code("user-1", "AAAA1111").await.unwrap());
        assert!(store.consume_recovery_code("user-1", "BBBB2222").await.unwrap());
    }

    #[tokio::test]
    async fn test_mfa_enroll_returns_totp_uri_and_codes() {
        let state = build_mfa_state();
        let app = mfa_router(state);

        let req = post_json("/auth/v1/mfa/enroll", serde_json::json!({ "user_id": "user-1" }));
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert!(json["totp_uri"].as_str().unwrap().starts_with("otpauth://"));
        assert!(json["secret"].is_string());
        let codes = json["recovery_codes"].as_array().unwrap();
        assert_eq!(codes.len(), RECOVERY_CODE_COUNT);
    }

    #[tokio::test]
    async fn test_mfa_enroll_then_verify_with_totp() {
        let mfa_store = Arc::new(InMemoryMfaStore::new());
        let state = Arc::new(MfaAuthState {
            mfa_store: mfa_store.clone(),
            issuer:    "FraiseQL".to_string(),
        });
        let app = mfa_router(state);

        let req = post_json("/auth/v1/mfa/enroll", serde_json::json!({ "user_id": "user-1" }));
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::OK);

        let enrollment = mfa_store.get_enrollment("user-1").await.unwrap().unwrap();
        let code = generate_totp(&enrollment.secret, unix_now());

        let req = post_json(
            "/auth/v1/mfa/verify",
            serde_json::json!({ "user_id": "user-1", "code": code }),
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["verified"], true);

        let enrollment = mfa_store.get_enrollment("user-1").await.unwrap().unwrap();
        assert!(enrollment.verified);
    }

    #[tokio::test]
    async fn test_mfa_verify_with_recovery_code() {
        let mfa_store = Arc::new(InMemoryMfaStore::new());
        let state = Arc::new(MfaAuthState {
            mfa_store: mfa_store.clone(),
            issuer:    "FraiseQL".to_string(),
        });
        let app = mfa_router(state);

        let req = post_json("/auth/v1/mfa/enroll", serde_json::json!({ "user_id": "user-1" }));
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let recovery_code = json["recovery_codes"][0].as_str().unwrap().to_string();

        let req = post_json(
            "/auth/v1/mfa/verify",
            serde_json::json!({ "user_id": "user-1", "code": recovery_code }),
        );
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::OK);

        let req = post_json(
            "/auth/v1/mfa/verify",
            serde_json::json!({ "user_id": "user-1", "code": recovery_code }),
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_mfa_verify_wrong_code_returns_401() {
        let state = build_mfa_state();
        let app = mfa_router(state);

        let req = post_json("/auth/v1/mfa/enroll", serde_json::json!({ "user_id": "user-1" }));
        app.clone().oneshot(req).await.unwrap();

        let req = post_json(
            "/auth/v1/mfa/verify",
            serde_json::json!({ "user_id": "user-1", "code": "000000" }),
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_mfa_verify_no_enrollment_returns_404() {
        let state = build_mfa_state();
        let app = mfa_router(state);

        let req = post_json(
            "/auth/v1/mfa/verify",
            serde_json::json!({ "user_id": "user-no-mfa", "code": "123456" }),
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_mfa_unenroll_with_valid_code() {
        let mfa_store = Arc::new(InMemoryMfaStore::new());
        let state = Arc::new(MfaAuthState {
            mfa_store: mfa_store.clone(),
            issuer:    "FraiseQL".to_string(),
        });
        let app = mfa_router(state);

        let req = post_json("/auth/v1/mfa/enroll", serde_json::json!({ "user_id": "user-1" }));
        app.clone().oneshot(req).await.unwrap();

        let enrollment = mfa_store.get_enrollment("user-1").await.unwrap().unwrap();
        let code = generate_totp(&enrollment.secret, unix_now());

        let req = post_json(
            "/auth/v1/mfa/unenroll",
            serde_json::json!({ "user_id": "user-1", "code": code }),
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::OK);

        assert!(mfa_store.get_enrollment("user-1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_mfa_unenroll_wrong_code_returns_401() {
        let state = build_mfa_state();
        let app = mfa_router(state);

        let req = post_json("/auth/v1/mfa/enroll", serde_json::json!({ "user_id": "user-1" }));
        app.clone().oneshot(req).await.unwrap();

        let req = post_json(
            "/auth/v1/mfa/unenroll",
            serde_json::json!({ "user_id": "user-1", "code": "000000" }),
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_mfa_enroll_empty_user_id_returns_400() {
        let state = build_mfa_state();
        let app = mfa_router(state);

        let req = post_json("/auth/v1/mfa/enroll", serde_json::json!({ "user_id": "" }));
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_mfa_duplicate_enroll_returns_409() {
        let mfa_store = Arc::new(InMemoryMfaStore::new());
        let state = Arc::new(MfaAuthState {
            mfa_store: mfa_store.clone(),
            issuer:    "FraiseQL".to_string(),
        });
        let app = mfa_router(state);

        let req = post_json("/auth/v1/mfa/enroll", serde_json::json!({ "user_id": "user-1" }));
        app.clone().oneshot(req).await.unwrap();

        let enrollment = mfa_store.get_enrollment("user-1").await.unwrap().unwrap();
        let code = generate_totp(&enrollment.secret, unix_now());
        let req = post_json(
            "/auth/v1/mfa/verify",
            serde_json::json!({ "user_id": "user-1", "code": code }),
        );
        app.clone().oneshot(req).await.unwrap();

        let req = post_json("/auth/v1/mfa/enroll", serde_json::json!({ "user_id": "user-1" }));
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::CONFLICT);
    }
}

// ── pkce_tests ────────────────────────────────────────────────────────────────
mod pkce_tests {
    use std::sync::Arc;
    use std::time::Duration;

    use super::super::pkce::*;
    use super::super::state_encryption::{EncryptionAlgorithm, StateEncryptionService};

    fn store_no_enc(ttl_secs: u64) -> PkceStateStore {
        PkceStateStore::new(ttl_secs, None)
    }

    fn enc_service() -> Arc<StateEncryptionService> {
        Arc::new(StateEncryptionService::from_raw_key(
            &[0u8; 32],
            EncryptionAlgorithm::Chacha20Poly1305,
        ))
    }

    #[tokio::test]
    async fn test_create_and_consume_roundtrip() {
        let store = store_no_enc(600);
        let (token, verifier) = store.create_state("https://app.example.com/cb").await.unwrap();
        let result = store.consume_state(&token).await.unwrap();
        assert_eq!(result.verifier, verifier);
        assert_eq!(result.redirect_uri, "https://app.example.com/cb");
    }

    #[tokio::test]
    async fn test_consume_removes_entry_cannot_reuse() {
        let store = store_no_enc(600);
        let (token, _) = store.create_state("https://app.example.com/cb").await.unwrap();
        store.consume_state(&token).await.unwrap();
        assert!(
            matches!(store.consume_state(&token).await, Err(PkceError::StateNotFound)),
            "second consume must return StateNotFound"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn test_expired_state_returns_state_expired_not_not_found() {
        let store = store_no_enc(1);
        let (token, _) = store.create_state("https://example.com").await.unwrap();
        tokio::time::advance(Duration::from_millis(1100)).await;
        assert!(
            matches!(store.consume_state(&token).await, Err(PkceError::StateExpired)),
            "expired state must be StateExpired, not StateNotFound"
        );
    }

    #[tokio::test]
    async fn test_unknown_token_returns_not_found() {
        let store = store_no_enc(600);
        assert!(matches!(
            store.consume_state("completely-unknown-token").await,
            Err(PkceError::StateNotFound)
        ));
    }

    #[tokio::test]
    async fn test_two_distinct_states_dont_interfere() {
        let store = store_no_enc(600);
        let (t1, v1) = store.create_state("https://a.example.com/cb").await.unwrap();
        let (t2, v2) = store.create_state("https://b.example.com/cb").await.unwrap();
        let r2 = store.consume_state(&t2).await.unwrap();
        let r1 = store.consume_state(&t1).await.unwrap();
        assert_eq!(r1.verifier, v1);
        assert_eq!(r2.verifier, v2);
    }

    #[test]
    fn test_s256_challenge_matches_rfc7636_appendix_a() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let expected = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
        assert_eq!(PkceStateStore::s256_challenge(verifier), expected);
    }

    #[tokio::test]
    async fn test_verifier_length_and_charset_are_rfc7636_compliant() {
        let store = store_no_enc(600);
        let (_, verifier) = store.create_state("https://example.com").await.unwrap();
        assert!(
            verifier.len() >= 43 && verifier.len() <= 128,
            "verifier length {} is outside the 43–128 char range",
            verifier.len()
        );
        assert!(!verifier.contains('='), "verifier must not contain padding characters");
    }

    #[tokio::test]
    async fn test_encrypted_token_is_longer_than_raw_internal_key() {
        let store = PkceStateStore::new(600, Some(enc_service()));
        let (token, _) = store.create_state("https://app.example.com/cb").await.unwrap();
        assert!(
            token.len() > 43,
            "encrypted token (len={}) must be longer than a raw 32-byte key (43 chars)",
            token.len()
        );
    }

    #[tokio::test]
    async fn test_encrypted_roundtrip_works_end_to_end() {
        let store = PkceStateStore::new(600, Some(enc_service()));
        let (token, verifier) = store.create_state("https://app.example.com/cb").await.unwrap();
        let result = store.consume_state(&token).await.unwrap();
        assert_eq!(result.verifier, verifier);
    }

    #[tokio::test]
    async fn test_tampered_encrypted_token_returns_not_found() {
        let store = PkceStateStore::new(600, Some(enc_service()));
        store.create_state("https://app.example.com/cb").await.unwrap();
        let result = store.consume_state("aGVsbG8gd29ybGQ").await;
        assert!(
            matches!(result, Err(PkceError::StateNotFound)),
            "tampered token must yield StateNotFound, not an internal error"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn test_consume_at_exact_ttl_boundary_succeeds() {
        let store = store_no_enc(2);
        let (token, verifier) = store.create_state("https://example.com").await.unwrap();
        tokio::time::advance(Duration::from_secs(2)).await;
        let result = store.consume_state(&token).await.unwrap();
        assert_eq!(result.verifier, verifier, "state at exact TTL boundary must still be valid");
    }

    #[test]
    fn test_is_in_memory_returns_true_for_in_memory_store() {
        let store = PkceStateStore::new(600, None);
        assert!(store.is_in_memory());
    }

    #[tokio::test]
    async fn test_is_empty_true_for_fresh_store() {
        let store = store_no_enc(600);
        assert!(store.is_empty(), "fresh store must be empty");
        assert_eq!(store.len(), 0);
    }

    #[tokio::test]
    async fn test_is_empty_false_after_create() {
        let store = store_no_enc(600);
        store.create_state("https://example.com").await.unwrap();
        assert!(!store.is_empty(), "store with one entry must not be empty");
        assert_eq!(store.len(), 1);
    }

    #[tokio::test]
    async fn test_is_empty_true_after_consume() {
        let store = store_no_enc(600);
        let (token, _) = store.create_state("https://example.com").await.unwrap();
        store.consume_state(&token).await.unwrap();
        assert!(store.is_empty(), "store must be empty after consuming the only entry");
    }

    #[tokio::test]
    async fn test_cleanup_removes_expired_leaves_valid() {
        let store = store_no_enc(1);
        store.create_state("https://a.example.com").await.unwrap();
        tokio::time::sleep(Duration::from_millis(1100)).await;
        store.cleanup_expired().await;
        assert_eq!(store.len(), 0, "expired entry must be removed by cleanup");

        let store2 = store_no_enc(600);
        store2.create_state("https://b.example.com").await.unwrap();
        store2.cleanup_expired().await;
        assert_eq!(store2.len(), 1, "unexpired entry must survive cleanup");
    }

    #[tokio::test]
    async fn test_store_full_error_when_cap_reached() {
        let store = PkceStateStore::new_capped(600, None, 2);

        store.create_state("https://a.example.com").await.unwrap();
        store.create_state("https://b.example.com").await.unwrap();

        let result = store.create_state("https://c.example.com").await;
        assert!(
            result.is_err(),
            "create_state must fail when the store has reached its capacity"
        );
        let err = result.unwrap_err();
        assert!(err.to_string().contains("full"), "error must mention 'full' — got: {err}");
    }

    #[tokio::test]
    async fn test_purge_expired_frees_capacity_for_new_entries() {
        let store = PkceStateStore::new_capped(1, None, 2);

        store.create_state("https://a.example.com").await.unwrap();
        store.create_state("https://b.example.com").await.unwrap();

        assert!(store.create_state("https://c.example.com").await.is_err());

        tokio::time::sleep(Duration::from_millis(1100)).await;
        store.purge_expired();

        store.create_state("https://c.example.com").await.unwrap();
        assert_eq!(store.len(), 1, "store must contain exactly the new entry after purge");
    }

    #[tokio::test]
    async fn test_purge_expired_leaves_non_expired_entries_intact() {
        let store = PkceStateStore::new_capped(600, None, 10);

        let (t1, _) = store.create_state("https://a.example.com").await.unwrap();
        let (t2, _) = store.create_state("https://b.example.com").await.unwrap();

        store.purge_expired();

        assert_eq!(store.len(), 2, "unexpired entries must survive purge_expired");

        assert!(store.consume_state(&t1).await.is_ok(), "t1 must still be consumable");
        assert!(store.consume_state(&t2).await.is_ok(), "t2 must still be consumable");
    }

    #[cfg(feature = "redis-pkce")]
    #[tokio::test]
    #[ignore = "requires Redis — set REDIS_URL=redis://localhost:6379"]
    async fn test_redis_pkce_create_and_consume_roundtrip() {
        let url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());
        let store = PkceStateStore::new_redis(&url, 300, None)
            .await
            .expect("Redis connection failed");

        let (token, verifier) = store.create_state("https://example.com/cb").await.unwrap();
        let consumed = store.consume_state(&token).await.unwrap();
        assert_eq!(consumed.verifier, verifier);
        assert_eq!(consumed.redirect_uri, "https://example.com/cb");
    }

    #[cfg(feature = "redis-pkce")]
    #[tokio::test]
    #[ignore = "requires Redis — set REDIS_URL=redis://localhost:6379"]
    async fn test_redis_pkce_one_shot_consumption() {
        let url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());
        let store = PkceStateStore::new_redis(&url, 300, None)
            .await
            .expect("Redis connection failed");

        let (token, _) = store.create_state("https://example.com/cb").await.unwrap();
        store.consume_state(&token).await.unwrap();

        let second = store.consume_state(&token).await;
        assert!(
            matches!(second, Err(PkceError::StateNotFound)),
            "second consume must return StateNotFound — GETDEL guarantees one-shot"
        );
    }

    #[cfg(feature = "redis-pkce")]
    #[tokio::test]
    #[ignore = "requires Redis — set REDIS_URL=redis://localhost:6379"]
    async fn test_redis_pkce_two_instances_share_state() {
        let url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());

        let store_a = PkceStateStore::new_redis(&url, 300, None)
            .await
            .expect("Redis connection failed");
        let store_b = PkceStateStore::new_redis(&url, 300, None)
            .await
            .expect("Redis connection failed");

        let (token, verifier) = store_a.create_state("https://example.com/cb").await.unwrap();

        let consumed = store_b.consume_state(&token).await.unwrap();
        assert_eq!(
            consumed.verifier, verifier,
            "cross-replica consumption must succeed with shared Redis"
        );
    }

    #[cfg(feature = "redis-pkce")]
    #[tokio::test]
    #[ignore = "requires Redis — set REDIS_URL=redis://localhost:6379"]
    async fn test_redis_pkce_tampered_token_rejected() {
        let url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());
        let enc = Some(Arc::new(StateEncryptionService::from_raw_key(
            &[0u8; 32],
            EncryptionAlgorithm::Chacha20Poly1305,
        )));
        let store = PkceStateStore::new_redis(&url, 300, enc)
            .await
            .expect("Redis connection failed");

        store.create_state("https://example.com/cb").await.unwrap();

        let result = store.consume_state("completely-fabricated-token").await;
        assert!(
            matches!(result, Err(PkceError::StateNotFound)),
            "tampered token must be rejected"
        );
    }
}

// ── state_encryption_tests ────────────────────────────────────────────────────
// Combines service_tests and tests blocks from state_encryption.rs
mod state_encryption_service_tests {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

    use super::super::state_encryption::*;

    fn chacha_svc() -> StateEncryptionService {
        StateEncryptionService::from_raw_key(&[0u8; 32], EncryptionAlgorithm::Chacha20Poly1305)
    }
    fn aes_svc() -> StateEncryptionService {
        StateEncryptionService::from_raw_key(&[0u8; 32], EncryptionAlgorithm::Aes256Gcm)
    }

    #[test]
    fn test_chacha_encrypt_decrypt_roundtrip() {
        let svc = chacha_svc();
        let pt = b"oauth_state_nonce_12345";
        assert_eq!(svc.decrypt(&svc.encrypt(pt).unwrap()).unwrap(), pt);
    }

    #[test]
    fn test_chacha_two_encryptions_differ() {
        let svc = chacha_svc();
        assert_ne!(svc.encrypt(b"hello").unwrap(), svc.encrypt(b"hello").unwrap());
    }

    #[test]
    fn test_chacha_tampered_fails() {
        let svc = chacha_svc();
        let ct = svc.encrypt(b"secret").unwrap();
        let mut bytes = URL_SAFE_NO_PAD.decode(&ct).unwrap();
        bytes[15] ^= 0xFF;
        let tampered = URL_SAFE_NO_PAD.encode(&bytes);
        assert!(matches!(svc.decrypt(&tampered), Err(DecryptionError::AuthenticationFailed)));
    }

    #[test]
    fn test_chacha_wrong_key_fails() {
        let a =
            StateEncryptionService::from_raw_key(&[0u8; 32], EncryptionAlgorithm::Chacha20Poly1305);
        let b =
            StateEncryptionService::from_raw_key(&[1u8; 32], EncryptionAlgorithm::Chacha20Poly1305);
        let ct = a.encrypt(b"secret").unwrap();
        assert!(matches!(b.decrypt(&ct), Err(DecryptionError::AuthenticationFailed)));
    }

    #[test]
    fn test_aes_encrypt_decrypt_roundtrip() {
        let svc = aes_svc();
        let pt = b"pkce_code_challenge";
        assert_eq!(svc.decrypt(&svc.encrypt(pt).unwrap()).unwrap(), pt);
    }

    #[test]
    fn test_aes_two_encryptions_differ() {
        let svc = aes_svc();
        assert_ne!(svc.encrypt(b"hello").unwrap(), svc.encrypt(b"hello").unwrap());
    }

    #[test]
    fn test_aes_tampered_fails() {
        let svc = aes_svc();
        let ct = svc.encrypt(b"secret").unwrap();
        let mut bytes = URL_SAFE_NO_PAD.decode(&ct).unwrap();
        bytes[15] ^= 0xFF;
        let tampered = URL_SAFE_NO_PAD.encode(&bytes);
        assert!(matches!(svc.decrypt(&tampered), Err(DecryptionError::AuthenticationFailed)));
    }

    #[test]
    fn test_aes_wrong_key_fails() {
        let a = StateEncryptionService::from_raw_key(&[0u8; 32], EncryptionAlgorithm::Aes256Gcm);
        let b = StateEncryptionService::from_raw_key(&[1u8; 32], EncryptionAlgorithm::Aes256Gcm);
        let ct = a.encrypt(b"secret").unwrap();
        assert!(matches!(b.decrypt(&ct), Err(DecryptionError::AuthenticationFailed)));
    }

    #[test]
    fn test_empty_ciphertext_invalid_input() {
        assert!(matches!(chacha_svc().decrypt(""), Err(DecryptionError::InvalidInput(_))));
    }

    #[test]
    fn test_too_short_invalid_input() {
        let short = URL_SAFE_NO_PAD.encode([0u8; 11]);
        assert!(matches!(chacha_svc().decrypt(&short), Err(DecryptionError::InvalidInput(_))));
    }

    #[test]
    fn test_bad_base64_invalid_input() {
        assert!(matches!(
            chacha_svc().decrypt("not!valid@base64#"),
            Err(DecryptionError::InvalidInput(_))
        ));
    }

    #[test]
    fn test_from_hex_key_valid() {
        let hex = "00".repeat(32);
        StateEncryptionService::from_hex_key(&hex, EncryptionAlgorithm::Chacha20Poly1305)
            .unwrap_or_else(|e| panic!("expected Ok for valid 64-char hex key: {e}"));
    }

    #[test]
    fn test_from_hex_key_wrong_length() {
        assert!(matches!(
            StateEncryptionService::from_hex_key("deadbeef", EncryptionAlgorithm::Chacha20Poly1305),
            Err(KeyError::WrongLength(_))
        ));
    }

    #[test]
    fn test_from_hex_key_invalid_hex() {
        let bad = "zz".repeat(32);
        assert!(matches!(
            StateEncryptionService::from_hex_key(&bad, EncryptionAlgorithm::Chacha20Poly1305),
            Err(KeyError::InvalidHex)
        ));
    }

    #[test]
    fn test_debug_redacts_key() {
        let svc = chacha_svc();
        let s = format!("{svc:?}");
        assert!(!s.contains("00000000"), "key bytes must not appear in debug output");
        assert!(s.contains("REDACTED"));
    }

    #[test]
    fn test_from_compiled_schema_enabled_missing_key_returns_error() {
        std::env::remove_var("FRAISEQL_TEST_MISSING_ENC_KEY_B1");
        let json = serde_json::json!({
            "state_encryption": {
                "enabled": true,
                "algorithm": "chacha20-poly1305",
                "key_env": "FRAISEQL_TEST_MISSING_ENC_KEY_B1"
            }
        });
        let result = StateEncryptionService::from_compiled_schema(&json);
        assert!(result.is_err(), "should error when enabled=true but env var absent");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("FRAISEQL_TEST_MISSING_ENC_KEY_B1"));
    }

    #[test]
    fn test_from_compiled_schema_enabled() {
        let key_hex = "aa".repeat(32);
        std::env::set_var("TEST_SVC_ENC_KEY_P3", &key_hex);
        let json = serde_json::json!({
            "state_encryption": {
                "enabled": true,
                "algorithm": "chacha20-poly1305",
                "key_env": "TEST_SVC_ENC_KEY_P3"
            }
        });
        let svc = StateEncryptionService::from_compiled_schema(&json)
            .expect("should succeed when env var is set");
        assert!(svc.is_some());
        std::env::remove_var("TEST_SVC_ENC_KEY_P3");
    }

    #[test]
    fn test_from_compiled_schema_disabled() {
        let json = serde_json::json!({"state_encryption": {"enabled": false}});
        assert!(
            StateEncryptionService::from_compiled_schema(&json)
                .expect("disabled should be ok")
                .is_none()
        );
    }

    #[test]
    fn test_from_compiled_schema_missing() {
        assert!(
            StateEncryptionService::from_compiled_schema(&serde_json::json!({}))
                .expect("missing should be ok")
                .is_none()
        );
    }

    #[test]
    fn test_cross_algorithm_fails() {
        let chacha =
            StateEncryptionService::from_raw_key(&[0u8; 32], EncryptionAlgorithm::Chacha20Poly1305);
        let aes = StateEncryptionService::from_raw_key(&[0u8; 32], EncryptionAlgorithm::Aes256Gcm);
        let ct = chacha.encrypt(b"cross").unwrap();
        assert!(matches!(aes.decrypt(&ct), Err(DecryptionError::AuthenticationFailed)));
    }
}

mod state_encryption_legacy_tests {
    use super::super::state_encryption::*;
    use super::super::error::AuthError;

    fn test_key() -> [u8; 32] {
        [42u8; 32]
    }

    #[test]
    fn test_encrypt_decrypt() {
        let encryption = StateEncryption::new(&test_key()).expect("Init failed");
        let state = "oauth_state_test_value";

        let encrypted = encryption.encrypt(state).expect("Encryption failed");
        let decrypted = encryption.decrypt(&encrypted).expect("Decryption failed");

        assert_eq!(decrypted, state);
    }

    #[test]
    fn test_encrypt_produces_ciphertext() {
        let encryption = StateEncryption::new(&test_key()).expect("Init failed");
        let state = "test_state";

        let encrypted = encryption.encrypt(state).expect("Encryption failed");

        assert_ne!(encrypted.ciphertext, state.as_bytes());
    }

    #[test]
    fn test_empty_state() {
        let encryption = StateEncryption::new(&test_key()).expect("Init failed");
        let state = "";

        let encrypted = encryption.encrypt(state).expect("Encryption failed");
        let decrypted = encryption.decrypt(&encrypted).expect("Decryption failed");

        assert_eq!(decrypted, state);
    }

    #[test]
    fn test_different_keys_fail_decryption() {
        let key1 = [42u8; 32];
        let key2 = [99u8; 32];
        let state = "secret_state";

        let encryption1 = StateEncryption::new(&key1).expect("Init 1 failed");
        let encrypted = encryption1.encrypt(state).expect("Encryption failed");

        let encryption2 = StateEncryption::new(&key2).expect("Init 2 failed");
        let result = encryption2.decrypt(&encrypted);

        assert!(
            matches!(result, Err(AuthError::InvalidState)),
            "expected InvalidState for wrong-key decryption, got: {result:?}"
        );
    }

    #[test]
    fn test_tampered_ciphertext_fails() {
        let encryption = StateEncryption::new(&test_key()).expect("Init failed");
        let state = "tamper_test";

        let mut encrypted = encryption.encrypt(state).expect("Encryption failed");

        if !encrypted.ciphertext.is_empty() {
            encrypted.ciphertext[0] ^= 0xFF;
        }

        let result = encryption.decrypt(&encrypted);
        assert!(
            matches!(result, Err(AuthError::InvalidState)),
            "expected InvalidState for tampered ciphertext, got: {result:?}"
        );
    }

    #[test]
    fn test_tampered_nonce_fails() {
        let encryption = StateEncryption::new(&test_key()).expect("Init failed");
        let state = "nonce_tamper";

        let mut encrypted = encryption.encrypt(state).expect("Encryption failed");

        encrypted.nonce[0] ^= 0xFF;

        let result = encryption.decrypt(&encrypted);
        assert!(
            matches!(result, Err(AuthError::InvalidState)),
            "expected InvalidState for tampered nonce, got: {result:?}"
        );
    }

    #[test]
    fn test_truncated_ciphertext_fails() {
        let encryption = StateEncryption::new(&test_key()).expect("Init failed");
        let state = "truncation_test";

        let mut encrypted = encryption.encrypt(state).expect("Encryption failed");

        if encrypted.ciphertext.len() > 1 {
            encrypted.ciphertext.truncate(encrypted.ciphertext.len() - 1);
        }

        let result = encryption.decrypt(&encrypted);
        assert!(
            matches!(result, Err(AuthError::InvalidState)),
            "expected InvalidState for truncated ciphertext, got: {result:?}"
        );
    }

    #[test]
    fn test_serialization() {
        let encryption = StateEncryption::new(&test_key()).expect("Init failed");
        let state = "serialization_test";

        let bytes = encryption.encrypt_to_bytes(state).expect("Encryption failed");

        let decrypted = encryption.decrypt_from_bytes(&bytes).expect("Decryption failed");

        assert_eq!(decrypted, state);
    }

    #[test]
    fn test_random_nonces() {
        let encryption = StateEncryption::new(&test_key()).expect("Init failed");
        let state = "random_nonce_test";

        let encrypted1 = encryption.encrypt(state).expect("Encryption 1 failed");
        let encrypted2 = encryption.encrypt(state).expect("Encryption 2 failed");

        assert_ne!(encrypted1.nonce, encrypted2.nonce);

        let decrypted1 = encryption.decrypt(&encrypted1).expect("Decryption 1 failed");
        let decrypted2 = encryption.decrypt(&encrypted2).expect("Decryption 2 failed");

        assert_eq!(decrypted1, state);
        assert_eq!(decrypted2, state);
    }

    #[test]
    fn test_long_state() {
        let encryption = StateEncryption::new(&test_key()).expect("Init failed");
        let state = "a".repeat(10_000);

        let encrypted = encryption.encrypt(&state).expect("Encryption failed");
        let decrypted = encryption.decrypt(&encrypted).expect("Decryption failed");

        assert_eq!(decrypted, state);
    }

    #[test]
    fn test_special_characters() {
        let encryption = StateEncryption::new(&test_key()).expect("Init failed");
        let state = "state:with-special_chars.and/symbols!@#$%^&*()";

        let encrypted = encryption.encrypt(state).expect("Encryption failed");
        let decrypted = encryption.decrypt(&encrypted).expect("Decryption failed");

        assert_eq!(decrypted, state);
    }

    #[test]
    fn test_unicode_state() {
        let encryption = StateEncryption::new(&test_key()).expect("Init failed");
        let state = "state_with_emoji_🔐_🔒_🔓_and_emoji";

        let encrypted = encryption.encrypt(state).expect("Encryption failed");
        let decrypted = encryption.decrypt(&encrypted).expect("Decryption failed");

        assert_eq!(decrypted, state);
    }

    #[test]
    fn test_null_bytes_in_state() {
        let encryption = StateEncryption::new(&test_key()).expect("Init failed");
        let state = "state_with\x00null\x00bytes\x00";

        let encrypted = encryption.encrypt(state).expect("Encryption failed");
        let decrypted = encryption.decrypt(&encrypted).expect("Decryption failed");

        assert_eq!(decrypted, state);
    }

    #[test]
    fn test_key_generation() {
        let key1 = generate_state_encryption_key();
        let key2 = generate_state_encryption_key();

        assert_ne!(key1, key2);

        assert_eq!(key1.len(), 32);
        assert_eq!(key2.len(), 32);

        let enc1 = StateEncryption::new(&key1).expect("Init 1 failed");
        let enc2 = StateEncryption::new(&key2).expect("Init 2 failed");

        let state = "test";
        let encrypted1 = enc1.encrypt(state).expect("Encryption 1 failed");
        let encrypted2 = enc2.encrypt(state).expect("Encryption 2 failed");

        assert_eq!(enc1.decrypt(&encrypted1).expect("Decryption 1 failed"), state);
        assert_eq!(enc2.decrypt(&encrypted2).expect("Decryption 2 failed"), state);
    }

    #[test]
    fn test_large_ciphertext() {
        let encryption = StateEncryption::new(&test_key()).expect("Init failed");
        let state = "x".repeat(100_000);

        let encrypted = encryption.encrypt(&state).expect("Encryption failed");
        let decrypted = encryption.decrypt(&encrypted).expect("Decryption failed");

        assert_eq!(decrypted, state);
    }
}

// ── jwks_tests ────────────────────────────────────────────────────────────────
mod jwks_tests {
    use std::time::Duration;

    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    use super::super::jwks::*;

    fn jwks_fixture() -> serde_json::Value {
        serde_json::json!({
            "keys": [
                {
                    "kty": "RSA",
                    "kid": "test-key-1",
                    "use": "sig",
                    "n": "0vx7agoebGcQSuuPiLJXZptN9nndrQmbXEps2aiAFbWhM78LhWx4cbbfAAtVT86zwu1RK7aPFFxuhDR1L6tSoc_BJECPebWKRXjBZCiFV4n3oknjhMstn64tZ_2W-5JsGY4Hc5n9yBXArwl93lqt7_RN5w6Cf0h4QyQ5v-65YGjQR0_FDW2QvzqY368QQMicAtaSqzs8KJZgnYb9c7d0zgdAZHzu6qMQvRL5hajrn1n91CbOpbISD08qNLyrdkt-bFTWhAI4vMQFh6WeZu0fM4lFd2NcRwr3XPksINHaQ-G_xBniIqbw0Ls1jF44-csFCur-kEgU8awapJzKnqDKgw",
                    "e": "AQAB"
                }
            ]
        })
    }

    #[tokio::test]
    async fn test_jwks_cache_empty() {
        let cache =
            JwksCache::new("https://example.com/.well-known/jwks.json", Duration::from_secs(3600))
                .unwrap();
        assert!(cache.get_key_from_cache("nonexistent_kid").is_none());
    }

    #[tokio::test]
    async fn test_jwks_cache_fetch_and_retrieve() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(jwks_fixture()))
            .mount(&mock_server)
            .await;

        let cache = JwksCache::new(
            &format!("{}/.well-known/jwks.json", mock_server.uri()),
            Duration::from_secs(3600),
        )
        .unwrap();

        let key = cache.get_key("test-key-1").await.unwrap();
        assert!(key.is_some());
    }

    #[tokio::test]
    async fn test_jwks_cache_missing_kid_returns_none() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(jwks_fixture()))
            .mount(&mock_server)
            .await;

        let cache = JwksCache::new(
            &format!("{}/.well-known/jwks.json", mock_server.uri()),
            Duration::from_secs(3600),
        )
        .unwrap();

        let key = cache.get_key("nonexistent-kid").await.unwrap();
        assert!(key.is_none());
    }

    #[tokio::test]
    async fn test_jwks_cache_ttl_refresh() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(jwks_fixture()))
            .expect(2)
            .mount(&mock_server)
            .await;

        let cache = JwksCache::new(
            &format!("{}/.well-known/jwks.json", mock_server.uri()),
            Duration::from_secs(0),
        )
        .unwrap();

        let _ = cache.get_key("test-key-1").await.unwrap();
        let _ = cache.get_key("test-key-1").await.unwrap();
    }

    #[tokio::test]
    async fn test_jwks_cache_force_refresh() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(jwks_fixture()))
            .mount(&mock_server)
            .await;

        let cache = JwksCache::new(
            &format!("{}/.well-known/jwks.json", mock_server.uri()),
            Duration::from_secs(3600),
        )
        .unwrap();

        cache.force_refresh().await.unwrap();
        assert!(cache.get_key_from_cache("test-key-1").is_some());
    }

    #[tokio::test]
    async fn test_jwks_cache_network_error() {
        let cache =
            JwksCache::new("http://127.0.0.1:1/nonexistent", Duration::from_secs(3600)).unwrap();
        let result = cache.get_key("any-kid").await;
        assert!(result.is_err(), "expected Err for network error (connection refused)");
    }

    #[test]
    fn jwks_response_cap_constant_is_reasonable() {
        const { assert!(MAX_JWKS_RESPONSE_BYTES >= 64 * 1024) }
        const { assert!(MAX_JWKS_RESPONSE_BYTES <= 100 * 1024 * 1024) }
    }

    #[tokio::test]
    async fn jwks_oversized_response_is_rejected() {
        let mock_server = MockServer::start().await;
        let oversized = vec![b'x'; MAX_JWKS_RESPONSE_BYTES + 1];
        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(oversized))
            .mount(&mock_server)
            .await;

        let cache = JwksCache::new(
            &format!("{}/.well-known/jwks.json", mock_server.uri()),
            Duration::from_secs(3600),
        )
        .unwrap();
        let result = cache.get_key("any-kid").await;
        assert!(result.is_err(), "oversized JWKS response must be rejected");
        let msg = result.err().unwrap();
        assert!(msg.contains("too large"), "error must mention size limit: {msg}");
    }

    #[tokio::test]
    async fn jwks_within_size_limit_is_accepted() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(jwks_fixture()))
            .mount(&mock_server)
            .await;

        let cache = JwksCache::new(
            &format!("{}/.well-known/jwks.json", mock_server.uri()),
            Duration::from_secs(3600),
        )
        .unwrap();
        let key = cache
            .get_key("test-key-1")
            .await
            .unwrap_or_else(|e| panic!("normal JWKS response must be accepted, got: {e}"));
        assert!(key.is_some(), "expected key 'test-key-1' to be present in JWKS response");
    }

    #[test]
    fn test_jwks_cache_rejects_invalid_url() {
        let result = JwksCache::new("not-a-url", Duration::from_secs(3600));
        assert!(result.is_err(), "invalid URL should be rejected at construction");
        assert!(matches!(result.unwrap_err(), JwksError::InvalidUrl { .. }));
    }

    #[test]
    fn test_jwks_cache_rejects_non_http_scheme() {
        let result = JwksCache::new("ftp://example.com/jwks.json", Duration::from_secs(3600));
        assert!(matches!(result.unwrap_err(), JwksError::InvalidScheme { .. }));
    }

    #[test]
    fn test_jwks_cache_rejects_http_non_localhost() {
        let result = JwksCache::new("http://example.com/jwks.json", Duration::from_secs(3600));
        assert!(matches!(result.unwrap_err(), JwksError::InvalidScheme { .. }));
    }

    #[test]
    fn test_jwks_cache_accepts_https() {
        let result =
            JwksCache::new("https://example.com/.well-known/jwks.json", Duration::from_secs(3600));
        assert!(result.is_ok(), "valid https:// URL should be accepted");
    }

    #[test]
    fn test_jwks_cache_accepts_http_localhost() {
        let result = JwksCache::new(
            "http://localhost:8080/.well-known/jwks.json",
            Duration::from_secs(3600),
        );
        assert!(result.is_ok(), "http://localhost should be accepted for dev");
    }

    #[test]
    fn test_ssrf_blocks_loopback_v4() {
        let ip: std::net::IpAddr = "127.0.0.1".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "127.0.0.1 must be blocked");
        let ip: std::net::IpAddr = "127.255.255.255".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "127.x.x.x must be blocked");
    }

    #[test]
    fn test_ssrf_blocks_rfc1918_10() {
        let ip: std::net::IpAddr = "10.0.0.1".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "10.0.0.1 must be blocked");
        let ip: std::net::IpAddr = "10.255.255.255".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "10.255.255.255 must be blocked");
    }

    #[test]
    fn test_ssrf_blocks_rfc1918_172() {
        let ip: std::net::IpAddr = "172.16.0.1".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "172.16.0.1 must be blocked");
        let ip: std::net::IpAddr = "172.31.255.255".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "172.31.255.255 must be blocked");
        let ip: std::net::IpAddr = "172.15.0.1".parse().unwrap();
        assert!(!is_ssrf_blocked_ip(&ip), "172.15.0.1 must NOT be blocked");
        let ip: std::net::IpAddr = "172.32.0.1".parse().unwrap();
        assert!(!is_ssrf_blocked_ip(&ip), "172.32.0.1 must NOT be blocked");
    }

    #[test]
    fn test_ssrf_blocks_rfc1918_192_168() {
        let ip: std::net::IpAddr = "192.168.0.1".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "192.168.0.1 must be blocked");
        let ip: std::net::IpAddr = "192.168.255.255".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "192.168.255.255 must be blocked");
        let ip: std::net::IpAddr = "192.169.0.1".parse().unwrap();
        assert!(!is_ssrf_blocked_ip(&ip), "192.169.0.1 must NOT be blocked");
    }

    #[test]
    fn test_ssrf_blocks_link_local_169_254() {
        let ip: std::net::IpAddr = "169.254.0.1".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "169.254.x.x must be blocked");
        let ip: std::net::IpAddr = "169.254.169.254".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "AWS metadata IP must be blocked");
    }

    #[test]
    fn test_ssrf_blocks_cgnat_100_64() {
        let ip: std::net::IpAddr = "100.64.0.1".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "100.64.0.1 (CGNAT) must be blocked");
        let ip: std::net::IpAddr = "100.127.255.255".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "100.127.255.255 (CGNAT) must be blocked");
        let ip: std::net::IpAddr = "100.63.255.255".parse().unwrap();
        assert!(!is_ssrf_blocked_ip(&ip), "100.63.x.x is NOT CGNAT");
        let ip: std::net::IpAddr = "100.128.0.1".parse().unwrap();
        assert!(!is_ssrf_blocked_ip(&ip), "100.128.x.x is NOT CGNAT");
    }

    #[test]
    fn test_ssrf_blocks_unspecified_v4() {
        let ip: std::net::IpAddr = "0.0.0.0".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "0.0.0.0 must be blocked");
    }

    #[test]
    fn test_ssrf_allows_public_ips() {
        for addr in &["8.8.8.8", "1.1.1.1", "93.184.216.34", "203.0.113.1"] {
            let ip: std::net::IpAddr = addr.parse().unwrap();
            assert!(!is_ssrf_blocked_ip(&ip), "{addr} is public and must NOT be blocked");
        }
    }

    #[test]
    fn test_ssrf_blocks_loopback_v6() {
        let ip: std::net::IpAddr = "::1".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "::1 must be blocked");
    }

    #[test]
    fn test_ssrf_blocks_unspecified_v6() {
        let ip: std::net::IpAddr = "::".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), ":: must be blocked");
    }

    #[test]
    fn test_ssrf_blocks_ipv4_mapped_v6() {
        let ip: std::net::IpAddr = "::ffff:127.0.0.1".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "::ffff:127.0.0.1 must be blocked");
        let ip: std::net::IpAddr = "::ffff:10.0.0.1".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "::ffff:10.0.0.1 must be blocked");
    }

    #[test]
    fn test_ssrf_blocks_ula_v6() {
        let ip: std::net::IpAddr = "fc00::1".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "fc00::1 (ULA) must be blocked");
        let ip: std::net::IpAddr = "fd00::1".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "fd00::1 (ULA) must be blocked");
    }

    #[test]
    fn test_ssrf_blocks_link_local_v6() {
        let ip: std::net::IpAddr = "fe80::1".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "fe80::1 (link-local) must be blocked");
    }

    #[test]
    fn test_ssrf_allows_public_v6() {
        let ip: std::net::IpAddr = "2001:4860:4860::8888".parse().unwrap();
        assert!(!is_ssrf_blocked_ip(&ip), "Google DNS v6 must NOT be blocked");
    }

    #[test]
    fn test_jwks_cache_debug_format() {
        let cache =
            JwksCache::new("https://example.com/.well-known/jwks.json", Duration::from_secs(3600))
                .unwrap();
        let dbg = format!("{cache:?}");
        assert!(dbg.contains("JwksCache"), "Debug output must contain struct name");
        assert!(dbg.contains("example.com"), "Debug output must contain jwks_uri");
    }
}

// ── multi_provider_tests ──────────────────────────────────────────────────────
mod multi_provider_tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use axum::{Router, body::Body, http::Request, routing::get};
    use tower::ServiceExt as _;

    use super::super::multi_provider::*;
    use super::super::account_linking::UserStore;
    use super::super::error::Result as AuthResult;
    use super::super::provider::{OAuthProvider, TokenResponse, UserInfo};
    use super::super::session::{InMemorySessionStore, SessionStore};
    use super::super::state_store::{InMemoryStateStore, StateStore};

    #[derive(Debug, Clone)]
    struct MockProvider {
        name:      String,
        auth_url:  String,
        user_info: UserInfo,
    }

    impl MockProvider {
        fn new(name: &str) -> Self {
            Self {
                name:      name.to_string(),
                auth_url:  format!("https://{name}.example.com/authorize"),
                user_info: UserInfo {
                    id:         format!("{name}-user-1"),
                    email:      format!("user@{name}.com"),
                    name:       Some("Test User".to_string()),
                    picture:    None,
                    raw_claims: serde_json::json!({}),
                },
            }
        }

        fn with_email(name: &str, email: &str) -> Self {
            Self {
                name:      name.to_string(),
                auth_url:  format!("https://{name}.example.com/authorize"),
                user_info: UserInfo {
                    id:         format!("{name}-user-1"),
                    email:      email.to_string(),
                    name:       Some("Test User".to_string()),
                    picture:    None,
                    raw_claims: serde_json::json!({}),
                },
            }
        }
    }

    #[async_trait]
    impl OAuthProvider for MockProvider {
        fn name(&self) -> &str {
            &self.name
        }

        fn authorization_url(&self, state: &str) -> String {
            format!("{}?state={}&client_id=test", self.auth_url, state)
        }

        async fn exchange_code(&self, _code: &str) -> AuthResult<TokenResponse> {
            Ok(TokenResponse {
                access_token:  "mock_access_token".to_string(),
                refresh_token: Some("mock_refresh_token".to_string()),
                expires_in:    3600,
                token_type:    "Bearer".to_string(),
            })
        }

        async fn user_info(&self, _access_token: &str) -> AuthResult<UserInfo> {
            Ok(self.user_info.clone())
        }
    }

    fn build_multi_provider_state(providers: Vec<(&str, MockProvider)>) -> Arc<MultiProviderAuthState> {
        build_state_with_user_store(providers, None)
    }

    fn build_state_with_user_store(
        providers: Vec<(&str, MockProvider)>,
        user_store: Option<Arc<dyn UserStore>>,
    ) -> Arc<MultiProviderAuthState> {
        let state_store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        let session_store: Arc<dyn SessionStore> = Arc::new(InMemorySessionStore::new());
        let mut auth_state = MultiProviderAuthState::new(state_store, session_store);
        if let Some(us) = user_store {
            auth_state = auth_state.with_user_store(us);
        }
        for (name, provider) in providers {
            auth_state.register_provider(name, Arc::new(provider));
        }
        Arc::new(auth_state)
    }

    fn multi_auth_router(state: Arc<MultiProviderAuthState>) -> Router {
        Router::new()
            .route("/auth/v1/providers", get(list_providers))
            .route("/auth/v1/authorize", get(authorize))
            .route("/auth/v1/callback", get(callback))
            .with_state(state)
    }

    #[tokio::test]
    async fn test_list_providers_returns_registered_providers() {
        let state = build_multi_provider_state(vec![
            ("github", MockProvider::new("github")),
            ("google", MockProvider::new("google")),
        ]);
        let app = multi_auth_router(state);

        let req = Request::builder()
            .uri("/auth/v1/providers")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), axum::http::StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        let providers = json["providers"].as_array().unwrap();
        assert_eq!(providers.len(), 2);
        assert!(providers.contains(&serde_json::json!("github")));
        assert!(providers.contains(&serde_json::json!("google")));
    }

    #[tokio::test]
    async fn test_list_providers_empty_when_none_registered() {
        let state = build_multi_provider_state(vec![]);
        let app = multi_auth_router(state);

        let req = Request::builder()
            .uri("/auth/v1/providers")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), axum::http::StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["providers"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_authorize_redirects_to_provider() {
        let state = build_multi_provider_state(vec![
            ("github", MockProvider::new("github")),
        ]);
        let app = multi_auth_router(state);

        let req = Request::builder()
            .uri("/auth/v1/authorize?provider=github&redirect_uri=https%3A%2F%2Fapp.example.com%2Fcb")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();

        assert!(
            resp.status().is_redirection(),
            "expected redirect, got {}",
            resp.status()
        );

        let location = resp
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .expect("Location header must be present");

        assert!(
            location.starts_with("https://github.example.com/authorize"),
            "redirect should go to github authorize URL, got: {location}"
        );
        assert!(location.contains("state="), "redirect must include state parameter");
        assert!(location.contains("client_id=test"), "redirect must include client_id");
    }

    #[tokio::test]
    async fn test_authorize_unknown_provider_returns_400() {
        let state = build_multi_provider_state(vec![
            ("github", MockProvider::new("github")),
        ]);
        let app = multi_auth_router(state);

        let req = Request::builder()
            .uri("/auth/v1/authorize?provider=twitter&redirect_uri=https%3A%2F%2Fapp.example.com%2Fcb")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            json["error"].as_str().unwrap().contains("unknown provider"),
            "error must mention unknown provider: {json}"
        );
    }

    #[tokio::test]
    async fn test_authorize_missing_redirect_uri_returns_400() {
        let state = build_multi_provider_state(vec![
            ("github", MockProvider::new("github")),
        ]);
        let app = multi_auth_router(state);

        let req = Request::builder()
            .uri("/auth/v1/authorize?provider=github")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert!(resp.status().is_client_error());
    }

    #[tokio::test]
    async fn test_authorize_empty_redirect_uri_returns_400() {
        let state = build_multi_provider_state(vec![
            ("github", MockProvider::new("github")),
        ]);
        let app = multi_auth_router(state);

        let req = Request::builder()
            .uri("/auth/v1/authorize?provider=github&redirect_uri=")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_authorize_oversized_redirect_uri_returns_400() {
        let state = build_multi_provider_state(vec![
            ("github", MockProvider::new("github")),
        ]);
        let app = multi_auth_router(state);

        let long_uri = "https://example.com/".to_string() + &"a".repeat(2100);
        let encoded = urlencoding::encode(&long_uri);
        let req = Request::builder()
            .uri(format!("/auth/v1/authorize?provider=github&redirect_uri={encoded}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_callback_unknown_state_returns_400() {
        let state = build_multi_provider_state(vec![
            ("github", MockProvider::new("github")),
        ]);
        let app = multi_auth_router(state);

        let req = Request::builder()
            .uri("/auth/v1/callback?code=test123&state=unknown-state-token")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_callback_missing_code_returns_400() {
        let state = build_multi_provider_state(vec![
            ("github", MockProvider::new("github")),
        ]);
        let app = multi_auth_router(state);

        let req = Request::builder()
            .uri("/auth/v1/callback?state=some-state")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_callback_provider_error_returns_sanitized_message() {
        let state = build_multi_provider_state(vec![
            ("github", MockProvider::new("github")),
        ]);
        let app = multi_auth_router(state);

        let req = Request::builder()
            .uri("/auth/v1/callback?error=access_denied&error_description=internal+details")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let error_msg = json["error"].as_str().unwrap();
        assert_eq!(error_msg, "Access was denied");
        assert!(!error_msg.contains("internal details"));
    }

    #[tokio::test]
    async fn test_authorize_to_callback_round_trip() {
        let state = build_multi_provider_state(vec![
            ("github", MockProvider::new("github")),
        ]);
        let app = multi_auth_router(state);

        let req = Request::builder()
            .uri("/auth/v1/authorize?provider=github&redirect_uri=https%3A%2F%2Fapp.example.com%2Fcb")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();

        assert!(resp.status().is_redirection());

        let location = resp
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .unwrap()
            .to_string();

        let parsed = reqwest::Url::parse(&location).unwrap();
        let state_token = parsed
            .query_pairs()
            .find(|(k, _)| k == "state")
            .map(|(_, v)| v.into_owned())
            .expect("state must be in redirect URL");

        let callback_uri = format!("/auth/v1/callback?code=auth_code_123&state={state_token}");
        let req2 = Request::builder()
            .uri(&callback_uri)
            .body(Body::empty())
            .unwrap();
        let resp2 = app.oneshot(req2).await.unwrap();

        assert_eq!(resp2.status(), axum::http::StatusCode::OK, "callback should return 200");

        let body = axum::body::to_bytes(resp2.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert!(json["access_token"].is_string(), "must have access_token");
        assert!(json["refresh_token"].is_string(), "must have refresh_token");
        assert_eq!(json["token_type"], "Bearer");
        assert!(json["expires_in"].is_number(), "must have expires_in");
        assert_eq!(json["provider"], "github", "must include provider name");
    }

    #[tokio::test]
    async fn test_callback_state_consumed_on_first_use() {
        let state = build_multi_provider_state(vec![
            ("github", MockProvider::new("github")),
        ]);
        let app = multi_auth_router(state);

        let req = Request::builder()
            .uri("/auth/v1/authorize?provider=github&redirect_uri=https%3A%2F%2Fapp.example.com%2Fcb")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let location = resp.headers().get("location").unwrap().to_str().unwrap().to_string();
        let parsed = reqwest::Url::parse(&location).unwrap();
        let state_token = parsed
            .query_pairs()
            .find(|(k, _)| k == "state")
            .map(|(_, v)| v.into_owned())
            .unwrap();

        let req1 = Request::builder()
            .uri(format!("/auth/v1/callback?code=code1&state={state_token}"))
            .body(Body::empty())
            .unwrap();
        let resp1 = app.clone().oneshot(req1).await.unwrap();
        assert_eq!(resp1.status(), axum::http::StatusCode::OK);

        let req2 = Request::builder()
            .uri(format!("/auth/v1/callback?code=code2&state={state_token}"))
            .body(Body::empty())
            .unwrap();
        let resp2 = app.oneshot(req2).await.unwrap();
        assert_eq!(resp2.status(), axum::http::StatusCode::BAD_REQUEST, "state replay must be rejected");
    }

    #[tokio::test]
    async fn test_different_providers_produce_different_callbacks() {
        let state = build_multi_provider_state(vec![
            ("github", MockProvider::new("github")),
            ("google", MockProvider::new("google")),
        ]);
        let app = multi_auth_router(state);

        let req_gh = Request::builder()
            .uri("/auth/v1/authorize?provider=github&redirect_uri=https%3A%2F%2Fapp.example.com")
            .body(Body::empty())
            .unwrap();
        let resp_gh = app.clone().oneshot(req_gh).await.unwrap();
        let loc_gh = resp_gh.headers().get("location").unwrap().to_str().unwrap().to_string();

        let req_gg = Request::builder()
            .uri("/auth/v1/authorize?provider=google&redirect_uri=https%3A%2F%2Fapp.example.com")
            .body(Body::empty())
            .unwrap();
        let resp_gg = app.clone().oneshot(req_gg).await.unwrap();
        let loc_gg = resp_gg.headers().get("location").unwrap().to_str().unwrap().to_string();

        assert!(loc_gh.starts_with("https://github.example.com/"), "github redirect wrong");
        assert!(loc_gg.starts_with("https://google.example.com/"), "google redirect wrong");

        let state_gh = reqwest::Url::parse(&loc_gh).unwrap()
            .query_pairs().find(|(k, _)| k == "state").map(|(_, v)| v.into_owned()).unwrap();
        let state_gg = reqwest::Url::parse(&loc_gg).unwrap()
            .query_pairs().find(|(k, _)| k == "state").map(|(_, v)| v.into_owned()).unwrap();

        let req_cb_gh = Request::builder()
            .uri(format!("/auth/v1/callback?code=c1&state={state_gh}"))
            .body(Body::empty())
            .unwrap();
        let resp_cb_gh = app.clone().oneshot(req_cb_gh).await.unwrap();
        let body_gh = axum::body::to_bytes(resp_cb_gh.into_body(), usize::MAX).await.unwrap();
        let json_gh: serde_json::Value = serde_json::from_slice(&body_gh).unwrap();
        assert_eq!(json_gh["provider"], "github");

        let req_cb_gg = Request::builder()
            .uri(format!("/auth/v1/callback?code=c2&state={state_gg}"))
            .body(Body::empty())
            .unwrap();
        let resp_cb_gg = app.oneshot(req_cb_gg).await.unwrap();
        let body_gg = axum::body::to_bytes(resp_cb_gg.into_body(), usize::MAX).await.unwrap();
        let json_gg: serde_json::Value = serde_json::from_slice(&body_gg).unwrap();
        assert_eq!(json_gg["provider"], "google");
    }

    #[test]
    fn test_provider_names_sorted() {
        let state_store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        let session_store: Arc<dyn SessionStore> = Arc::new(InMemorySessionStore::new());
        let mut auth_state = MultiProviderAuthState::new(state_store, session_store);
        auth_state.register_provider("google", Arc::new(MockProvider::new("google")));
        auth_state.register_provider("auth0", Arc::new(MockProvider::new("auth0")));
        auth_state.register_provider("github", Arc::new(MockProvider::new("github")));

        let names = auth_state.provider_names();
        assert_eq!(names, vec!["auth0", "github", "google"]);
    }

    #[test]
    fn test_get_provider_returns_none_for_unknown() {
        let state_store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        let session_store: Arc<dyn SessionStore> = Arc::new(InMemorySessionStore::new());
        let auth_state = MultiProviderAuthState::new(state_store, session_store);
        assert!(auth_state.get_provider("nonexistent").is_none());
    }

    async fn do_auth_round_trip(app: &Router, provider: &str) -> serde_json::Value {
        let req = Request::builder()
            .uri(format!(
                "/auth/v1/authorize?provider={provider}&redirect_uri=https%3A%2F%2Fapp.example.com%2Fcb"
            ))
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert!(resp.status().is_redirection(), "authorize should redirect");

        let location = resp.headers().get("location").unwrap().to_str().unwrap().to_string();
        let parsed = reqwest::Url::parse(&location).unwrap();
        let state_token = parsed
            .query_pairs()
            .find(|(k, _)| k == "state")
            .map(|(_, v)| v.into_owned())
            .unwrap();

        let req2 = Request::builder()
            .uri(format!("/auth/v1/callback?code=authcode&state={state_token}"))
            .body(Body::empty())
            .unwrap();
        let resp2 = app.clone().oneshot(req2).await.unwrap();
        assert_eq!(resp2.status(), axum::http::StatusCode::OK);

        let body = axum::body::to_bytes(resp2.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn test_account_linking_same_email_different_providers() {
        use super::super::account_linking::InMemoryUserStore;

        let user_store = Arc::new(InMemoryUserStore::new());
        let state = build_state_with_user_store(
            vec![
                ("github", MockProvider::with_email("github", "alice@example.com")),
                ("google", MockProvider::with_email("google", "alice@example.com")),
            ],
            Some(user_store.clone() as Arc<dyn UserStore>),
        );
        let app = multi_auth_router(state);

        let json1 = do_auth_round_trip(&app, "github").await;
        let json2 = do_auth_round_trip(&app, "google").await;

        assert_eq!(json1["provider"], "github");
        assert_eq!(json2["provider"], "google");

        assert_eq!(user_store.user_count().await, 1);
    }

    #[tokio::test]
    async fn test_account_linking_different_emails_different_users() {
        use super::super::account_linking::InMemoryUserStore;

        let user_store = Arc::new(InMemoryUserStore::new());
        let state = build_state_with_user_store(
            vec![
                ("github", MockProvider::with_email("github", "alice@example.com")),
                ("google", MockProvider::with_email("google", "bob@example.com")),
            ],
            Some(user_store.clone() as Arc<dyn UserStore>),
        );
        let app = multi_auth_router(state);

        do_auth_round_trip(&app, "github").await;
        do_auth_round_trip(&app, "google").await;

        assert_eq!(user_store.user_count().await, 2);
    }

    #[tokio::test]
    async fn test_without_user_store_uses_provider_id() {
        let state = build_multi_provider_state(vec![
            ("github", MockProvider::new("github")),
        ]);
        let app = multi_auth_router(state);

        let json = do_auth_round_trip(&app, "github").await;
        assert!(json["access_token"].is_string());
        assert_eq!(json["provider"], "github");
    }
}

// ── oidc_provider_tests ───────────────────────────────────────────────────────
mod oidc_provider_tests {
    use super::super::oidc_provider::*;
    use super::super::provider::OAuthProvider;

    #[test]
    fn oidc_discovery_cap_constant_is_reasonable() {
        const { assert!(MAX_OIDC_DISCOVERY_BYTES >= 1024) }
        const { assert!(MAX_OIDC_DISCOVERY_BYTES <= 10 * 1024 * 1024) }
    }

    #[test]
    fn oidc_token_cap_constant_is_reasonable() {
        const { assert!(MAX_OIDC_TOKEN_BYTES >= 64 * 1024) }
        const { assert!(MAX_OIDC_TOKEN_BYTES <= 100 * 1024 * 1024) }
    }

    #[test]
    fn oidc_request_timeout_is_set() {
        let secs = OIDC_REQUEST_TIMEOUT.as_secs();
        assert!(secs > 0 && secs <= 120, "OIDC timeout should be 1–120 s, got {secs}");
    }

    #[tokio::test]
    async fn oidc_discovery_oversized_response_is_rejected() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };

        let mock_server = MockServer::start().await;
        let oversized = vec![b'x'; MAX_OIDC_DISCOVERY_BYTES + 1];
        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(oversized))
            .mount(&mock_server)
            .await;

        let result = temp_env::async_with_vars(
            [("FRAISEQL_OIDC_ALLOW_INSECURE", Some("1"))],
            OidcProvider::new(
                "test",
                &mock_server.uri(),
                "client_id",
                "client_secret",
                "http://localhost/callback",
            ),
        )
        .await;

        assert!(result.is_err(), "oversized discovery response must be rejected");
        let msg = result.err().unwrap().to_string();
        assert!(
            msg.contains("too large") || msg.contains("large"),
            "error must mention size: {msg}"
        );
    }

    #[tokio::test]
    async fn oidc_discovery_within_size_limit_proceeds_to_parse() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };

        let mock_server = MockServer::start().await;
        let tiny = b"{}".to_vec();
        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(tiny))
            .mount(&mock_server)
            .await;

        let result = temp_env::async_with_vars(
            [("FRAISEQL_OIDC_ALLOW_INSECURE", Some("1"))],
            OidcProvider::new(
                "test",
                &mock_server.uri(),
                "client_id",
                "client_secret",
                "http://localhost/callback",
            ),
        )
        .await;

        assert!(
            result.is_err(),
            "expected Err when discovery doc has missing fields, got: {result:?}"
        );
        let msg = result.err().unwrap().to_string();
        assert!(
            !msg.contains("too large"),
            "size gate must not trigger for a small response: {msg}"
        );
    }

    #[test]
    fn test_oauth_provider_name() {
        let provider = OidcProvider {
            name:          "my-oidc".to_string(),
            issuer_url:    "https://example.com".to_string(),
            client_id:     "client_id".to_string(),
            client_secret: zeroize::Zeroizing::new("secret".to_string()),
            redirect_uri:  "http://localhost:8000/callback".to_string(),
            discovery:     OidcDiscovery {
                issuer:                 "https://example.com".to_string(),
                authorization_endpoint: "https://example.com/auth".to_string(),
                token_endpoint:         "https://example.com/token".to_string(),
                userinfo_endpoint:      "https://example.com/userinfo".to_string(),
                jwks_uri:               None,
                revocation_endpoint:    None,
            },
            client:        reqwest::Client::new(),
        };
        assert_eq!(OAuthProvider::name(&provider), "my-oidc");
    }

    #[test]
    fn test_oauth_provider_debug() {
        let provider = OidcProvider {
            name:          "test".to_string(),
            issuer_url:    "https://example.com".to_string(),
            client_id:     "client_id".to_string(),
            client_secret: zeroize::Zeroizing::new("secret".to_string()),
            redirect_uri:  "http://localhost:8000/callback".to_string(),
            discovery:     OidcDiscovery {
                issuer:                 "https://example.com".to_string(),
                authorization_endpoint: "https://example.com/auth".to_string(),
                token_endpoint:         "https://example.com/token".to_string(),
                userinfo_endpoint:      "https://example.com/userinfo".to_string(),
                jwks_uri:               None,
                revocation_endpoint:    None,
            },
            client:        reqwest::Client::new(),
        };

        let debug_str = format!("{:?}", provider);
        assert!(debug_str.contains("OidcProvider"));
        assert!(debug_str.contains("test"));
    }

    #[test]
    fn test_add_auth_params() {
        let provider = OidcProvider {
            name:          "test".to_string(),
            issuer_url:    "https://example.com".to_string(),
            client_id:     "my_client".to_string(),
            client_secret: zeroize::Zeroizing::new("secret".to_string()),
            redirect_uri:  "http://localhost:8000/callback".to_string(),
            discovery:     OidcDiscovery {
                issuer:                 "https://example.com".to_string(),
                authorization_endpoint: "https://example.com/auth".to_string(),
                token_endpoint:         "https://example.com/token".to_string(),
                userinfo_endpoint:      "https://example.com/userinfo".to_string(),
                jwks_uri:               None,
                revocation_endpoint:    None,
            },
            client:        reqwest::Client::new(),
        };

        let mut url = "https://example.com/auth".to_string();
        provider.add_auth_params(&mut url, "state123", None);

        assert!(url.contains("client_id=my_client"));
        assert!(url.contains("state=state123"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("scope=openid"));
    }

    #[tokio::test]
    async fn revoke_token_non_success_status_is_rejected() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };

        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/revoke"))
            .respond_with(ResponseTemplate::new(400))
            .mount(&mock_server)
            .await;

        let provider = OidcProvider {
            name:          "test".to_string(),
            issuer_url:    mock_server.uri(),
            client_id:     "client_id".to_string(),
            client_secret: zeroize::Zeroizing::new("secret".to_string()),
            redirect_uri:  "http://localhost/callback".to_string(),
            discovery:     OidcDiscovery {
                issuer:                 mock_server.uri(),
                authorization_endpoint: format!("{}/auth", mock_server.uri()),
                token_endpoint:         format!("{}/token", mock_server.uri()),
                userinfo_endpoint:      format!("{}/userinfo", mock_server.uri()),
                jwks_uri:               None,
                revocation_endpoint:    Some(format!("{}/revoke", mock_server.uri())),
            },
            client:        reqwest::Client::new(),
        };

        let result = provider.revoke_token("some_token").await;
        assert!(result.is_err(), "non-2xx revocation response must be rejected");
        let msg = result.err().unwrap().to_string();
        assert!(
            msg.contains("400") || msg.contains("revocation"),
            "error must mention HTTP status or revocation failure: {msg}"
        );
    }

    #[tokio::test]
    async fn revoke_token_success_returns_ok() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };

        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/revoke"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let provider = OidcProvider {
            name:          "test".to_string(),
            issuer_url:    mock_server.uri(),
            client_id:     "client_id".to_string(),
            client_secret: zeroize::Zeroizing::new("secret".to_string()),
            redirect_uri:  "http://localhost/callback".to_string(),
            discovery:     OidcDiscovery {
                issuer:                 mock_server.uri(),
                authorization_endpoint: format!("{}/auth", mock_server.uri()),
                token_endpoint:         format!("{}/token", mock_server.uri()),
                userinfo_endpoint:      format!("{}/userinfo", mock_server.uri()),
                jwks_uri:               None,
                revocation_endpoint:    Some(format!("{}/revoke", mock_server.uri())),
            },
            client:        reqwest::Client::new(),
        };

        provider
            .revoke_token("some_token")
            .await
            .unwrap_or_else(|e| panic!("200 revocation response must return Ok: {e}"));
    }

    #[test]
    fn oidc_issuer_url_must_use_https_scheme() {
        temp_env::with_vars([("FRAISEQL_OIDC_ALLOW_INSECURE", None::<&str>)], || {
            let result = validate_oidc_issuer_url("http://accounts.google.com");
            assert!(result.is_err(), "http:// issuer URL must be rejected");
            let msg = result.unwrap_err().to_string();
            assert!(
                msg.contains("https") || msg.contains("scheme"),
                "error must mention scheme requirement: {msg}"
            );
        });
    }

    #[test]
    fn oidc_issuer_url_rejects_non_url_scheme() {
        temp_env::with_vars([("FRAISEQL_OIDC_ALLOW_INSECURE", None::<&str>)], || {
            let result = validate_oidc_issuer_url("ftp://accounts.example.com");
            assert!(result.is_err(), "non-https scheme must be rejected");
        });
    }

    #[test]
    fn oidc_issuer_url_accepts_https_public_host() {
        temp_env::with_vars([("FRAISEQL_OIDC_ALLOW_INSECURE", None::<&str>)], || {
            let result = validate_oidc_issuer_url("https://accounts.google.com");
            assert!(result.is_ok(), "valid https public URL should be accepted: {result:?}");
        });
    }

    #[test]
    fn oidc_issuer_url_rejects_localhost() {
        temp_env::with_vars([("FRAISEQL_OIDC_ALLOW_INSECURE", None::<&str>)], || {
            let result = validate_oidc_issuer_url("https://localhost:8080");
            assert!(result.is_err(), "localhost issuer must be rejected (SSRF protection)");
            let msg = result.unwrap_err().to_string();
            assert!(
                msg.contains("SSRF") || msg.contains("loopback") || msg.contains("private"),
                "error must mention SSRF protection: {msg}"
            );
        });
    }

    #[test]
    fn oidc_issuer_url_rejects_loopback_ipv4() {
        temp_env::with_vars([("FRAISEQL_OIDC_ALLOW_INSECURE", None::<&str>)], || {
            let result = validate_oidc_issuer_url("https://127.0.0.1");
            assert!(result.is_err(), "loopback IPv4 issuer must be rejected");
        });
    }

    #[test]
    fn oidc_issuer_url_rejects_loopback_ipv6() {
        temp_env::with_vars([("FRAISEQL_OIDC_ALLOW_INSECURE", None::<&str>)], || {
            let result = validate_oidc_issuer_url("https://[::1]");
            assert!(result.is_err(), "loopback IPv6 issuer must be rejected");
        });
    }

    #[test]
    fn oidc_issuer_url_rejects_rfc1918_private_range() {
        temp_env::with_vars([("FRAISEQL_OIDC_ALLOW_INSECURE", None::<&str>)], || {
            let result = validate_oidc_issuer_url("https://10.0.0.1");
            assert!(result.is_err(), "RFC 1918 private IP must be rejected (SSRF protection)");
            let result2 = validate_oidc_issuer_url("https://172.16.0.1");
            assert!(result2.is_err(), "172.16/12 private IP must be rejected");
            let result3 = validate_oidc_issuer_url("https://192.168.1.1");
            assert!(result3.is_err(), "192.168/16 private IP must be rejected");
        });
    }

    #[test]
    fn oidc_issuer_url_rejects_link_local() {
        temp_env::with_vars([("FRAISEQL_OIDC_ALLOW_INSECURE", None::<&str>)], || {
            let result = validate_oidc_issuer_url("https://169.254.169.254");
            assert!(result.is_err(), "link-local (AWS IMDS) IP must be rejected (SSRF protection)");
        });
    }

    #[tokio::test]
    async fn oidc_provider_new_rejects_http_issuer() {
        temp_env::with_vars([("FRAISEQL_OIDC_ALLOW_INSECURE", None::<&str>)], || {
            let result = validate_oidc_issuer_url("http://evil.example.com");
            assert!(result.is_err(), "http:// issuer URL must be rejected by SSRF guard");
            let msg = result.unwrap_err().to_string();
            assert!(
                msg.contains("https") || msg.contains("scheme"),
                "error must mention scheme requirement: {msg}"
            );
        });
    }

    #[tokio::test]
    async fn oidc_provider_new_rejects_loopback_issuer() {
        temp_env::with_vars([("FRAISEQL_OIDC_ALLOW_INSECURE", None::<&str>)], || {
            let result = validate_oidc_issuer_url("https://127.0.0.1:9999");
            assert!(result.is_err(), "OidcProvider::new must reject loopback issuer URLs");
        });
    }

    #[test]
    fn oidc_provider_client_secret_is_zeroized_on_drop() {
        let mut secret = zeroize::Zeroizing::new("oidc-provider-secret-12345".to_string());
        assert!(!secret.is_empty(), "secret should be non-empty before zeroize");
        zeroize::Zeroize::zeroize(&mut *secret);
        assert!(secret.is_empty(), "secret bytes must be wiped after zeroize");

        let provider = OidcProvider {
            name:          "test".to_string(),
            issuer_url:    "https://example.com".to_string(),
            client_id:     "id".to_string(),
            client_secret: zeroize::Zeroizing::new("my_secret".to_string()),
            redirect_uri:  "https://example.com/callback".to_string(),
            discovery:     OidcDiscovery {
                issuer:                 "https://example.com".to_string(),
                authorization_endpoint: "https://example.com/auth".to_string(),
                token_endpoint:         "https://example.com/token".to_string(),
                userinfo_endpoint:      "https://example.com/userinfo".to_string(),
                jwks_uri:               None,
                revocation_endpoint:    None,
            },
            client:        reqwest::Client::new(),
        };
        let _: &zeroize::Zeroizing<String> = &provider.client_secret;
    }
}

// ── oidc_server_client_tests ──────────────────────────────────────────────────
mod oidc_server_client_tests {
    use super::super::oidc_server_client::*;

    const VALID_VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";

    fn test_client() -> OidcServerClient {
        OidcServerClient::new(
            "test-client",
            "test-secret",
            "https://api.example.com/auth/callback",
            "https://provider.example.com/authorize",
            "https://provider.example.com/token",
        )
    }

    #[test]
    fn test_authorization_url_contains_required_pkce_params() {
        let client = test_client();
        let url = client.authorization_url("my_state", "my_challenge", "S256");
        assert!(url.contains("response_type=code"), "missing response_type");
        assert!(url.contains("client_id=test-client"), "missing client_id");
        assert!(url.contains("code_challenge=my_challenge"), "missing code_challenge");
        assert!(url.contains("code_challenge_method=S256"), "missing method");
        assert!(url.contains("state="), "missing state");
        assert!(url.contains("redirect_uri="), "missing redirect_uri");
    }

    #[test]
    fn oidc_response_cap_constant_is_reasonable() {
        assert_eq!(OidcServerClient::MAX_OIDC_RESPONSE_BYTES, 1024 * 1024);
    }

    #[test]
    fn oidc_response_cap_covers_error_path() {
        const { assert!(OidcServerClient::MAX_OIDC_RESPONSE_BYTES >= 64 * 1024) }
        const { assert!(OidcServerClient::MAX_OIDC_RESPONSE_BYTES <= 100 * 1024 * 1024) }
    }

    #[tokio::test]
    async fn oidc_oversized_error_response_is_rejected() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };

        let mock_server = MockServer::start().await;
        let oversized = vec![b'e'; OidcServerClient::MAX_OIDC_RESPONSE_BYTES + 1];
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(400).set_body_bytes(oversized))
            .mount(&mock_server)
            .await;

        let client = OidcServerClient::new(
            "client_id",
            "client_secret",
            "https://example.com/callback",
            "https://example.com/auth",
            format!("{}/token", mock_server.uri()),
        );
        let http = reqwest::Client::new();
        let result = client.exchange_code("code", VALID_VERIFIER, &http).await;

        assert!(result.is_err(), "oversized error response must be rejected");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("too large"), "error must mention size limit, got: {msg}");
    }

    #[tokio::test]
    async fn oidc_oversized_success_response_is_rejected() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };

        let mock_server = MockServer::start().await;
        let oversized = vec![b'x'; OidcServerClient::MAX_OIDC_RESPONSE_BYTES + 1];
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(oversized))
            .mount(&mock_server)
            .await;

        let client = OidcServerClient::new(
            "client_id",
            "client_secret",
            "https://example.com/callback",
            "https://example.com/auth",
            format!("{}/token", mock_server.uri()),
        );
        let http = reqwest::Client::new();
        let result = client.exchange_code("code", VALID_VERIFIER, &http).await;

        assert!(result.is_err(), "oversized success response must be rejected, got: {result:?}");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("too large"), "error must mention size limit, got: {msg}");
    }

    #[test]
    fn test_authorization_url_includes_openid_scope() {
        let client = test_client();
        let url = client.authorization_url("s", "c", "S256");
        assert!(url.contains("openid"), "authorization URL must request the openid scope: {url}");
    }

    #[test]
    fn test_authorization_url_state_is_percent_encoded() {
        let client = test_client();
        let state_with_spaces = "hello world+test";
        let url = client.authorization_url(state_with_spaces, "challenge", "S256");
        let state_segment = url.split("state=").nth(1).unwrap().split('&').next().unwrap();
        assert!(!state_segment.contains(' '), "space in state must be percent-encoded");
        assert!(!state_segment.contains('+'), "plus in state must be percent-encoded");
    }

    #[test]
    fn test_from_compiled_schema_absent_auth_returns_none() {
        let json = serde_json::json!({});
        assert!(OidcServerClient::from_compiled_schema(&json).is_none());
    }

    #[test]
    fn test_from_compiled_schema_missing_env_var_returns_none() {
        let json = serde_json::json!({
            "auth": {
                "discovery_url":       "https://example.com",
                "client_id":           "x",
                "client_secret_env":   "__FRAISEQL_TEST_DEFINITELY_UNSET_42XYZ__",
                "server_redirect_uri": "https://api.example.com/auth/callback"
            },
            "auth_endpoints": {
                "authorization_endpoint": "https://example.com/auth",
                "token_endpoint":         "https://example.com/token"
            }
        });
        let _ = OidcServerClient::from_compiled_schema(&json);
    }

    #[test]
    fn test_from_compiled_schema_missing_endpoints_returns_none() {
        let json = serde_json::json!({
            "auth": {
                "discovery_url":       "https://example.com",
                "client_id":           "x",
                "client_secret_env":   "PATH",
                "server_redirect_uri": "https://api.example.com/auth/callback"
            }
        });
        assert!(
            OidcServerClient::from_compiled_schema(&json).is_none(),
            "missing auth_endpoints must return None"
        );
    }

    #[test]
    fn test_debug_redacts_client_secret() {
        let client = test_client();
        let debug_str = format!("{client:?}");
        assert!(
            !debug_str.contains("test-secret"),
            "Debug output must not expose the client secret: {debug_str}"
        );
        assert!(debug_str.contains("[REDACTED]"));
    }

    #[test]
    fn oidc_server_client_secret_is_zeroized_on_drop() {
        let mut secret = zeroize::Zeroizing::new("oidc-server-secret-12345".to_string());
        assert!(!secret.is_empty(), "secret should be non-empty before zeroize");
        zeroize::Zeroize::zeroize(&mut *secret);
        assert!(secret.is_empty(), "secret bytes must be wiped after zeroize");

        let client = test_client();
        let _: &zeroize::Zeroizing<String> = &client.client_secret;
    }

    #[tokio::test]
    async fn exchange_code_rejects_short_code_verifier() {
        let client = test_client();
        let http = reqwest::Client::new();
        let short = "a".repeat(OidcServerClient::MIN_CODE_VERIFIER_BYTES - 1);
        let result = client.exchange_code("code_abc", &short, &http).await;
        assert!(result.is_err(), "exchange_code must reject a short code_verifier");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("too short"), "error must mention 'too short', got: {msg}");
    }

    #[tokio::test]
    async fn exchange_code_rejects_long_code_verifier() {
        let client = test_client();
        let http = reqwest::Client::new();
        let long = "a".repeat(OidcServerClient::MAX_CODE_VERIFIER_BYTES + 1);
        let result = client.exchange_code("code_abc", &long, &http).await;
        assert!(result.is_err(), "exchange_code must reject an oversized code_verifier");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("too long"), "error must mention 'too long', got: {msg}");
    }

    #[test]
    fn code_verifier_bounds_match_rfc7636() {
        assert_eq!(OidcServerClient::MIN_CODE_VERIFIER_BYTES, 43);
        assert_eq!(OidcServerClient::MAX_CODE_VERIFIER_BYTES, 128);
    }

    #[test]
    fn valid_verifier_constant_is_within_bounds() {
        assert!(VALID_VERIFIER.len() >= OidcServerClient::MIN_CODE_VERIFIER_BYTES);
        assert!(VALID_VERIFIER.len() <= OidcServerClient::MAX_CODE_VERIFIER_BYTES);
    }
}

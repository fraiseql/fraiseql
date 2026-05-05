//! Security header enforcement.

use std::collections::HashMap;

/// Security headers configuration
#[derive(Debug)]
pub struct SecurityHeaders {
    headers: HashMap<String, String>,
}

impl Default for SecurityHeaders {
    /// Create default security headers
    fn default() -> Self {
        let mut headers = HashMap::new();

        // Prevent XSS
        headers.insert("X-XSS-Protection".to_string(), "1; mode=block".to_string());

        // Prevent MIME sniffing
        headers.insert("X-Content-Type-Options".to_string(), "nosniff".to_string());

        // Prevent clickjacking
        headers.insert("X-Frame-Options".to_string(), "DENY".to_string());

        // Referrer policy
        headers
            .insert("Referrer-Policy".to_string(), "strict-origin-when-cross-origin".to_string());

        // Permissions policy
        headers.insert(
            "Permissions-Policy".to_string(),
            "geolocation=(), microphone=(), camera=()".to_string(),
        );

        Self { headers }
    }
}

impl SecurityHeaders {
    /// Create production-grade security headers
    #[must_use]
    pub fn production() -> Self {
        let mut headers = Self::default().headers;

        // Stricter CSP for production
        headers.insert(
            "Content-Security-Policy".to_string(),
            "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data: https:; font-src 'self'; connect-src 'self'; frame-ancestors 'none'".to_string(),
        );

        // HSTS with preload
        headers.insert(
            "Strict-Transport-Security".to_string(),
            "max-age=63072000; includeSubDomains; preload".to_string(),
        );

        Self { headers }
    }

    /// Get headers as Vec for HTTP response
    #[must_use]
    pub fn to_vec(&self) -> Vec<(String, String)> {
        self.headers.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }

    /// Add custom header
    pub fn add(&mut self, name: String, value: String) {
        self.headers.insert(name, value);
    }

    /// Remove header
    pub fn remove(&mut self, name: &str) {
        self.headers.remove(name);
    }

    /// Get header value
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&String> {
        self.headers.get(name)
    }

    /// Check if header exists
    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        self.headers.contains_key(name)
    }

    /// Get all header names
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.headers.keys().cloned().collect()
    }

    /// Merge with another `SecurityHeaders` instance
    pub fn merge(&mut self, other: &Self) {
        for (key, value) in &other.headers {
            self.headers.insert(key.clone(), value.clone());
        }
    }

    /// Create headers for development environment
    #[must_use]
    pub fn development() -> Self {
        let mut headers = Self::default().headers;

        // More permissive CSP for development
        headers.insert(
            "Content-Security-Policy".to_string(),
            "default-src 'self' 'unsafe-inline' 'unsafe-eval'; script-src 'self' 'unsafe-inline' 'unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https: http:; font-src 'self' data:; connect-src 'self' ws: wss: http: https:".to_string(),
        );

        Self { headers }
    }
}

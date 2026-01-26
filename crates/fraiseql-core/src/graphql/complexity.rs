// GraphQL query complexity analysis to prevent DoS attacks
// Limits: max depth, max field count, max total complexity score

/// Query complexity configuration
#[derive(Debug, Clone)]
pub struct ComplexityConfig {
    /// Maximum query depth (nesting level) - default: 15
    pub max_depth: usize,
    /// Maximum field count in a single query - default: 100
    pub max_fields: usize,
    /// Maximum complexity score (depth * field_count) - default: 500
    pub max_score: usize,
}

impl Default for ComplexityConfig {
    fn default() -> Self {
        Self {
            max_depth: 15,
            max_fields: 100,
            max_score: 500,
        }
    }
}

/// Query complexity analyzer
pub struct ComplexityAnalyzer {
    config: ComplexityConfig,
}

impl ComplexityAnalyzer {
    /// Create a new analyzer with default config
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: ComplexityConfig::default(),
        }
    }

    /// Create with custom config
    #[must_use]
    pub fn with_config(config: ComplexityConfig) -> Self {
        Self { config }
    }

    /// Analyze query complexity
    /// Returns (max_depth, field_count, total_score)
    #[must_use]
    pub fn analyze_complexity(&self, query: &str) -> (usize, usize, usize) {
        // Parse query string to count nesting and fields
        let mut max_depth = 0;
        let mut current_depth = 0;
        let mut field_count = 0;
        let mut in_braces = false;

        for ch in query.chars() {
            match ch {
                '{' => {
                    in_braces = true;
                    current_depth += 1;
                    max_depth = max_depth.max(current_depth);
                }
                '}' => {
                    if current_depth > 0 {
                        current_depth -= 1;
                    }
                    in_braces = false;
                }
                '(' | ')' => {
                    // Argument delimiters - not counted as fields
                }
                c if in_braces && c.is_alphabetic() => {
                    // Count this as a potential field start
                    field_count += 1;
                }
                _ => {}
            }
        }

        let total_score = max_depth * field_count.max(1);
        (max_depth, field_count, total_score)
    }

    /// Check if query exceeds limits
    pub fn is_query_too_complex(&self, query: &str) -> Result<(), String> {
        let (depth, fields, score) = self.analyze_complexity(query);

        if depth > self.config.max_depth {
            return Err(format!(
                "Query depth {} exceeds maximum {}",
                depth, self.config.max_depth
            ));
        }

        if fields > self.config.max_fields {
            return Err(format!(
                "Query field count {} exceeds maximum {}",
                fields, self.config.max_fields
            ));
        }

        if score > self.config.max_score {
            return Err(format!(
                "Query complexity score {} exceeds maximum {}",
                score, self.config.max_score
            ));
        }

        Ok(())
    }
}

impl Default for ComplexityAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_query_complexity() {
        let analyzer = ComplexityAnalyzer::new();
        let query = "{ users { id name } }";
        let (depth, _fields, _score) = analyzer.analyze_complexity(query);
        assert!(depth <= 3);
    }

    #[test]
    fn test_deeply_nested_query() {
        let analyzer = ComplexityAnalyzer::new();
        let query = "{ a { b { c { d { e { f { g { h } } } } } } } }";
        let (depth, _fields, _score) = analyzer.analyze_complexity(query);
        assert!(depth >= 8);
    }

    #[test]
    fn test_query_too_deep() {
        let config = ComplexityConfig {
            max_depth: 5,
            max_fields: 100,
            max_score: 500,
        };
        let analyzer = ComplexityAnalyzer::with_config(config);

        let query = "{ a { b { c { d { e { f { g { h } } } } } } } }";
        assert!(analyzer.is_query_too_complex(query).is_err());
    }

    #[test]
    fn test_query_within_limits() {
        let analyzer = ComplexityAnalyzer::new();
        let query = "{ users { id name email } posts { id title } }";
        assert!(analyzer.is_query_too_complex(query).is_ok());
    }

    #[test]
    fn test_complexity_score() {
        let analyzer = ComplexityAnalyzer::new();
        let query = "{ users { id name email } }";
        let (_depth, _fields, score) = analyzer.analyze_complexity(query);
        // Score should be reasonable
        assert!(score <= 500);
    }
}

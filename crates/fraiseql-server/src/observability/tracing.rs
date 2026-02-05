//! Distributed tracing with OpenTelemetry
//!
//! Provides span creation, trace context management, and propagation

use std::fmt;

/// Tracer initialization result
pub fn init_tracer() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize OpenTelemetry tracer
    // In minimal GREEN phase, this is a no-op
    Ok(())
}

/// Span attribute key-value pair
#[derive(Clone, Debug)]
pub struct SpanAttribute {
    pub key:   String,
    pub value: String,
}

/// Span builder for creating spans with attributes
#[derive(Debug)]
pub struct SpanBuilder {
    name:       String,
    attributes: Vec<SpanAttribute>,
}

impl SpanBuilder {
    /// Create a new span builder
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name:       name.into(),
            attributes: Vec::new(),
        }
    }

    /// Add an attribute to the span
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.push(SpanAttribute {
            key:   key.into(),
            value: value.into(),
        });
        self
    }

    /// Build the span
    pub fn build(self) -> Span {
        Span {
            name:       self.name,
            attributes: self.attributes,
            status:     SpanStatus::Ok,
        }
    }
}

/// Span status
#[derive(Clone, Debug)]
pub enum SpanStatus {
    /// Span completed successfully
    Ok,
    /// Span encountered an error
    Error {
        /// Error message
        message: String,
        /// Error code or SQLSTATE
        code:    String,
    },
}

/// A tracing span
#[derive(Clone, Debug)]
pub struct Span {
    /// Span name
    pub name:       String,
    /// Span attributes
    pub attributes: Vec<SpanAttribute>,
    /// Span completion status
    pub status:     SpanStatus,
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Span({})", self.name)
    }
}

/// Create a new span
pub fn create_span(name: impl Into<String>) -> SpanBuilder {
    SpanBuilder::new(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_builder() {
        let span = SpanBuilder::new("test_operation")
            .with_attribute("operation_type", "query")
            .with_attribute("user_id", "user-123")
            .build();

        assert_eq!(span.name, "test_operation");
        assert_eq!(span.attributes.len(), 2);
    }
}

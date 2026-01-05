//! Error handling for Tylax conversions
//!
//! This module provides a unified error type and result type for all
//! conversion operations.

use std::fmt;

/// Conversion error type
#[derive(Debug, Clone)]
pub enum ConversionError {
    /// Parse error - input could not be parsed
    ParseError {
        message: String,
        line: Option<usize>,
        column: Option<usize>,
    },
    /// Unsupported feature
    UnsupportedFeature {
        feature: String,
        suggestion: Option<String>,
    },
    /// Invalid input
    InvalidInput { message: String },
    /// IO error (for file operations)
    IoError { message: String },
    /// Internal error
    InternalError { message: String },
}

impl fmt::Display for ConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConversionError::ParseError {
                message,
                line,
                column,
            } => {
                if let (Some(l), Some(c)) = (line, column) {
                    write!(f, "Parse error at line {}, column {}: {}", l, c, message)
                } else if let Some(l) = line {
                    write!(f, "Parse error at line {}: {}", l, message)
                } else {
                    write!(f, "Parse error: {}", message)
                }
            }
            ConversionError::UnsupportedFeature {
                feature,
                suggestion,
            } => {
                if let Some(sug) = suggestion {
                    write!(f, "Unsupported feature '{}'. {}", feature, sug)
                } else {
                    write!(f, "Unsupported feature: {}", feature)
                }
            }
            ConversionError::InvalidInput { message } => {
                write!(f, "Invalid input: {}", message)
            }
            ConversionError::IoError { message } => {
                write!(f, "IO error: {}", message)
            }
            ConversionError::InternalError { message } => {
                write!(f, "Internal error: {}", message)
            }
        }
    }
}

impl std::error::Error for ConversionError {}

impl From<std::io::Error> for ConversionError {
    fn from(err: std::io::Error) -> Self {
        ConversionError::IoError {
            message: err.to_string(),
        }
    }
}

/// Result type for conversion operations
pub type ConversionResult<T> = Result<T, ConversionError>;

/// Conversion warnings (non-fatal issues)
#[derive(Debug, Clone)]
pub struct ConversionWarning {
    pub message: String,
    pub line: Option<usize>,
    pub suggestion: Option<String>,
}

impl fmt::Display for ConversionWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(l) = self.line {
            write!(f, "Warning at line {}: {}", l, self.message)?;
        } else {
            write!(f, "Warning: {}", self.message)?;
        }
        if let Some(ref sug) = self.suggestion {
            write!(f, " ({})", sug)?;
        }
        Ok(())
    }
}

/// Conversion output with optional warnings
#[derive(Debug, Clone)]
pub struct ConversionOutput {
    /// The converted content
    pub content: String,
    /// Any warnings generated during conversion
    pub warnings: Vec<ConversionWarning>,
}

impl ConversionOutput {
    pub fn new(content: String) -> Self {
        Self {
            content,
            warnings: Vec::new(),
        }
    }

    pub fn with_warnings(content: String, warnings: Vec<ConversionWarning>) -> Self {
        Self { content, warnings }
    }

    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }
}

// Convenience constructors for errors
impl ConversionError {
    pub fn parse(message: impl Into<String>) -> Self {
        ConversionError::ParseError {
            message: message.into(),
            line: None,
            column: None,
        }
    }

    pub fn parse_at(message: impl Into<String>, line: usize, column: usize) -> Self {
        ConversionError::ParseError {
            message: message.into(),
            line: Some(line),
            column: Some(column),
        }
    }

    pub fn unsupported(feature: impl Into<String>) -> Self {
        ConversionError::UnsupportedFeature {
            feature: feature.into(),
            suggestion: None,
        }
    }

    pub fn unsupported_with_suggestion(
        feature: impl Into<String>,
        suggestion: impl Into<String>,
    ) -> Self {
        ConversionError::UnsupportedFeature {
            feature: feature.into(),
            suggestion: Some(suggestion.into()),
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        ConversionError::InvalidInput {
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        ConversionError::InternalError {
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_error_display() {
        let err = ConversionError::parse("unexpected token");
        assert!(err.to_string().contains("Parse error"));
        assert!(err.to_string().contains("unexpected token"));
    }

    #[test]
    fn test_parse_error_with_location() {
        let err = ConversionError::parse_at("unexpected token", 10, 5);
        let msg = err.to_string();
        assert!(msg.contains("line 10"));
        assert!(msg.contains("column 5"));
    }

    #[test]
    fn test_unsupported_feature() {
        let err = ConversionError::unsupported_with_suggestion(
            "tikz-3d",
            "Consider using 2D approximation",
        );
        let msg = err.to_string();
        assert!(msg.contains("tikz-3d"));
        assert!(msg.contains("Consider"));
    }

    #[test]
    fn test_conversion_output() {
        let output = ConversionOutput::new("hello".to_string());
        assert!(!output.has_warnings());

        let output_with_warn = ConversionOutput::with_warnings(
            "hello".to_string(),
            vec![ConversionWarning {
                message: "test warning".to_string(),
                line: Some(1),
                suggestion: None,
            }],
        );
        assert!(output_with_warn.has_warnings());
    }
}

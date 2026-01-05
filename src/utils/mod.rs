//! Utility modules
//!
//! This module contains utilities and helpers:
//! - Diagnostics and error reporting
//! - File resolution for multi-file documents
//! - Error types and result types

pub mod diagnostics;
pub mod error;
pub mod files;

// Re-export commonly used items
pub use diagnostics::{check_latex, format_diagnostics, Diagnostic, DiagnosticLevel};
pub use error::{ConversionError, ConversionOutput, ConversionResult, ConversionWarning};
pub use files::{FileResolveError, FileResolver, MemoryFileResolver, NoopFileResolver};

#[cfg(not(target_arch = "wasm32"))]
pub use files::StdFileResolver;

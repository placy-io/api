// Copyright 2025 Placy
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Error types for placy-core.

use std::path::PathBuf;
use thiserror::Error;

/// Result type alias using the [`Error`](enum@Error) enum.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur during JAR/ZIP processing.
#[derive(Debug, Error)]
pub enum Error {
    /// I/O error occurred.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// ZIP archive error.
    #[error("ZIP error: {0}")]
    Zip(#[from] zip::result::ZipError),

    /// Java class file parsing error.
    #[error("Class file error: {0}")]
    ClassFile(String),

    /// Regex compilation error.
    #[error("Invalid regex pattern '{pattern}': {source}")]
    InvalidRegex {
        pattern: String,
        #[source]
        source: regex::Error,
    },

    /// Security violation detected.
    #[error("Security violation: {0}")]
    SecurityViolation(String),

    /// Path traversal attempt detected.
    #[error("Path traversal detected in entry: {0}")]
    PathTraversal(String),

    /// File or archive exceeds size limits.
    #[error("Size limit exceeded: {message}")]
    SizeLimitExceeded { message: String },

    /// Too many files in archive.
    #[error("File count limit exceeded: found {found}, maximum is {limit}")]
    FileCountExceeded { found: usize, limit: usize },

    /// Invalid archive structure.
    #[error("Invalid archive structure: {0}")]
    InvalidArchive(String),

    /// File not found.
    #[error("File not found: {0}")]
    FileNotFound(PathBuf),

    /// Invalid UTF-8 encoding.
    #[error("Invalid UTF-8 in {context}: {details}")]
    InvalidUtf8 { context: String, details: String },

    /// Configuration error.
    #[error("Configuration error: {0}")]
    Config(String),
}

impl From<ristretto_classfile::Error> for Error {
    fn from(err: ristretto_classfile::Error) -> Self {
        Error::ClassFile(err.to_string())
    }
}

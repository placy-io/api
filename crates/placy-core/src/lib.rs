//! # placy-core
//!
//! A high-performance library for JAR/ZIP placeholder replacement.
//!
//! This library provides robust APIs for:
//! - Loading and processing JAR files with placeholder replacement in Java class constant pools
//! - Processing ZIP archives containing multiple JARs with regex-based file matching
//! - Automatic text file placeholder replacement (UTF-8/UTF-16)
//! - Security-focused design with path traversal protection
//!
//! ## Features
//!
//! - **Regex-based file matching**: Use `process.txt` to specify which JAR files to process
//! - **File exclusion**: Use `ignore.txt` to exclude files from processing
//! - **Parallel processing**: Uses Rayon for concurrent class file processing
//! - **Security hardened**: Comprehensive path traversal and zip bomb protection
//!
//! ## Example
//!
//! ```no_run
//! use placy_core::{Config, process_jar, process_archive};
//! use std::path::Path;
//!
//! # fn main() -> placy_core::Result<()> {
//! // Process a single JAR file
//! let config = Config::builder()
//!     .add_placeholder("%%__USERNAME__%%", "john_doe")
//!     .add_placeholder("%%__TIMESTAMP__%%", "1234567890")
//!     .build();
//!
//! let input_jar = std::fs::read("input.jar")?;
//! let output_jar = process_jar(&input_jar, &config)?;
//! std::fs::write("output.jar", output_jar)?;
//!
//! // Process a ZIP archive with multiple JARs
//! let input_zip = std::fs::read("input.zip")?;
//! let output_zip = process_archive(&input_zip, &config)?;
//! std::fs::write("output.zip", output_zip)?;
//! # Ok(())
//! # }
//! ```

pub mod archive;
pub mod config;
pub mod error;
pub mod jar;
pub mod processor;
pub mod security;
mod text;

pub use archive::process_archive;
pub use config::{Config, ConfigBuilder};
pub use error::{Error, Result};
pub use jar::{process_jar, JarFile};

//! PlacyRust - A high-performance JAR file placeholder replacement tool
//!
//! This library provides utilities for loading JAR files into memory, processing Java class files,
//! and replacing placeholder strings in the constant pool with actual values.
//!
//! # Features
//!
//! - **In-Memory Processing**: Load entire JAR files into memory for fast access
//! - **Parallel Processing**: Process multiple class files simultaneously using Rayon
//! - **Constant Pool Manipulation**: Replace UTF-8 strings in Java class constant pools
//! - **Structure Preservation**: Maintains all non-class files (manifests, resources, etc.)
//!
//! # Architecture
//!
//! The library is organized into two main modules:
//!
//! - [`jar`]: Handles JAR file I/O and in-memory representation
//! - [`processor`]: Provides class file processing and placeholder replacement logic
//!
//! # Example
//!
//! ```no_run
//! use PlacyRust::{load_jar_in_memory, process_classes, write_jar};
//! use std::collections::HashMap;
//! use std::fs;
//!
//! # fn main() -> anyhow::Result<()> {
//! // Load a JAR file into memory
//! let jar_bytes = fs::read("input.jar")?;
//! let mut jar_memory = load_jar_in_memory(jar_bytes)?;
//!
//! // Define placeholder replacements
//! let mut placeholders = HashMap::new();
//! placeholders.insert("%%__USERNAME__%%", "john_doe");
//! placeholders.insert("%%__TIMESTAMP__%%", "1234567890");
//! placeholders.insert("%%__NONCE__%%", "a1b2c3d4e5f6");
//!
//! // Process all class files, replacing placeholders
//! process_classes(&mut jar_memory, &placeholders)?;
//!
//! // Write the modified JAR to disk
//! write_jar(&jar_memory, "output.jar")?;
//! # Ok(())
//! # }
//! ```
//!
//! # Performance
//!
//! The library is optimized for performance:
//!
//! - Uses parallel processing via [`rayon`] for class file manipulation
//! - Stores entire JAR in memory to minimize I/O operations
//! - Pre-allocates buffers based on known file sizes
//! - Processes class files in-place when possible
//!
//! # Use Cases
//!
//! This library is particularly useful for:
//!
//! - Build systems that need to inject version information into JARs
//! - License servers that customize JAR files per customer
//! - Obfuscation tools that need to modify constant pool entries
//! - Any scenario requiring batch modification of Java class files

pub mod config;
pub mod jar;
pub mod processor;
pub mod workflows;
pub mod zip_operations;

pub use config::Config;
pub use jar::{JarMemory, load_jar_in_memory, write_jar};
pub use processor::process_classes;
pub use workflows::{process_jar_workflow, process_single_jar_in_place, process_zip_workflow};

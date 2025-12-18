// Copyright 2025 Placy
// SPDX-License-Identifier: AGPL-3.0-or-later

//! JAR file operations and processing.

use crate::config::Config;
use crate::error::{Error, Result};
use crate::processor::process_class_bytes;
use crate::security::validate_archive_path;
use crate::text::process_text_file;

use rayon::prelude::*;
use std::io::{Cursor, Read, Write};
use zip::{write::FileOptions, CompressionMethod, ZipArchive, ZipWriter};

/// In-memory representation of a JAR file.
///
/// Separates class files from other resources for efficient processing.
#[derive(Debug, Clone)]
pub struct JarFile {
    /// Class files with their paths and bytecode.
    pub classes: Vec<(String, Vec<u8>)>,
    /// Non-class files (resources, manifests, etc.).
    pub resources: Vec<(String, Vec<u8>)>,
}

impl JarFile {
    /// Creates a new empty JarFile.
    pub fn new() -> Self {
        Self {
            classes: Vec::new(),
            resources: Vec::new(),
        }
    }

    /// Returns the total number of entries.
    pub fn len(&self) -> usize {
        self.classes.len() + self.resources.len()
    }

    /// Returns true if the JAR has no entries.
    pub fn is_empty(&self) -> bool {
        self.classes.is_empty() && self.resources.is_empty()
    }
}

impl Default for JarFile {
    fn default() -> Self {
        Self::new()
    }
}

/// Loads a JAR file from bytes into memory.
///
/// # Arguments
///
/// * `bytes` - The raw bytes of the JAR file
/// * `config` - Configuration for size limits
///
/// # Returns
///
/// A [`JarFile`] instance containing all entries, separated into classes and resources.
///
/// # Security
///
/// This function validates all paths for traversal attacks and enforces size limits.
pub fn load_jar(bytes: &[u8], config: &Config) -> Result<JarFile> {
    // Check overall size
    if bytes.len() as u64 > config.max_zip_size {
        return Err(Error::SizeLimitExceeded {
            message: format!(
                "JAR size {} bytes exceeds limit of {} bytes",
                bytes.len(),
                config.max_zip_size
            ),
        });
    }

    let cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor)?;

    // Check file count
    if archive.len() > config.max_file_count {
        return Err(Error::FileCountExceeded {
            found: archive.len(),
            limit: config.max_file_count,
        });
    }

    let mut classes = Vec::new();
    let mut resources = Vec::new();
    let mut total_uncompressed: u64 = 0;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();

        // Validate path for security
        validate_archive_path(&name)?;

        // Skip directories
        if entry.is_dir() {
            continue;
        }

        // Check individual file size
        if entry.size() > config.max_file_size {
            return Err(Error::SizeLimitExceeded {
                message: format!(
                    "File '{}' size {} bytes exceeds limit of {} bytes",
                    name,
                    entry.size(),
                    config.max_file_size
                ),
            });
        }

        // Track total uncompressed size for zip bomb detection
        total_uncompressed += entry.size();
        let compressed_size = entry.compressed_size().max(1); // Avoid division by zero
        let ratio = total_uncompressed as f64 / compressed_size as f64;
        if ratio > config.max_compression_ratio {
            return Err(Error::SecurityViolation(format!(
                "Possible zip bomb detected: compression ratio {:.1} exceeds limit of {:.1}",
                ratio, config.max_compression_ratio
            )));
        }

        // Read file content
        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut buf)?;

        if name.ends_with(".class") {
            classes.push((name, buf));
        } else {
            resources.push((name, buf));
        }
    }

    Ok(JarFile { classes, resources })
}

/// Writes a [`JarFile`] to bytes.
///
/// # Arguments
///
/// * `jar` - The JAR file to serialize
///
/// # Returns
///
/// The serialized JAR as a byte vector.
pub fn write_jar(jar: &JarFile) -> Result<Vec<u8>> {
    let mut buffer = Vec::new();
    {
        let cursor = Cursor::new(&mut buffer);
        let mut zip = ZipWriter::new(cursor);

        let options: FileOptions<'_, ()> = FileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .unix_permissions(0o644);

        // Write classes first, then resources
        for (name, bytes) in jar.classes.iter().chain(jar.resources.iter()) {
            zip.start_file(name, options)?;
            zip.write_all(bytes)?;
        }

        zip.finish()?;
    }

    Ok(buffer)
}

/// Processes a JAR file, replacing placeholders in class files and text resources.
///
/// This is the main entry point for single JAR processing.
///
/// # Arguments
///
/// * `jar_bytes` - The raw bytes of the input JAR
/// * `config` - Configuration including placeholders and limits
///
/// # Returns
///
/// The processed JAR as a byte vector.
///
/// # Example
///
/// ```no_run
/// use placy_core::{Config, process_jar};
///
/// # fn main() -> placy_core::Result<()> {
/// let config = Config::builder()
///     .add_placeholder("%%__USER__%%", "alice")
///     .build();
///
/// let input = std::fs::read("input.jar")?;
/// let output = process_jar(&input, &config)?;
/// std::fs::write("output.jar", output)?;
/// # Ok(())
/// # }
/// ```
pub fn process_jar(jar_bytes: &[u8], config: &Config) -> Result<Vec<u8>> {
    let mut jar = load_jar(jar_bytes, config)?;
    let placeholders = config.placeholder_refs();

    // Process class files in parallel
    jar.classes = jar
        .classes
        .into_par_iter()
        .map(|(path, bytes)| {
            let processed = process_class_bytes(&bytes, &placeholders)?;
            Ok((path, processed))
        })
        .collect::<Result<Vec<_>>>()?;

    // Process text resources in parallel
    jar.resources = jar
        .resources
        .into_par_iter()
        .map(|(path, bytes)| {
            // Check if this is a text file
            let is_text = path
                .rsplit('.')
                .next()
                .map(|ext| config.is_text_extension(ext))
                .unwrap_or(false);

            if is_text {
                if let Some(processed) = process_text_file(&bytes, &placeholders)? {
                    return Ok((path, processed));
                }
            }
            Ok((path, bytes))
        })
        .collect::<Result<Vec<_>>>()?;

    write_jar(&jar)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Creates a minimal valid JAR file in memory for testing.
    fn create_test_jar(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buffer = Vec::new();
        {
            let cursor = Cursor::new(&mut buffer);
            let mut zip = ZipWriter::new(cursor);

            let options: FileOptions<'_, ()> =
                FileOptions::default().compression_method(CompressionMethod::Stored);

            for (name, content) in files {
                zip.start_file(*name, options).unwrap();
                zip.write_all(content).unwrap();
            }

            zip.finish().unwrap();
        }
        buffer
    }

    #[test]
    fn test_load_jar_basic() {
        let jar_bytes = create_test_jar(&[
            ("META-INF/MANIFEST.MF", b"Manifest-Version: 1.0\n"),
            ("config.properties", b"user=%%__USER__%%\n"),
        ]);

        let config = Config::default();
        let jar = load_jar(&jar_bytes, &config).unwrap();

        assert_eq!(jar.classes.len(), 0);
        assert_eq!(jar.resources.len(), 2);
    }

    #[test]
    fn test_load_jar_path_traversal_blocked() {
        let jar_bytes = create_test_jar(&[("../escape.txt", b"malicious")]);

        let config = Config::default();
        let result = load_jar(&jar_bytes, &config);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::PathTraversal(_)));
    }

    #[test]
    fn test_load_jar_size_limit() {
        let large_content = vec![0u8; 1024 * 1024]; // 1 MB
        let jar_bytes = create_test_jar(&[("large.bin", &large_content)]);

        let config = Config::builder()
            .with_max_file_size(1024) // 1 KB limit
            .build();

        let result = load_jar(&jar_bytes, &config);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            Error::SizeLimitExceeded { .. }
        ));
    }

    #[test]
    fn test_load_jar_file_count_limit() {
        let files: Vec<(&str, &[u8])> = (0..10)
            .map(|i| {
                let name: &'static str = Box::leak(format!("file{i}.txt").into_boxed_str());
                (name, b"content" as &[u8])
            })
            .collect();

        let jar_bytes = create_test_jar(&files);

        let config = Config::builder().with_max_file_count(5).build();

        let result = load_jar(&jar_bytes, &config);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            Error::FileCountExceeded { .. }
        ));
    }

    #[test]
    fn test_write_jar_roundtrip() {
        let original = JarFile {
            classes: vec![],
            resources: vec![("test.txt".to_string(), b"hello".to_vec())],
        };

        let bytes = write_jar(&original).unwrap();
        let config = Config::default();
        let loaded = load_jar(&bytes, &config).unwrap();

        assert_eq!(loaded.resources.len(), 1);
        assert_eq!(loaded.resources[0].0, "test.txt");
        assert_eq!(loaded.resources[0].1, b"hello");
    }

    #[test]
    fn test_process_jar_text_replacement() {
        let jar_bytes = create_test_jar(&[(
            "config.properties",
            b"username=%%__USER__%%\nid=%%__ID__%%\n",
        )]);

        let config = Config::builder()
            .add_placeholder("%%__USER__%%", "alice")
            .add_placeholder("%%__ID__%%", "12345")
            .build();

        let output = process_jar(&jar_bytes, &config).unwrap();
        let result = load_jar(&output, &Config::default()).unwrap();

        let content = String::from_utf8(result.resources[0].1.clone()).unwrap();
        assert!(content.contains("username=alice"));
        assert!(content.contains("id=12345"));
        assert!(!content.contains("%%__USER__%%"));
    }
}

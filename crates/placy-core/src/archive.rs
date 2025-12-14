//! ZIP archive processing with regex-based file matching.
//!
//! This module handles ZIP archives containing multiple JAR files and resources.
//! It uses `process.txt` and `ignore.txt` for flexible file matching.

use crate::config::{parse_pattern_file, Config, FilePatterns};
use crate::error::{Error, Result};
use crate::jar::{load_jar, write_jar, JarFile};
use crate::security::validate_archive_path;
use crate::text::process_text_file;

use rayon::prelude::*;
use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use zip::{write::FileOptions, CompressionMethod, ZipArchive, ZipWriter};

/// Special file names for archive control.
const PROCESS_FILE: &str = "process.txt";
const IGNORE_FILE: &str = "ignore.txt";

/// Represents a file entry in an archive.
#[derive(Debug, Clone)]
struct ArchiveEntry {
    path: String,
    content: Vec<u8>,
    is_directory: bool,
}

/// Processes a ZIP archive containing JARs and other files.
///
/// This function implements the following logic:
///
/// 1. **process.txt**: Contains regex patterns for JAR files to process.
///    - If present, only JAR files matching at least one pattern are processed.
///    - If absent, all JAR files are processed.
///
/// 2. **ignore.txt**: Contains regex patterns for files to completely ignore.
///    - Matching files are excluded from the output entirely.
///
/// 3. **Non-JAR files**: All text files (based on extension) have placeholders replaced.
///
/// 4. **Security**: All paths are validated for traversal attacks.
///
/// # Arguments
///
/// * `archive_bytes` - The raw bytes of the input ZIP archive
/// * `config` - Configuration including placeholders and limits
///
/// # Returns
///
/// The processed ZIP archive as a byte vector.
///
/// # Example
///
/// ```no_run
/// use placy_core::{Config, process_archive};
///
/// # fn main() -> placy_core::Result<()> {
/// let config = Config::builder()
///     .add_placeholder("%%__USER__%%", "alice")
///     .add_placeholder("%%__NONCE__%%", "abc123")
///     .build();
///
/// let input = std::fs::read("input.zip")?;
/// let output = process_archive(&input, &config)?;
/// std::fs::write("output.zip", output)?;
/// # Ok(())
/// # }
/// ```
pub fn process_archive(archive_bytes: &[u8], config: &Config) -> Result<Vec<u8>> {
    // Validate archive size
    if archive_bytes.len() as u64 > config.max_zip_size {
        return Err(Error::SizeLimitExceeded {
            message: format!(
                "Archive size {} bytes exceeds limit of {} bytes",
                archive_bytes.len(),
                config.max_zip_size
            ),
        });
    }

    // Load the archive
    let cursor = Cursor::new(archive_bytes);
    let mut archive = ZipArchive::new(cursor)?;

    // Validate file count
    if archive.len() > config.max_file_count {
        return Err(Error::FileCountExceeded {
            found: archive.len(),
            limit: config.max_file_count,
        });
    }

    // First pass: extract all entries and find control files
    let mut entries: Vec<ArchiveEntry> = Vec::with_capacity(archive.len());
    let mut process_content: Option<String> = None;
    let mut ignore_content: Option<String> = None;
    let mut total_uncompressed: u64 = 0;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();

        // Validate path
        validate_archive_path(&name)?;

        // Check for zip bomb
        total_uncompressed += entry.size();
        let compressed_size = entry.compressed_size().max(1);
        let ratio = total_uncompressed as f64 / compressed_size as f64;
        if ratio > config.max_compression_ratio {
            return Err(Error::SecurityViolation(format!(
                "Possible zip bomb: compression ratio {:.1} exceeds limit {:.1}",
                ratio, config.max_compression_ratio
            )));
        }

        if entry.is_dir() {
            entries.push(ArchiveEntry {
                path: name,
                content: Vec::new(),
                is_directory: true,
            });
            continue;
        }

        // Check file size
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

        // Read content
        let mut content = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut content)?;

        // Check for control files
        let normalized_name = normalize_path(&name);
        if normalized_name == PROCESS_FILE {
            process_content =
                Some(
                    String::from_utf8(content.clone()).map_err(|e| Error::InvalidUtf8 {
                        context: PROCESS_FILE.to_string(),
                        details: e.to_string(),
                    })?,
                );
        } else if normalized_name == IGNORE_FILE {
            ignore_content =
                Some(
                    String::from_utf8(content.clone()).map_err(|e| Error::InvalidUtf8 {
                        context: IGNORE_FILE.to_string(),
                        details: e.to_string(),
                    })?,
                );
        }

        entries.push(ArchiveEntry {
            path: name,
            content,
            is_directory: false,
        });
    }

    // Parse patterns
    let process_patterns = process_content
        .as_ref()
        .map(|c| parse_pattern_file(c))
        .unwrap_or_default();
    let ignore_patterns = ignore_content
        .as_ref()
        .map(|c| parse_pattern_file(c))
        .unwrap_or_default();

    let patterns = FilePatterns::new(&process_patterns, &ignore_patterns)?;

    // Process entries
    let placeholders = config.placeholder_refs();
    let processed_entries: Vec<ArchiveEntry> = entries
        .into_par_iter()
        .filter_map(|entry| process_entry(entry, &patterns, &placeholders, config).transpose())
        .collect::<Result<Vec<_>>>()?;

    // Write output archive
    write_archive(&processed_entries, config)
}

/// Normalizes a path for comparison (removes leading ./ and handles root entries).
fn normalize_path(path: &str) -> &str {
    let path = path.trim_start_matches("./");
    // Handle paths that might be in subdirectories
    path.rsplit('/').next().unwrap_or(path)
}

/// Processes a single archive entry.
///
/// Returns:
/// - `Ok(Some(entry))` - Entry should be included (possibly modified)
/// - `Ok(None)` - Entry should be excluded (matched ignore pattern or is control file)
/// - `Err(e)` - Processing error
fn process_entry(
    mut entry: ArchiveEntry,
    patterns: &FilePatterns,
    placeholders: &HashMap<&str, &str>,
    config: &Config,
) -> Result<Option<ArchiveEntry>> {
    // Skip directories
    if entry.is_directory {
        return Ok(Some(entry));
    }

    let normalized = normalize_path(&entry.path);

    // Remove control files from output if configured
    if (normalized == PROCESS_FILE && config.delete_process_file)
        || (normalized == IGNORE_FILE && config.delete_ignore_file)
    {
        return Ok(None);
    }

    // Check ignore patterns
    if patterns.should_ignore(&entry.path) {
        return Ok(None);
    }

    // Check if it's a JAR file
    if entry.path.ends_with(".jar") {
        // Check if we should process this JAR
        if patterns.should_process_jar(&entry.path) {
            // Process the JAR
            let jar = load_jar(&entry.content, config)?;
            let processed_jar = process_jar_internal(jar, placeholders, config)?;
            entry.content = write_jar(&processed_jar)?;
        }
        // If not processing, keep the JAR as-is
        return Ok(Some(entry));
    }

    // Check if it's a text file
    let extension = entry.path.rsplit('.').next().unwrap_or("");

    if config.is_text_extension(extension) {
        if let Some(processed) = process_text_file(&entry.content, placeholders)? {
            entry.content = processed;
        }
    }

    Ok(Some(entry))
}

/// Internal JAR processing without re-serializing.
fn process_jar_internal(
    mut jar: JarFile,
    placeholders: &HashMap<&str, &str>,
    config: &Config,
) -> Result<JarFile> {
    use crate::processor::process_class_bytes;

    // Process class files in parallel
    jar.classes = jar
        .classes
        .into_par_iter()
        .map(|(path, bytes)| {
            let processed = process_class_bytes(&bytes, placeholders)?;
            Ok((path, processed))
        })
        .collect::<Result<Vec<_>>>()?;

    // Process text resources in parallel
    jar.resources = jar
        .resources
        .into_par_iter()
        .map(|(path, bytes)| {
            let extension = path.rsplit('.').next().unwrap_or("");
            if config.is_text_extension(extension) {
                if let Some(processed) = process_text_file(&bytes, placeholders)? {
                    return Ok((path, processed));
                }
            }
            Ok((path, bytes))
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(jar)
}

/// Writes processed entries to a new ZIP archive.
fn write_archive(entries: &[ArchiveEntry], _config: &Config) -> Result<Vec<u8>> {
    let mut buffer = Vec::new();
    {
        let cursor = Cursor::new(&mut buffer);
        let mut zip = ZipWriter::new(cursor);

        let file_options: FileOptions<'_, ()> = FileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o644);

        let dir_options: FileOptions<'_, ()> = FileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .unix_permissions(0o755);

        for entry in entries {
            if entry.is_directory {
                zip.add_directory(&entry.path, dir_options)?;
            } else {
                zip.start_file(&entry.path, file_options)?;
                zip.write_all(&entry.content)?;
            }
        }

        zip.finish()?;
    }

    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Creates a test ZIP archive.
    fn create_test_archive(files: &[(&str, &[u8])]) -> Vec<u8> {
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

    /// Extracts files from an archive for verification.
    fn extract_archive(bytes: &[u8]) -> HashMap<String, Vec<u8>> {
        let cursor = Cursor::new(bytes);
        let mut archive = ZipArchive::new(cursor).unwrap();
        let mut files = HashMap::new();

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).unwrap();
            if !entry.is_dir() {
                let mut content = Vec::new();
                entry.read_to_end(&mut content).unwrap();
                files.insert(entry.name().to_string(), content);
            }
        }

        files
    }

    #[test]
    fn test_process_archive_basic() {
        let archive = create_test_archive(&[
            ("config.properties", b"user=%%__USER__%%\n"),
            ("readme.txt", b"Hello %%__USER__%%!\n"),
        ]);

        let config = Config::builder()
            .add_placeholder("%%__USER__%%", "alice")
            .build();

        let output = process_archive(&archive, &config).unwrap();
        let files = extract_archive(&output);

        let config_content = String::from_utf8(files["config.properties"].clone()).unwrap();
        assert!(config_content.contains("user=alice"));
        assert!(!config_content.contains("%%__USER__%%"));
    }

    #[test]
    fn test_process_archive_with_process_txt() {
        // Create a simple JAR-like file (just a text file with .jar extension for testing)
        let jar1 = create_test_archive(&[("config.txt", b"user=%%__USER__%%\n")]);
        let jar2 = create_test_archive(&[("config.txt", b"user=%%__USER__%%\n")]);

        let archive = create_test_archive(&[
            ("process.txt", b".*plugin.*\\.jar\n"),
            ("libs/plugin-core.jar", &jar1),
            ("libs/utils.jar", &jar2),
        ]);

        let config = Config::builder()
            .add_placeholder("%%__USER__%%", "alice")
            .build();

        let output = process_archive(&archive, &config).unwrap();
        let files = extract_archive(&output);

        // process.txt should be removed
        assert!(!files.contains_key("process.txt"));

        // Both JARs should be present
        assert!(files.contains_key("libs/plugin-core.jar"));
        assert!(files.contains_key("libs/utils.jar"));
    }

    #[test]
    fn test_process_archive_with_ignore_txt() {
        let archive = create_test_archive(&[
            ("ignore.txt", b".*\\.log\n.*temp.*\n"),
            ("config.txt", b"user=%%__USER__%%\n"),
            ("debug.log", b"some log content"),
            ("temp/cache.dat", b"cached data"),
        ]);

        let config = Config::builder()
            .add_placeholder("%%__USER__%%", "alice")
            .build();

        let output = process_archive(&archive, &config).unwrap();
        let files = extract_archive(&output);

        // ignore.txt and matching files should be removed
        assert!(!files.contains_key("ignore.txt"));
        assert!(!files.contains_key("debug.log"));
        assert!(!files.contains_key("temp/cache.dat"));

        // config.txt should be present and processed
        assert!(files.contains_key("config.txt"));
        let content = String::from_utf8(files["config.txt"].clone()).unwrap();
        assert!(content.contains("user=alice"));
    }

    #[test]
    fn test_process_archive_path_traversal_blocked() {
        let archive = create_test_archive(&[("../escape.txt", b"malicious")]);

        let config = Config::default();
        let result = process_archive(&archive, &config);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::PathTraversal(_)));
    }

    #[test]
    fn test_process_archive_size_limits() {
        let large_content = vec![0u8; 1024 * 1024]; // 1 MB
        let archive = create_test_archive(&[("large.bin", &large_content)]);

        let config = Config::builder()
            .with_max_file_size(1024) // 1 KB limit
            .build();

        let result = process_archive(&archive, &config);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            Error::SizeLimitExceeded { .. }
        ));
    }

    #[test]
    fn test_normalize_path() {
        assert_eq!(normalize_path("process.txt"), "process.txt");
        assert_eq!(normalize_path("./process.txt"), "process.txt");
        assert_eq!(normalize_path("foo/process.txt"), "process.txt");
        assert_eq!(normalize_path("foo/bar/process.txt"), "process.txt");
    }
}

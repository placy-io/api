//! Configuration for JAR/ZIP processing.

use crate::error::{Error, Result};
use regex::Regex;
use std::collections::HashMap;

/// Default file extensions to process as text files.
pub const DEFAULT_TEXT_EXTENSIONS: &[&str] = &[
    "txt",
    "xml",
    "json",
    "properties",
    "yml",
    "yaml",
    "conf",
    "cfg",
    "js",
    "ts",
    "tsx",
    "jsx",
    "html",
    "htm",
    "css",
    "scss",
    "less",
    "md",
    "markdown",
    "ini",
    "toml",
];

/// Default maximum ZIP file size (100 MB).
pub const DEFAULT_MAX_ZIP_SIZE: u64 = 100 * 1024 * 1024;

/// Default maximum individual file size (10 MB).
pub const DEFAULT_MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Default maximum number of files in an archive.
pub const DEFAULT_MAX_FILE_COUNT: usize = 1000;

/// Default maximum compression ratio to detect zip bombs.
pub const DEFAULT_MAX_COMPRESSION_RATIO: f64 = 100.0;

/// Configuration for placeholder replacement processing.
#[derive(Debug, Clone)]
pub struct Config {
    /// Placeholder key-value pairs for replacement.
    pub(crate) placeholders: HashMap<String, String>,

    /// File extensions to treat as text files.
    pub(crate) text_extensions: Vec<String>,

    /// Maximum ZIP archive size in bytes.
    pub(crate) max_zip_size: u64,

    /// Maximum individual file size in bytes.
    pub(crate) max_file_size: u64,

    /// Maximum number of files allowed in an archive.
    pub(crate) max_file_count: usize,

    /// Maximum compression ratio (uncompressed/compressed) to detect zip bombs.
    pub(crate) max_compression_ratio: f64,

    /// Whether to delete process.txt after processing.
    pub(crate) delete_process_file: bool,

    /// Whether to delete ignore.txt after processing.
    pub(crate) delete_ignore_file: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            placeholders: HashMap::new(),
            text_extensions: DEFAULT_TEXT_EXTENSIONS
                .iter()
                .map(|s| s.to_string())
                .collect(),
            max_zip_size: DEFAULT_MAX_ZIP_SIZE,
            max_file_size: DEFAULT_MAX_FILE_SIZE,
            max_file_count: DEFAULT_MAX_FILE_COUNT,
            max_compression_ratio: DEFAULT_MAX_COMPRESSION_RATIO,
            delete_process_file: true,
            delete_ignore_file: true,
        }
    }
}

impl Config {
    /// Creates a new ConfigBuilder.
    pub fn builder() -> ConfigBuilder {
        ConfigBuilder::default()
    }

    /// Returns the placeholders map.
    pub fn placeholders(&self) -> &HashMap<String, String> {
        &self.placeholders
    }

    /// Returns the text file extensions.
    pub fn text_extensions(&self) -> &[String] {
        &self.text_extensions
    }

    /// Checks if a file extension should be treated as text.
    pub fn is_text_extension(&self, ext: &str) -> bool {
        self.text_extensions
            .iter()
            .any(|e| e.eq_ignore_ascii_case(ext))
    }

    /// Creates a HashMap of placeholder references for processing.
    pub fn placeholder_refs(&self) -> HashMap<&str, &str> {
        self.placeholders
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect()
    }
}

/// Builder for [`Config`].
#[derive(Debug, Default)]
pub struct ConfigBuilder {
    placeholders: HashMap<String, String>,
    text_extensions: Option<Vec<String>>,
    max_zip_size: Option<u64>,
    max_file_size: Option<u64>,
    max_file_count: Option<usize>,
    max_compression_ratio: Option<f64>,
    delete_process_file: Option<bool>,
    delete_ignore_file: Option<bool>,
}

impl ConfigBuilder {
    /// Creates a new ConfigBuilder with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a placeholder replacement.
    pub fn add_placeholder(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.placeholders.insert(key.into(), value.into());
        self
    }

    /// Sets multiple placeholders at once.
    pub fn with_placeholders(mut self, placeholders: HashMap<String, String>) -> Self {
        self.placeholders = placeholders;
        self
    }

    /// Sets the text file extensions.
    pub fn with_text_extensions(mut self, extensions: Vec<String>) -> Self {
        self.text_extensions = Some(extensions);
        self
    }

    /// Sets the maximum ZIP size in bytes.
    pub fn with_max_zip_size(mut self, size: u64) -> Self {
        self.max_zip_size = Some(size);
        self
    }

    /// Sets the maximum individual file size in bytes.
    pub fn with_max_file_size(mut self, size: u64) -> Self {
        self.max_file_size = Some(size);
        self
    }

    /// Sets the maximum file count.
    pub fn with_max_file_count(mut self, count: usize) -> Self {
        self.max_file_count = Some(count);
        self
    }

    /// Sets the maximum compression ratio.
    pub fn with_max_compression_ratio(mut self, ratio: f64) -> Self {
        self.max_compression_ratio = Some(ratio);
        self
    }

    /// Sets whether to delete process.txt after processing.
    pub fn with_delete_process_file(mut self, delete: bool) -> Self {
        self.delete_process_file = Some(delete);
        self
    }

    /// Sets whether to delete ignore.txt after processing.
    pub fn with_delete_ignore_file(mut self, delete: bool) -> Self {
        self.delete_ignore_file = Some(delete);
        self
    }

    /// Builds the Config.
    pub fn build(self) -> Config {
        let default = Config::default();
        Config {
            placeholders: self.placeholders,
            text_extensions: self.text_extensions.unwrap_or(default.text_extensions),
            max_zip_size: self.max_zip_size.unwrap_or(default.max_zip_size),
            max_file_size: self.max_file_size.unwrap_or(default.max_file_size),
            max_file_count: self.max_file_count.unwrap_or(default.max_file_count),
            max_compression_ratio: self
                .max_compression_ratio
                .unwrap_or(default.max_compression_ratio),
            delete_process_file: self
                .delete_process_file
                .unwrap_or(default.delete_process_file),
            delete_ignore_file: self
                .delete_ignore_file
                .unwrap_or(default.delete_ignore_file),
        }
    }
}

/// Compiled regex patterns for file matching.
#[derive(Debug, Default)]
pub struct FilePatterns {
    /// Patterns for files to process (from process.txt).
    pub process_patterns: Vec<Regex>,
    /// Patterns for files to ignore (from ignore.txt).
    pub ignore_patterns: Vec<Regex>,
}

impl FilePatterns {
    /// Creates a new FilePatterns from raw pattern strings.
    pub fn new(process_patterns: &[String], ignore_patterns: &[String]) -> Result<Self> {
        let process_patterns = compile_patterns(process_patterns)?;
        let ignore_patterns = compile_patterns(ignore_patterns)?;

        Ok(Self {
            process_patterns,
            ignore_patterns,
        })
    }

    /// Checks if a JAR file path should be processed.
    ///
    /// A file is processed if:
    /// 1. It matches at least one process pattern (or there are no process patterns)
    /// 2. It does NOT match any ignore pattern
    pub fn should_process_jar(&self, path: &str) -> bool {
        // Check ignore patterns first
        if self.ignore_patterns.iter().any(|p| p.is_match(path)) {
            return false;
        }

        // If no process patterns, process all non-ignored JARs
        if self.process_patterns.is_empty() {
            return true;
        }

        // Otherwise, must match at least one process pattern
        self.process_patterns.iter().any(|p| p.is_match(path))
    }

    /// Checks if a file should be ignored entirely.
    pub fn should_ignore(&self, path: &str) -> bool {
        self.ignore_patterns.iter().any(|p| p.is_match(path))
    }
}

/// Compiles a list of pattern strings into regex objects.
fn compile_patterns(patterns: &[String]) -> Result<Vec<Regex>> {
    patterns
        .iter()
        .filter(|p| !p.trim().is_empty() && !p.trim().starts_with('#'))
        .map(|p| {
            Regex::new(p.trim()).map_err(|e| Error::InvalidRegex {
                pattern: p.clone(),
                source: e,
            })
        })
        .collect()
}

/// Parses a file content into a list of patterns (one per line).
pub fn parse_pattern_file(content: &str) -> Vec<String> {
    content
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

/// Parses a size string like "100MB" into bytes.
pub fn parse_size(size_str: &str) -> Result<u64> {
    let size_str = size_str.trim().to_uppercase();

    let (number_part, multiplier) = if let Some(num) = size_str.strip_suffix("GB") {
        (num, 1024u64 * 1024 * 1024)
    } else if let Some(num) = size_str.strip_suffix("MB") {
        (num, 1024u64 * 1024)
    } else if let Some(num) = size_str.strip_suffix("KB") {
        (num, 1024u64)
    } else if let Some(num) = size_str.strip_suffix('B') {
        (num, 1u64)
    } else {
        return Err(Error::Config(format!(
            "Invalid size format '{size_str}'. Use B, KB, MB, or GB suffix."
        )));
    };

    let number: u64 = number_part
        .trim()
        .parse()
        .map_err(|_| Error::Config(format!("Invalid number in size: {size_str}")))?;

    Ok(number * multiplier)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_builder() {
        let config = Config::builder()
            .add_placeholder("%%__USER__%%", "test")
            .add_placeholder("%%__TIME__%%", "123")
            .with_max_file_count(500)
            .build();

        assert_eq!(config.placeholders.len(), 2);
        assert_eq!(
            config.placeholders.get("%%__USER__%%"),
            Some(&"test".to_string())
        );
        assert_eq!(config.max_file_count, 500);
    }

    #[test]
    fn test_is_text_extension() {
        let config = Config::default();
        assert!(config.is_text_extension("txt"));
        assert!(config.is_text_extension("TXT"));
        assert!(config.is_text_extension("json"));
        assert!(!config.is_text_extension("class"));
        assert!(!config.is_text_extension("jar"));
    }

    #[test]
    fn test_file_patterns() {
        let patterns =
            FilePatterns::new(&[".*plugin.*\\.jar".to_string()], &[".*test.*".to_string()])
                .unwrap();

        assert!(patterns.should_process_jar("libs/plugin-core.jar"));
        assert!(!patterns.should_process_jar("libs/utils.jar"));
        assert!(!patterns.should_process_jar("libs/plugin-test.jar")); // matches ignore
    }

    #[test]
    fn test_file_patterns_empty_process() {
        let patterns = FilePatterns::new(&[], &[".*test.*".to_string()]).unwrap();

        // With no process patterns, all non-ignored files are processed
        assert!(patterns.should_process_jar("libs/plugin.jar"));
        assert!(!patterns.should_process_jar("test-utils.jar"));
    }

    #[test]
    fn test_parse_pattern_file() {
        let content = r#"
            # This is a comment
            .*plugin.*\.jar
            
            .*core.*\.jar
            # Another comment
        "#;

        let patterns = parse_pattern_file(content);
        assert_eq!(patterns.len(), 2);
        assert_eq!(patterns[0], ".*plugin.*\\.jar");
        assert_eq!(patterns[1], ".*core.*\\.jar");
    }

    #[test]
    fn test_parse_size() {
        assert_eq!(parse_size("100MB").unwrap(), 100 * 1024 * 1024);
        assert_eq!(parse_size("10KB").unwrap(), 10 * 1024);
        assert_eq!(parse_size("1GB").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(parse_size("500B").unwrap(), 500);
        assert!(parse_size("invalid").is_err());
    }
}

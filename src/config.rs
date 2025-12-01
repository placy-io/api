use anyhow::{Context, Result, anyhow};
use std::collections::HashMap;
use std::path::PathBuf;

/// Default file extensions to process as text files
pub const DEFAULT_TEXT_FILE_EXTENSIONS: &[&str] = &[
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
];

/// Configuration for the JAR/ZIP processing application
#[derive(Debug, Clone)]
pub struct Config {
    /// Path to the input file (JAR or ZIP)
    pub input_path: PathBuf,

    /// Path to the output file
    pub output_path: PathBuf,

    /// Placeholder replacements (key -> value)
    pub placeholders: HashMap<String, String>,

    /// Whether to delete process.txt after processing (for ZIP workflow)
    pub delete_process_file: bool,

    /// Maximum number of files allowed in the ZIP
    pub maximum_allowed_files: usize,

    /// Maximum ZIP file size in bytes
    pub maximum_zip_size: u64,

    /// Maximum individual file size in bytes
    pub maximum_file_size: u64,

    /// Temporary extraction directory for ZIP workflow
    pub temp_extract_dir: PathBuf,
}

impl Config {
    /// Creates a new Config with the given input path
    pub fn new(input_path: PathBuf) -> Self {
        Self {
            output_path: Self::default_output_path(&input_path),
            input_path,
            placeholders: HashMap::new(),
            delete_process_file: true,
            maximum_allowed_files: 20,
            maximum_zip_size: 100 * 1024 * 1024, // 100 MB
            maximum_file_size: 10 * 1024 * 1024, // 10 MB
            temp_extract_dir: PathBuf::from("extracted_temp"),
        }
    }

    /// Determines the default output path based on input path
    fn default_output_path(input_path: &PathBuf) -> PathBuf {
        if input_path.extension().and_then(|s| s.to_str()) == Some("zip") {
            PathBuf::from("output.zip")
        } else {
            PathBuf::from("output.jar")
        }
    }

    /// Sets the output path
    pub fn with_output_path(mut self, output_path: PathBuf) -> Self {
        self.output_path = output_path;
        self
    }

    /// Sets the placeholders
    pub fn with_placeholders(mut self, placeholders: HashMap<String, String>) -> Self {
        self.placeholders = placeholders;
        self
    }

    /// Adds a single placeholder
    pub fn add_placeholder(mut self, key: String, value: String) -> Self {
        self.placeholders.insert(key, value);
        self
    }

    /// Sets whether to delete process.txt
    pub fn with_delete_process_file(mut self, delete: bool) -> Self {
        self.delete_process_file = delete;
        self
    }

    /// Sets the maximum allowed files
    pub fn with_maximum_allowed_files(mut self, max: usize) -> Self {
        self.maximum_allowed_files = max;
        self
    }

    /// Sets the maximum ZIP size in bytes
    pub fn with_maximum_zip_size(mut self, max: u64) -> Self {
        self.maximum_zip_size = max;
        self
    }

    /// Sets the maximum file size in bytes
    pub fn with_maximum_file_size(mut self, max: u64) -> Self {
        self.maximum_file_size = max;
        self
    }

    /// Sets the temporary extraction directory
    pub fn with_temp_extract_dir(mut self, dir: PathBuf) -> Self {
        self.temp_extract_dir = dir;
        self
    }

    /// Validates the configuration
    pub fn validate(&self) -> Result<()> {
        // Check if input file exists
        if !self.input_path.exists() {
            return Err(anyhow!(
                "Input file does not exist: {}",
                self.input_path.display()
            ));
        }

        // Check file extension
        let extension = self
            .input_path
            .extension()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("Input file has no extension"))?;

        if extension != "jar" && extension != "zip" {
            return Err(anyhow!(
                "Input file must be a .jar or .zip file, got: .{}",
                extension
            ));
        }

        // Check input file size
        let metadata =
            std::fs::metadata(&self.input_path).context("Failed to read input file metadata")?;

        if extension == "zip" && metadata.len() > self.maximum_zip_size {
            return Err(anyhow!(
                "ZIP file size ({} bytes) exceeds maximum allowed size ({} bytes)",
                metadata.len(),
                self.maximum_zip_size
            ));
        }

        // Validate limits
        if self.maximum_allowed_files == 0 {
            return Err(anyhow!("maximum_allowed_files must be greater than 0"));
        }

        if self.maximum_zip_size == 0 {
            return Err(anyhow!("maximum_zip_size must be greater than 0"));
        }

        if self.maximum_file_size == 0 {
            return Err(anyhow!("maximum_file_size must be greater than 0"));
        }

        // Validate output path
        if let Some(parent) = self.output_path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                return Err(anyhow!(
                    "Output directory does not exist: {}",
                    parent.display()
                ));
            }
        }

        Ok(())
    }

    /// Returns true if the input is a ZIP file
    pub fn is_zip_workflow(&self) -> bool {
        self.input_path
            .extension()
            .and_then(|s| s.to_str())
            .map(|ext| ext == "zip")
            .unwrap_or(false)
    }

    /// Returns true if the input is a JAR file
    pub fn is_jar_workflow(&self) -> bool {
        self.input_path
            .extension()
            .and_then(|s| s.to_str())
            .map(|ext| ext == "jar")
            .unwrap_or(false)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            input_path: PathBuf::from("input.jar"),
            output_path: PathBuf::from("output.jar"),
            placeholders: HashMap::new(),
            delete_process_file: true,
            maximum_allowed_files: 20,
            maximum_zip_size: 100 * 1024 * 1024, // 100 MB
            maximum_file_size: 10 * 1024 * 1024, // 10 MB
            temp_extract_dir: PathBuf::from("extracted_temp"),
        }
    }
}

/// Helper function to parse size strings like "100MB", "10MB" into bytes
pub fn parse_size(size_str: &str) -> Result<u64> {
    let size_str = size_str.trim().to_uppercase();

    let (number_part, unit) = if size_str.ends_with("KB") {
        (size_str.trim_end_matches("KB"), 1024u64)
    } else if size_str.ends_with("MB") {
        (size_str.trim_end_matches("MB"), 1024u64 * 1024)
    } else if size_str.ends_with("GB") {
        (size_str.trim_end_matches("GB"), 1024u64 * 1024 * 1024)
    } else {
        return Err(anyhow!(
            "Invalid size format: {}. Use KB, MB, or GB",
            size_str
        ));
    };

    let number: u64 = number_part
        .trim()
        .parse()
        .with_context(|| format!("Failed to parse number from: {}", size_str))?;

    Ok(number * unit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_size() {
        assert_eq!(parse_size("100MB").unwrap(), 100 * 1024 * 1024);
        assert_eq!(parse_size("10MB").unwrap(), 10 * 1024 * 1024);
        assert_eq!(parse_size("1KB").unwrap(), 1024);
        assert_eq!(parse_size("1GB").unwrap(), 1024 * 1024 * 1024);
    }

    #[test]
    fn test_config_builder() {
        let config = Config::new(PathBuf::from("test.jar"))
            .with_maximum_allowed_files(50)
            .with_maximum_zip_size(200 * 1024 * 1024);

        assert_eq!(config.maximum_allowed_files, 50);
        assert_eq!(config.maximum_zip_size, 200 * 1024 * 1024);
    }
}

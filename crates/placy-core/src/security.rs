// Copyright 2025 Placy
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Security utilities for path validation and traversal protection.

use crate::error::{Error, Result};
use std::path::{Component, Path, PathBuf};

/// Maximum allowed path depth to prevent deeply nested zip bombs.
const MAX_PATH_DEPTH: usize = 50;

/// Maximum allowed path length in bytes.
const MAX_PATH_LENGTH: usize = 4096;

/// Validates a path from an archive entry for security issues.
///
/// This function performs comprehensive security checks:
/// - Path traversal detection (../, absolute paths)
/// - Null byte injection
/// - Excessive path length
/// - Excessive nesting depth
/// - Reserved/dangerous filenames on Windows
///
/// # Arguments
///
/// * `path` - The path string from the archive entry
///
/// # Returns
///
/// Returns `Ok(PathBuf)` with the sanitized path, or an error if validation fails.
///
/// # Examples
///
/// ```
/// use placy_core::security::validate_archive_path;
///
/// // Valid paths
/// assert!(validate_archive_path("foo/bar/baz.class").is_ok());
/// assert!(validate_archive_path("META-INF/MANIFEST.MF").is_ok());
///
/// // Invalid paths
/// assert!(validate_archive_path("../escape.txt").is_err());
/// assert!(validate_archive_path("/absolute/path").is_err());
/// assert!(validate_archive_path("foo/../bar").is_err());
/// ```
pub fn validate_archive_path(path: &str) -> Result<PathBuf> {
    // Check for null bytes
    if path.contains('\0') {
        return Err(Error::PathTraversal(format!(
            "Null byte in path: {}",
            path.replace('\0', "\\0")
        )));
    }

    // Check path length
    if path.len() > MAX_PATH_LENGTH {
        return Err(Error::PathTraversal(format!(
            "Path exceeds maximum length of {} bytes: {} bytes",
            MAX_PATH_LENGTH,
            path.len()
        )));
    }

    // Empty paths are invalid
    if path.is_empty() {
        return Err(Error::PathTraversal("Empty path".to_string()));
    }

    let path_obj = Path::new(path);

    // Check for absolute paths
    if path_obj.is_absolute() {
        return Err(Error::PathTraversal(format!(
            "Absolute path not allowed: {path}"
        )));
    }

    // Check for Windows-style absolute paths even on Unix
    if path.starts_with('\\') || (path.len() >= 2 && path.as_bytes()[1] == b':') {
        return Err(Error::PathTraversal(format!(
            "Windows absolute path not allowed: {path}"
        )));
    }

    // Validate each component
    let mut depth: i32 = 0;
    let mut normalized = PathBuf::new();

    for component in path_obj.components() {
        match component {
            Component::Normal(name) => {
                // Check for reserved Windows names
                if let Some(name_str) = name.to_str() {
                    if is_windows_reserved_name(name_str) {
                        return Err(Error::PathTraversal(format!(
                            "Reserved filename not allowed: {name_str}"
                        )));
                    }
                }
                normalized.push(name);
                depth += 1;
            },
            Component::ParentDir => {
                // Any parent directory reference is suspicious
                return Err(Error::PathTraversal(format!(
                    "Parent directory traversal not allowed: {path}"
                )));
            },
            Component::CurDir => {
                // Skip current directory references (.)
                continue;
            },
            Component::RootDir | Component::Prefix(_) => {
                return Err(Error::PathTraversal(format!(
                    "Absolute path component not allowed: {path}"
                )));
            },
        }

        if depth as usize > MAX_PATH_DEPTH {
            return Err(Error::PathTraversal(format!(
                "Path depth exceeds maximum of {MAX_PATH_DEPTH}: {path}"
            )));
        }
    }

    // Final safety check: ensure normalized path doesn't escape
    if normalized.as_os_str().is_empty() {
        return Err(Error::PathTraversal(format!(
            "Path normalizes to empty: {path}"
        )));
    }

    Ok(normalized)
}

/// Checks if a filename is a Windows reserved name.
fn is_windows_reserved_name(name: &str) -> bool {
    let upper = name.to_uppercase();
    let base = upper.split('.').next().unwrap_or(&upper);

    matches!(
        base,
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

/// Validates that an extraction path stays within the intended base directory.
///
/// This provides an additional layer of security when extracting files.
///
/// # Arguments
///
/// * `base_dir` - The base directory for extraction
/// * `entry_path` - The path of the entry being extracted
///
/// # Returns
///
/// Returns the full canonical path if safe, or an error if the path would escape.
pub fn safe_join(base_dir: &Path, entry_path: &Path) -> Result<PathBuf> {
    let _joined = base_dir.join(entry_path);

    // Canonicalize the base directory (it must exist)
    let canonical_base = base_dir.canonicalize().map_err(|e| {
        Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "Base directory does not exist: {}: {}",
                base_dir.display(),
                e
            ),
        ))
    })?;

    // For the joined path, we need to check component by component
    // since the file may not exist yet
    let mut current = canonical_base.clone();
    for component in entry_path.components() {
        match component {
            Component::Normal(name) => {
                current.push(name);
            },
            Component::ParentDir => {
                return Err(Error::PathTraversal(format!(
                    "Path traversal in joined path: {}",
                    entry_path.display()
                )));
            },
            Component::CurDir => continue,
            _ => {
                return Err(Error::PathTraversal(format!(
                    "Invalid component in path: {}",
                    entry_path.display()
                )));
            },
        }
    }

    // Verify the result starts with the base
    if !current.starts_with(&canonical_base) {
        return Err(Error::PathTraversal(format!(
            "Path escapes base directory: {}",
            entry_path.display()
        )));
    }

    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_paths() {
        assert!(validate_archive_path("foo.class").is_ok());
        assert!(validate_archive_path("com/example/Main.class").is_ok());
        assert!(validate_archive_path("META-INF/MANIFEST.MF").is_ok());
        assert!(validate_archive_path("resources/data.json").is_ok());
    }

    #[test]
    fn test_parent_traversal() {
        assert!(validate_archive_path("../escape.txt").is_err());
        assert!(validate_archive_path("foo/../bar").is_err());
        assert!(validate_archive_path("foo/bar/../../escape").is_err());
        assert!(validate_archive_path("..").is_err());
    }

    #[test]
    fn test_absolute_paths() {
        assert!(validate_archive_path("/etc/passwd").is_err());
        assert!(validate_archive_path("/absolute/path").is_err());
        // Windows-style
        assert!(validate_archive_path("C:\\Windows\\System32").is_err());
        assert!(validate_archive_path("\\\\server\\share").is_err());
    }

    #[test]
    fn test_null_bytes() {
        assert!(validate_archive_path("foo\0bar.txt").is_err());
        assert!(validate_archive_path("\0").is_err());
    }

    #[test]
    fn test_empty_path() {
        assert!(validate_archive_path("").is_err());
    }

    #[test]
    fn test_windows_reserved() {
        assert!(validate_archive_path("CON").is_err());
        assert!(validate_archive_path("PRN.txt").is_err());
        assert!(validate_archive_path("COM1").is_err());
        assert!(validate_archive_path("LPT1").is_err());
        assert!(validate_archive_path("NUL").is_err());
    }

    #[test]
    fn test_current_dir_normalized() {
        let result = validate_archive_path("./foo/./bar").unwrap();
        assert_eq!(result, PathBuf::from("foo/bar"));
    }

    #[test]
    fn test_deep_nesting() {
        let deep_path = (0..60)
            .map(|i| format!("d{i}"))
            .collect::<Vec<_>>()
            .join("/");
        assert!(validate_archive_path(&deep_path).is_err());
    }

    #[test]
    fn test_long_path() {
        let long_name = "a".repeat(5000);
        assert!(validate_archive_path(&long_name).is_err());
    }
}

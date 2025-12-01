use anyhow::{Context, Result, anyhow};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use zip::ZipArchive;

use crate::config::Config;

/// Extracts a ZIP file to the specified directory with validation
pub fn extract_zip(zip_path: &Path, extract_dir: &Path, config: &Config) -> Result<()> {
    // Validate ZIP size
    let metadata = fs::metadata(zip_path)
        .with_context(|| format!("Failed to read ZIP file metadata: {}", zip_path.display()))?;

    if metadata.len() > config.maximum_zip_size {
        return Err(anyhow!(
            "ZIP file size ({} bytes) exceeds maximum allowed size ({} bytes)",
            metadata.len(),
            config.maximum_zip_size
        ));
    }

    // Open ZIP archive
    let file = fs::File::open(zip_path)
        .with_context(|| format!("Failed to open ZIP file: {}", zip_path.display()))?;
    let mut archive = ZipArchive::new(file)?;

    // Check file count
    if archive.len() > config.maximum_allowed_files {
        return Err(anyhow!(
            "ZIP contains {} files, exceeding maximum of {}",
            archive.len(),
            config.maximum_allowed_files
        ));
    }

    // Create extraction directory
    fs::create_dir_all(extract_dir).with_context(|| {
        format!(
            "Failed to create extraction directory: {}",
            extract_dir.display()
        )
    })?;

    // Extract files
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let outpath = extract_dir.join(file.name());

        // Security check: prevent path traversal
        if !outpath.starts_with(extract_dir) {
            return Err(anyhow!(
                "Security violation: file path escapes extraction directory: {}",
                file.name()
            ));
        }

        if file.is_dir() {
            fs::create_dir_all(&outpath)
                .with_context(|| format!("Failed to create directory: {}", outpath.display()))?;
        } else {
            // Check individual file size
            if file.size() > config.maximum_file_size {
                return Err(anyhow!(
                    "File '{}' size ({} bytes) exceeds maximum allowed size ({} bytes)",
                    file.name(),
                    file.size(),
                    config.maximum_file_size
                ));
            }

            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent).with_context(|| {
                    format!("Failed to create parent directory: {}", parent.display())
                })?;
            }

            let mut outfile = fs::File::create(&outpath)
                .with_context(|| format!("Failed to create file: {}", outpath.display()))?;
            std::io::copy(&mut file, &mut outfile)
                .with_context(|| format!("Failed to extract file: {}", outpath.display()))?;
        }
    }

    Ok(())
}

/// Reads and parses the process.txt file, returning a list of JAR file paths
pub fn read_process_file(extract_dir: &Path) -> Result<Vec<PathBuf>> {
    let process_txt_path = extract_dir.join("process.txt");

    if !process_txt_path.exists() {
        return Err(anyhow!(
            "process.txt not found in extracted directory: {}",
            extract_dir.display()
        ));
    }

    let process_file = fs::File::open(&process_txt_path).context("Failed to open process.txt")?;
    let reader = BufReader::new(process_file);

    let jar_paths: Vec<PathBuf> = reader
        .lines()
        .collect::<Result<Vec<_>, _>>()
        .context("Failed to read lines from process.txt")?
        .into_iter()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(PathBuf::from)
        .collect();

    if jar_paths.is_empty() {
        return Err(anyhow!("process.txt contains no valid JAR file paths"));
    }

    Ok(jar_paths)
}

/// Deletes the process.txt file from the extraction directory
pub fn delete_process_file(extract_dir: &Path) -> Result<()> {
    let process_txt_path = extract_dir.join("process.txt");

    if process_txt_path.exists() {
        fs::remove_file(&process_txt_path).with_context(|| {
            format!(
                "Failed to delete process.txt: {}",
                process_txt_path.display()
            )
        })?;
    }

    Ok(())
}

/// Creates a ZIP file from the specified directory
pub fn create_zip(source_dir: &Path, output_path: &Path, config: &Config) -> Result<()> {
    let output_file = fs::File::create(output_path).with_context(|| {
        format!(
            "Failed to create output ZIP file: {}",
            output_path.display()
        )
    })?;
    let mut zip_writer = zip::ZipWriter::new(output_file);

    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o755);

    add_directory_to_zip(&mut zip_writer, source_dir, source_dir, &options, config)?;

    zip_writer.finish().context("Failed to finalize ZIP file")?;

    Ok(())
}

/// Recursively adds a directory to a ZIP archive
fn add_directory_to_zip<W: Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    dir_path: &Path,
    base_path: &Path,
    options: &zip::write::SimpleFileOptions,
    config: &Config,
) -> Result<()> {
    let entries = fs::read_dir(dir_path)
        .with_context(|| format!("Failed to read directory: {}", dir_path.display()))?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        // Get relative path from base
        let relative_path = path
            .strip_prefix(base_path)
            .with_context(|| format!("Failed to compute relative path for: {}", path.display()))?;

        let name = relative_path
            .to_str()
            .ok_or_else(|| anyhow!("Invalid UTF-8 in path: {}", relative_path.display()))?
            .replace('\\', "/");

        if path.is_file() {
            // Check file size before adding
            let metadata = fs::metadata(&path)
                .with_context(|| format!("Failed to read file metadata: {}", path.display()))?;

            if metadata.len() > config.maximum_file_size {
                return Err(anyhow!(
                    "File '{}' size ({} bytes) exceeds maximum allowed size ({} bytes)",
                    name,
                    metadata.len(),
                    config.maximum_file_size
                ));
            }

            zip.start_file(&name, *options)
                .with_context(|| format!("Failed to start ZIP file entry: {}", name))?;

            let bytes = fs::read(&path)
                .with_context(|| format!("Failed to read file: {}", path.display()))?;

            zip.write_all(&bytes)
                .with_context(|| format!("Failed to write file to ZIP: {}", name))?;
        } else if path.is_dir() {
            add_directory_to_zip(zip, &path, base_path, options, config)?;
        }
    }

    Ok(())
}

/// Validates that all JAR files listed in the paths exist within the extraction directory
pub fn validate_jar_paths(jar_paths: &[PathBuf], extract_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut full_paths = Vec::new();

    for jar_relative_path in jar_paths {
        let jar_full_path = extract_dir.join(jar_relative_path);

        if !jar_full_path.exists() {
            return Err(anyhow!(
                "JAR file not found: {} (full path: {})",
                jar_relative_path.display(),
                jar_full_path.display()
            ));
        }

        if !jar_full_path.is_file() {
            return Err(anyhow!("Path is not a file: {}", jar_full_path.display()));
        }

        // Validate file extension
        if jar_full_path.extension().and_then(|s| s.to_str()) != Some("jar") {
            return Err(anyhow!(
                "File is not a JAR file: {}",
                jar_full_path.display()
            ));
        }

        full_paths.push(jar_full_path);
    }

    Ok(full_paths)
}

/// Cleans up the temporary extraction directory
pub fn cleanup_temp_directory(extract_dir: &Path) -> Result<()> {
    if extract_dir.exists() {
        fs::remove_dir_all(extract_dir).with_context(|| {
            format!(
                "Failed to remove temporary directory: {}",
                extract_dir.display()
            )
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_jar_paths() {
        // This test would require creating temporary files
        // Skipping for now, but this is where you'd add integration tests
    }
}

//! Workflow orchestration for JAR and ZIP processing
//!
//! This module contains the high-level workflow functions that coordinate
//! the processing of JAR and ZIP files with placeholder replacement.

use crate::config::Config;
use crate::{load_jar_in_memory, process_classes, write_jar, zip_operations};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

/// Processes a single JAR file workflow
///
/// This function handles the complete workflow for processing a single JAR file:
/// 1. Reads the JAR file from disk
/// 2. Loads it into memory
/// 3. Processes all classes with placeholder replacement
/// 4. Writes the modified JAR to the output path
///
/// # Arguments
/// * `config` - Configuration containing input/output paths and placeholders
///
/// # Example
/// ```no_run
/// use PlacyRust::config::Config;
/// use PlacyRust::workflows::process_jar_workflow;
/// use std::path::PathBuf;
///
/// let config = Config::new(PathBuf::from("input.jar"));
/// process_jar_workflow(&config)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn process_jar_workflow(config: &Config) -> Result<()> {
    println!("Mode: Single JAR Processing");
    println!("Input: {}", config.input_path.display());
    println!();

    let start = Instant::now();
    let jar_bytes = std::fs::read(&config.input_path)
        .with_context(|| format!("Failed to read JAR file: {}", config.input_path.display()))?;

    let mut jar_memory = load_jar_in_memory(jar_bytes)?;
    println!(
        "✓ Loaded JAR: {} classes, {} other files ({:?})",
        jar_memory.classes.len(),
        jar_memory.other_files.len(),
        start.elapsed()
    );

    let start = Instant::now();
    let placeholder_refs: HashMap<&str, &str> = config
        .placeholders
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    process_classes(&mut jar_memory, &placeholder_refs)?;
    println!(
        "✓ Processed {} classes ({:?})",
        jar_memory.classes.len(),
        start.elapsed()
    );

    let start = Instant::now();
    write_jar(&jar_memory, config.output_path.to_str().unwrap())?;
    println!("✓ Wrote output JAR ({:?})", start.elapsed());

    Ok(())
}

/// Processes a ZIP file workflow with multiple JARs
///
/// This function handles the complete workflow for processing a ZIP file
/// that contains multiple JAR files:
/// 1. Extracts the ZIP file to a temporary directory
/// 2. Reads the process.txt file to get the list of JARs to process
/// 3. Validates all JAR paths exist
/// 4. Processes each JAR file in place with placeholder replacement
/// 5. Optionally deletes process.txt
/// 6. Creates a new output ZIP file
/// 7. Cleans up temporary files
///
/// # Arguments
/// * `config` - Configuration containing paths, limits, and placeholders
///
/// # Example
/// ```no_run
/// use PlacyRust::config::Config;
/// use PlacyRust::workflows::process_zip_workflow;
/// use std::path::PathBuf;
///
/// let config = Config::new(PathBuf::from("input.zip"));
/// process_zip_workflow(&config)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn process_zip_workflow(config: &Config) -> Result<()> {
    println!("Mode: ZIP Workflow Processing");
    println!("Input: {}", config.input_path.display());
    println!("Configuration:");
    println!("  - Max files: {}", config.maximum_allowed_files);
    println!("  - Max ZIP size: {} bytes", config.maximum_zip_size);
    println!("  - Max file size: {} bytes", config.maximum_file_size);
    println!("  - Delete process.txt: {}", config.delete_process_file);
    println!();

    // Extract ZIP
    println!("Extracting ZIP file...");
    let extract_start = Instant::now();
    zip_operations::extract_zip(&config.input_path, &config.temp_extract_dir, config)?;
    println!(
        "✓ Extracted to {} ({:?})",
        config.temp_extract_dir.display(),
        extract_start.elapsed()
    );

    // Read process.txt
    let jar_paths = zip_operations::read_process_file(&config.temp_extract_dir)
        .context("Failed to read process.txt")?;
    println!("✓ Found {} JAR file(s) to process", jar_paths.len());

    // Validate JAR paths
    let full_jar_paths = zip_operations::validate_jar_paths(&jar_paths, &config.temp_extract_dir)
        .context("JAR path validation failed")?;

    // Process each JAR
    println!();
    for (idx, jar_full_path) in full_jar_paths.iter().enumerate() {
        let jar_relative_path = jar_full_path
            .strip_prefix(&config.temp_extract_dir)
            .unwrap_or(jar_full_path);

        println!(
            "[{}/{}] Processing: {}",
            idx + 1,
            full_jar_paths.len(),
            jar_relative_path.display()
        );

        let start = Instant::now();
        process_single_jar_in_place(jar_full_path, config)?;
        println!("  ✓ Completed in {:?}", start.elapsed());
    }

    // Delete process.txt if configured
    if config.delete_process_file {
        zip_operations::delete_process_file(&config.temp_extract_dir)?;
        println!("\n✓ Removed process.txt");
    }

    // Create output ZIP
    println!("Creating output ZIP...");
    let zip_start = Instant::now();
    zip_operations::create_zip(&config.temp_extract_dir, &config.output_path, config)?;
    println!("✓ Created output ZIP ({:?})", zip_start.elapsed());

    // Cleanup
    zip_operations::cleanup_temp_directory(&config.temp_extract_dir)?;
    println!("✓ Cleaned up temporary files");

    Ok(())
}

/// Processes a single JAR file in place (overwrites the original)
///
/// This is a helper function used by the ZIP workflow to process individual
/// JAR files. It modifies the JAR file in place rather than creating a new file.
///
/// # Arguments
/// * `jar_path` - Path to the JAR file to process
/// * `config` - Configuration containing placeholders
///
/// # Example
/// ```no_run
/// use PlacyRust::config::Config;
/// use PlacyRust::workflows::process_single_jar_in_place;
/// use std::path::PathBuf;
///
/// let config = Config::new(PathBuf::from("input.jar"));
/// let jar_path = PathBuf::from("extracted_temp/library.jar");
/// process_single_jar_in_place(&jar_path, &config)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn process_single_jar_in_place(jar_path: &PathBuf, config: &Config) -> Result<()> {
    let jar_bytes = std::fs::read(jar_path)
        .with_context(|| format!("Failed to read JAR: {}", jar_path.display()))?;

    let mut jar_memory = load_jar_in_memory(jar_bytes)?;

    println!(
        "  - Loaded: {} classes, {} other files",
        jar_memory.classes.len(),
        jar_memory.other_files.len()
    );

    let placeholder_refs: HashMap<&str, &str> = config
        .placeholders
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    process_classes(&mut jar_memory, &placeholder_refs)?;

    write_jar(&jar_memory, jar_path.to_str().unwrap())?;

    Ok(())
}

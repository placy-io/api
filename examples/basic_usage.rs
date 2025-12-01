//! Basic usage example for PlacyRust
//!
//! This example demonstrates how to:
//! 1. Load a JAR file into memory
//! 2. Define placeholder replacements
//! 3. Process all class files
//! 4. Write the modified JAR to disk

use PlacyRust::{load_jar_in_memory, process_classes, write_jar};
use std::collections::HashMap;
use std::fs;
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    println!("PlacyRust Basic Usage Example");
    println!("==============================\n");

    // Step 1: Load JAR file
    let input_path = "input.jar";
    println!("Loading JAR from: {}", input_path);

    let start = Instant::now();
    let jar_bytes = fs::read(input_path)?;
    let mut jar_memory = load_jar_in_memory(jar_bytes)?;

    println!(
        "✓ Loaded JAR with {} classes and {} other files in {:?}\n",
        jar_memory.classes.len(),
        jar_memory.other_files.len(),
        start.elapsed()
    );

    // Step 2: Define placeholders
    println!("Defining placeholder replacements:");
    let mut placeholders = HashMap::new();
    placeholders.insert("%%__USERNAME__%%", "alice");
    placeholders.insert("%%__TIMESTAMP__%%", "1704067200");
    placeholders.insert("%%__VERSION__%%", "1.0.0");
    placeholders.insert("%%__BUILD_ID__%%", "abc123def456");

    for (placeholder, replacement) in &placeholders {
        println!("  {} -> {}", placeholder, replacement);
    }
    println!();

    // Step 3: Process classes
    println!("Processing class files...");
    let start = Instant::now();
    process_classes(&mut jar_memory, &placeholders)?;
    println!(
        "✓ Processed {} classes in {:?}\n",
        jar_memory.classes.len(),
        start.elapsed()
    );

    // Step 4: Write output
    let output_path = "output.jar";
    println!("Writing modified JAR to: {}", output_path);
    let start = Instant::now();
    write_jar(&jar_memory, output_path)?;
    println!("✓ Wrote JAR file in {:?}\n", start.elapsed());

    println!("Done! Modified JAR saved to {}", output_path);

    Ok(())
}

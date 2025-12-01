//! JAR file operations and in-memory representation

use anyhow::Result;
use std::io::{Cursor, Read, Write};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::FileOptions};

/// In-memory representation of a JAR file
///
/// Separates class files from other resources for efficient processing
pub struct JarMemory {
    /// List of class files with their paths and bytecode
    pub classes: Vec<(String, Vec<u8>)>,
    /// List of non-class files (resources, manifests, etc.)
    pub other_files: Vec<(String, Vec<u8>)>,
}

/// Loads a JAR file from bytes into memory
///
/// # Arguments
/// * `bytes` - The raw bytes of the JAR file
///
/// # Returns
/// A `JarMemory` instance containing all files from the JAR, separated into classes and other files
///
/// # Example
/// ```no_run
/// use std::fs;
/// use placy_rust::load_jar_in_memory;
///
/// let jar_bytes = fs::read("input.jar")?;
/// let jar_memory = load_jar_in_memory(jar_bytes)?;
/// println!("Loaded {} classes", jar_memory.classes.len());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn load_jar_in_memory(bytes: Vec<u8>) -> Result<JarMemory> {
    let mut cursor = Cursor::new(&bytes);
    let mut archive = ZipArchive::new(&mut cursor)?;

    let mut classes = Vec::new();
    let mut other_files = Vec::new();

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();
        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut buf)?;

        if name.ends_with(".class") {
            classes.push((name, buf));
        } else {
            other_files.push((name, buf));
        }
    }

    Ok(JarMemory {
        classes,
        other_files,
    })
}

/// Writes a `JarMemory` instance to a JAR file
///
/// # Arguments
/// * `jar_memory` - The in-memory JAR representation to write
/// * `output_path` - Path where the JAR file should be written
///
/// # Example
/// ```no_run
/// use placy_rust::{load_jar_in_memory, write_jar};
/// use std::fs;
///
/// let jar_bytes = fs::read("input.jar")?;
/// let jar_memory = load_jar_in_memory(jar_bytes)?;
/// write_jar(&jar_memory, "output.jar")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn write_jar(jar_memory: &JarMemory, output_path: &str) -> Result<()> {
    let file = std::fs::File::create(output_path)?;
    let mut zip = ZipWriter::new(file);

    let options: FileOptions<'_, ()> = FileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o644);

    for (name, bytes) in jar_memory
        .classes
        .iter()
        .chain(jar_memory.other_files.iter())
    {
        zip.start_file(name, options)?;
        zip.write_all(bytes)?;
    }

    zip.finish()?;

    Ok(())
}

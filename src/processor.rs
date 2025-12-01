//! Class file processing and placeholder replacement

use anyhow::Result;
use rayon::prelude::*;
use ristretto_classfile::{ClassFile, Constant, ConstantPool};
use std::io::Cursor;

use crate::jar::JarMemory;

/// Processes all class files in a JAR, replacing placeholders in the constant pool
///
/// This function uses parallel processing to handle multiple class files simultaneously
/// for improved performance on large JARs.
///
/// # Arguments
/// * `jar_memory` - Mutable reference to the JAR in memory
/// * `placeholders` - HashMap of placeholder strings to their replacements
///
/// # Example
/// ```no_run
/// use placy_rust::{load_jar_in_memory, process_classes};
/// use std::collections::HashMap;
/// use std::fs;
///
/// let jar_bytes = fs::read("input.jar")?;
/// let mut jar_memory = load_jar_in_memory(jar_bytes)?;
///
/// let mut placeholders = HashMap::new();
/// placeholders.insert("%%__USERNAME__%%", "john_doe");
/// placeholders.insert("%%__TIMESTAMP__%%", "1234567890");
///
/// process_classes(&mut jar_memory, &placeholders)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn process_classes(
    jar_memory: &mut JarMemory,
    placeholders: &std::collections::HashMap<&str, &str>,
) -> Result<()> {
    jar_memory
        .classes
        .par_iter_mut()
        .try_for_each(|(_, class_bytes)| -> Result<()> {
            let mut parsed_class =
                ClassFile::from_bytes(&mut Cursor::new(std::mem::take(class_bytes)))?;
            let mut new_pool = ConstantPool::new();

            for entry in parsed_class.constant_pool.into_iter() {
                let mut entry = entry.clone();
                if let Constant::Utf8(ref mut btf8) = entry {
                    if let Some(replacement) = placeholders.get(btf8.as_str()) {
                        *btf8 = (*replacement).to_string();
                    }
                }
                new_pool.add(entry)?;
            }

            parsed_class.constant_pool = new_pool;
            parsed_class.verify()?;

            class_bytes.clear();
            parsed_class.to_bytes(class_bytes)?;
            Ok(())
        })?;

    Ok(())
}

/// Processes UTF-8 and UTF-16 text files in a JAR, replacing placeholders
///
/// This function uses parallel processing to handle multiple text files simultaneously.
/// It supports both UTF-8 and UTF-16 (LE/BE) encoded files, auto-detecting the encoding.
///
/// # Arguments
/// * `jar_memory` - Mutable reference to the JAR in memory
/// * `placeholders` - HashMap of placeholder strings to their replacements
/// * `file_extensions` - Optional slice of file extensions to process (e.g., &["txt", "xml", "properties"])
///                       If None, processes common text file extensions
///
/// # Example
/// ```no_run
/// use placy_rust::{load_jar_in_memory, processor::process_utf_files};
/// use std::collections::HashMap;
/// use std::fs;
///
/// let jar_bytes = fs::read("input.jar")?;
/// let mut jar_memory = load_jar_in_memory(jar_bytes)?;
///
/// let mut placeholders = HashMap::new();
/// placeholders.insert("%%__USERNAME__%%", "john_doe");
/// placeholders.insert("%%__TIMESTAMP__%%", "1234567890");
///
/// // Process default text file types
/// process_utf_files(&mut jar_memory, &placeholders, None)?;
///
/// // Or specify custom extensions
/// process_utf_files(&mut jar_memory, &placeholders, Some(&["txt", "xml", "json"]))?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn process_utf_files(
    jar_memory: &mut JarMemory,
    placeholders: &std::collections::HashMap<&str, &str>,
    file_extensions: Option<&[&str]>,
) -> Result<()> {
    let default_extensions = &[
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
    let extensions = file_extensions.unwrap_or(default_extensions);

    jar_memory
        .other_files
        .par_iter_mut()
        .try_for_each(|(path, file_bytes)| -> Result<()> {
            // Check if file has a matching extension
            let should_process = path
                .rfind('.')
                .map(|dot_idx| {
                    let ext = &path[dot_idx + 1..];
                    extensions
                        .iter()
                        .any(|&allowed_ext| ext.eq_ignore_ascii_case(allowed_ext))
                })
                .unwrap_or(false);

            if !should_process {
                return Ok(());
            }

            // Try to process the file
            if let Some(processed) = process_text_file(file_bytes, placeholders)? {
                *file_bytes = processed;
            }

            Ok(())
        })?;

    Ok(())
}

/// Processes a single text file, detecting encoding and replacing placeholders
fn process_text_file(
    file_bytes: &[u8],
    placeholders: &std::collections::HashMap<&str, &str>,
) -> Result<Option<Vec<u8>>> {
    // Try UTF-8 first (most common)
    if let Ok(text) = std::str::from_utf8(file_bytes) {
        let processed = replace_placeholders(text, placeholders);
        if processed != text {
            return Ok(Some(processed.into_bytes()));
        }
        return Ok(None);
    }

    // Try UTF-16 LE
    if file_bytes.len() >= 2 && file_bytes.len() % 2 == 0 {
        if let Some(processed) = try_utf16_le(file_bytes, placeholders)? {
            return Ok(Some(processed));
        }
    }

    // Try UTF-16 BE
    if file_bytes.len() >= 2 && file_bytes.len() % 2 == 0 {
        if let Some(processed) = try_utf16_be(file_bytes, placeholders)? {
            return Ok(Some(processed));
        }
    }

    // If we can't decode it, leave it unchanged
    Ok(None)
}

/// Replaces all placeholders in the given text
fn replace_placeholders(
    text: &str,
    placeholders: &std::collections::HashMap<&str, &str>,
) -> String {
    let mut result = text.to_string();
    for (placeholder, replacement) in placeholders {
        if result.contains(placeholder) {
            result = result.replace(placeholder, replacement);
        }
    }
    result
}

/// Tries to decode as UTF-16 LE and process placeholders
fn try_utf16_le(
    bytes: &[u8],
    placeholders: &std::collections::HashMap<&str, &str>,
) -> Result<Option<Vec<u8>>> {
    let u16_vec: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();

    if let Ok(text) = String::from_utf16(&u16_vec) {
        let processed = replace_placeholders(&text, placeholders);
        if processed != text {
            let encoded: Vec<u16> = processed.encode_utf16().collect();
            let bytes: Vec<u8> = encoded.iter().flat_map(|&u| u.to_le_bytes()).collect();
            return Ok(Some(bytes));
        }
    }

    Ok(None)
}

/// Tries to decode as UTF-16 BE and process placeholders
fn try_utf16_be(
    bytes: &[u8],
    placeholders: &std::collections::HashMap<&str, &str>,
) -> Result<Option<Vec<u8>>> {
    let u16_vec: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
        .collect();

    if let Ok(text) = String::from_utf16(&u16_vec) {
        let processed = replace_placeholders(&text, placeholders);
        if processed != text {
            let encoded: Vec<u16> = processed.encode_utf16().collect();
            let bytes: Vec<u8> = encoded.iter().flat_map(|&u| u.to_be_bytes()).collect();
            return Ok(Some(bytes));
        }
    }

    Ok(None)
}

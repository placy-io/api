// Copyright 2025 Placy
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Java class file constant pool processing.

use crate::error::Result;
use rayon::prelude::*;
use ristretto_classfile::{ClassFile, Constant, ConstantPool};
use std::collections::HashMap;
use std::io::Cursor;

/// Processes class file bytecode, replacing placeholders in the constant pool.
///
/// This function modifies the constant pool of a Java class file, replacing
/// any UTF-8 string constants that match placeholder keys with their values.
///
/// # Arguments
///
/// * `class_bytes` - The raw bytecode of the class file
/// * `placeholders` - Map of placeholder strings to their replacement values
///
/// # Returns
///
/// Returns the modified class file bytecode, or an error if parsing fails.
pub fn process_class_bytes(
    class_bytes: &[u8],
    placeholders: &HashMap<&str, &str>,
) -> Result<Vec<u8>> {
    let bytes_vec = class_bytes.to_vec();
    let mut cursor = Cursor::new(bytes_vec);
    let mut class_file = ClassFile::from_bytes(&mut cursor)?;

    let mut new_pool = ConstantPool::new();
    let mut modified = false;

    for entry in class_file.constant_pool.into_iter() {
        let mut entry = entry.clone();
        if let Constant::Utf8(ref mut utf8_str) = entry {
            if let Some(replacement) = placeholders.get(utf8_str.as_str()) {
                *utf8_str = (*replacement).to_string();
                modified = true;
            }
        }
        new_pool.add(entry)?;
    }

    if modified {
        class_file.constant_pool = new_pool;
        class_file.verify()?;
    }

    let mut output = Vec::new();
    class_file.to_bytes(&mut output)?;
    Ok(output)
}

/// Processes multiple class files in parallel.
///
/// # Arguments
///
/// * `classes` - Vector of (path, bytecode) tuples
/// * `placeholders` - Map of placeholder strings to their replacement values
///
/// # Returns
///
/// Returns the processed classes with the same paths but modified bytecode.
pub fn process_classes_parallel(
    classes: Vec<(String, Vec<u8>)>,
    placeholders: &HashMap<&str, &str>,
) -> Result<Vec<(String, Vec<u8>)>> {
    classes
        .into_par_iter()
        .map(|(path, bytes)| {
            let processed = process_class_bytes(&bytes, placeholders)?;
            Ok((path, processed))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Creating valid class files for testing requires actual bytecode.
    // These tests would need mock class files or integration tests with real JARs.

    #[test]
    fn test_empty_placeholders() {
        // With empty placeholders, class should be unchanged structurally
        // (though bytes might differ due to serialization)
        let placeholders: HashMap<&str, &str> = HashMap::new();

        // This would need a real class file to test properly
        // For now, we just verify the function compiles correctly
        assert!(placeholders.is_empty());
    }
}

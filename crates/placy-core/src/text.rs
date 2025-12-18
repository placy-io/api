// Copyright 2025 Placy
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Text file processing with UTF-8 and UTF-16 support.

use crate::error::Result;
use std::collections::HashMap;

/// Processes a text file, replacing placeholders.
///
/// Supports UTF-8 and UTF-16 (LE/BE) encodings with auto-detection.
///
/// # Returns
///
/// Returns `Some(bytes)` with the modified content if changes were made,
/// or `None` if no placeholders were found.
pub fn process_text_file(
    file_bytes: &[u8],
    placeholders: &HashMap<&str, &str>,
) -> Result<Option<Vec<u8>>> {
    // Try UTF-8 first (most common)
    if let Ok(text) = std::str::from_utf8(file_bytes) {
        let processed = replace_placeholders(text, placeholders);
        if processed != text {
            return Ok(Some(processed.into_bytes()));
        }
        return Ok(None);
    }

    // Try UTF-16 LE (with BOM check)
    if file_bytes.len() >= 2 && file_bytes.len() % 2 == 0 {
        // Check for UTF-16 LE BOM
        if file_bytes.starts_with(&[0xFF, 0xFE]) {
            if let Some(processed) = try_utf16_le(&file_bytes[2..], placeholders)? {
                let mut result = vec![0xFF, 0xFE];
                result.extend(processed);
                return Ok(Some(result));
            }
        }

        // Check for UTF-16 BE BOM
        if file_bytes.starts_with(&[0xFE, 0xFF]) {
            if let Some(processed) = try_utf16_be(&file_bytes[2..], placeholders)? {
                let mut result = vec![0xFE, 0xFF];
                result.extend(processed);
                return Ok(Some(result));
            }
        }

        // Try without BOM
        if let Some(processed) = try_utf16_le(file_bytes, placeholders)? {
            return Ok(Some(processed));
        }
        if let Some(processed) = try_utf16_be(file_bytes, placeholders)? {
            return Ok(Some(processed));
        }
    }

    // If we can't decode it, leave it unchanged
    Ok(None)
}

/// Replaces all placeholders in the given text.
fn replace_placeholders(text: &str, placeholders: &HashMap<&str, &str>) -> String {
    let mut result = text.to_string();
    for (placeholder, replacement) in placeholders {
        if result.contains(*placeholder) {
            result = result.replace(*placeholder, replacement);
        }
    }
    result
}

/// Tries to decode as UTF-16 LE and process placeholders.
fn try_utf16_le(bytes: &[u8], placeholders: &HashMap<&str, &str>) -> Result<Option<Vec<u8>>> {
    if bytes.len() % 2 != 0 {
        return Ok(None);
    }

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

/// Tries to decode as UTF-16 BE and process placeholders.
fn try_utf16_be(bytes: &[u8], placeholders: &HashMap<&str, &str>) -> Result<Option<Vec<u8>>> {
    if bytes.len() % 2 != 0 {
        return Ok(None);
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utf8_replacement() {
        let content = b"Hello %%__USER__%%, your id is %%__ID__%%";
        let mut placeholders = HashMap::new();
        placeholders.insert("%%__USER__%%", "Alice");
        placeholders.insert("%%__ID__%%", "12345");

        let result = process_text_file(content, &placeholders).unwrap();
        assert!(result.is_some());
        assert_eq!(
            String::from_utf8(result.unwrap()).unwrap(),
            "Hello Alice, your id is 12345"
        );
    }

    #[test]
    fn test_no_replacement_needed() {
        let content = b"Hello World";
        let mut placeholders = HashMap::new();
        placeholders.insert("%%__USER__%%", "Alice");

        let result = process_text_file(content, &placeholders).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_utf16_le_with_bom() {
        // "Hello %%__USER__%%"" in UTF-16 LE with BOM
        let text = "Hello %%__USER__%%";
        let mut content = vec![0xFF, 0xFE]; // BOM
        for u in text.encode_utf16() {
            content.extend_from_slice(&u.to_le_bytes());
        }

        let mut placeholders = HashMap::new();
        placeholders.insert("%%__USER__%%", "Bob");

        let result = process_text_file(&content, &placeholders).unwrap();
        assert!(result.is_some());

        let result_bytes = result.unwrap();
        assert_eq!(&result_bytes[0..2], &[0xFF, 0xFE]); // BOM preserved
    }

    #[test]
    fn test_binary_file_unchanged() {
        // Random binary data that's not valid UTF-8 or UTF-16
        let content = vec![0x80, 0x81, 0x82, 0x83, 0x84];
        let mut placeholders = HashMap::new();
        placeholders.insert("%%__USER__%%", "Alice");

        let result = process_text_file(&content, &placeholders).unwrap();
        assert!(result.is_none());
    }
}

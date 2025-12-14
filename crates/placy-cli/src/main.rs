//! Placy CLI - JAR/ZIP Placeholder Replacement Tool
//!
//! A command-line tool for replacing placeholders in JAR and ZIP archives.

use anyhow::{bail, Context, Result};
use placy_core::{process_archive, process_jar, Config};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn main() -> Result<()> {
    let args = parse_arguments()?;

    println!("Placy - JAR/ZIP Placeholder Replacement Tool");
    println!("=============================================\n");

    let total_start = Instant::now();

    // Read input file
    let input_bytes = std::fs::read(&args.input_path)
        .with_context(|| format!("Failed to read input file: {}", args.input_path.display()))?;

    println!(
        "Input: {} ({} bytes)",
        args.input_path.display(),
        input_bytes.len()
    );
    println!("Placeholders: {}", args.config.placeholders().len());
    println!();

    // Process based on file type
    let output_bytes = if is_jar_file(&args.input_path) {
        println!("Mode: Single JAR Processing");
        let start = Instant::now();
        let result =
            process_jar(&input_bytes, &args.config).context("Failed to process JAR file")?;
        println!("  Processed in {:?}", start.elapsed());
        result
    } else if is_zip_file(&args.input_path) {
        println!("Mode: ZIP Archive Processing");
        let start = Instant::now();
        let result =
            process_archive(&input_bytes, &args.config).context("Failed to process ZIP archive")?;
        println!("  Processed in {:?}", start.elapsed());
        result
    } else {
        bail!(
            "Unsupported file type. Expected .jar or .zip, got: {}",
            args.input_path.display()
        );
    };

    // Write output
    std::fs::write(&args.output_path, &output_bytes).with_context(|| {
        format!(
            "Failed to write output file: {}",
            args.output_path.display()
        )
    })?;

    println!();
    println!("Total time: {:?}", total_start.elapsed());
    println!(
        "Output: {} ({} bytes)",
        args.output_path.display(),
        output_bytes.len()
    );

    Ok(())
}

/// Parsed command-line arguments.
struct Args {
    input_path: PathBuf,
    output_path: PathBuf,
    config: Config,
}

/// Parses command-line arguments.
fn parse_arguments() -> Result<Args> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 || args[1] == "-h" || args[1] == "--help" {
        print_usage(&args[0]);
        std::process::exit(if args.len() < 2 { 1 } else { 0 });
    }

    let input_path = PathBuf::from(&args[1]);

    // Validate input exists
    if !input_path.exists() {
        bail!("Input file does not exist: {}", input_path.display());
    }

    // Parse remaining arguments
    let mut output_path: Option<PathBuf> = None;
    let mut placeholders: HashMap<String, String> = HashMap::new();
    let mut max_files: Option<usize> = None;
    let mut max_zip_size: Option<u64> = None;
    let mut max_file_size: Option<u64> = None;
    let mut delete_process_file = true;
    let mut delete_ignore_file = true;

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                if i + 1 >= args.len() {
                    bail!("Missing value for --output");
                }
                output_path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            },
            "-p" | "--placeholder" => {
                if i + 2 >= args.len() {
                    bail!("--placeholder requires two arguments: key and value");
                }
                placeholders.insert(args[i + 1].clone(), args[i + 2].clone());
                i += 3;
            },
            "--max-files" => {
                if i + 1 >= args.len() {
                    bail!("Missing value for --max-files");
                }
                max_files = Some(args[i + 1].parse().context("Invalid --max-files value")?);
                i += 2;
            },
            "--max-zip-size" => {
                if i + 1 >= args.len() {
                    bail!("Missing value for --max-zip-size");
                }
                max_zip_size = Some(
                    placy_core::config::parse_size(&args[i + 1])
                        .map_err(|e| anyhow::anyhow!("{e}"))?,
                );
                i += 2;
            },
            "--max-file-size" => {
                if i + 1 >= args.len() {
                    bail!("Missing value for --max-file-size");
                }
                max_file_size = Some(
                    placy_core::config::parse_size(&args[i + 1])
                        .map_err(|e| anyhow::anyhow!("{e}"))?,
                );
                i += 2;
            },
            "--keep-process-file" => {
                delete_process_file = false;
                i += 1;
            },
            "--keep-ignore-file" => {
                delete_ignore_file = false;
                i += 1;
            },
            arg => {
                bail!("Unknown argument: {arg}");
            },
        }
    }

    // Default output path based on input
    let output_path = output_path.unwrap_or_else(|| {
        if is_zip_file(&input_path) {
            PathBuf::from("output.zip")
        } else {
            PathBuf::from("output.jar")
        }
    });

    // Build config
    let mut builder = Config::builder()
        .with_placeholders(placeholders)
        .with_delete_process_file(delete_process_file)
        .with_delete_ignore_file(delete_ignore_file);

    if let Some(max) = max_files {
        builder = builder.with_max_file_count(max);
    }
    if let Some(max) = max_zip_size {
        builder = builder.with_max_zip_size(max);
    }
    if let Some(max) = max_file_size {
        builder = builder.with_max_file_size(max);
    }

    // Add default placeholders if none provided
    let mut config = builder.build();
    if config.placeholders().is_empty() {
        config = Config::builder()
            .add_placeholder("%%__USERNAME__%%", "default_user")
            .add_placeholder("%%__TIMESTAMP__%%", chrono_timestamp())
            .add_placeholder("%%__NONCE__%%", generate_nonce())
            .with_delete_process_file(delete_process_file)
            .with_delete_ignore_file(delete_ignore_file)
            .build();
    }

    Ok(Args {
        input_path,
        output_path,
        config,
    })
}

/// Checks if a path is a JAR file.
fn is_jar_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("jar"))
        .unwrap_or(false)
}

/// Checks if a path is a ZIP file.
fn is_zip_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("zip"))
        .unwrap_or(false)
}

/// Generates a simple timestamp.
fn chrono_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

/// Generates a random nonce.
fn generate_nonce() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{seed:032x}")
}

/// Prints usage information.
fn print_usage(program: &str) {
    eprintln!("Usage: {program} <input.jar|input.zip> [OPTIONS]");
    eprintln!();
    eprintln!("Arguments:");
    eprintln!("  <input>                       Input JAR or ZIP file to process");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  -o, --output <path>           Output file path");
    eprintln!("  -p, --placeholder <key> <val> Add a placeholder replacement");
    eprintln!("  --max-files <n>               Maximum files allowed (default: 1000)");
    eprintln!("  --max-zip-size <size>         Maximum ZIP size (default: 100MB)");
    eprintln!("  --max-file-size <size>        Maximum individual file size (default: 10MB)");
    eprintln!("  --keep-process-file           Don't remove process.txt from output");
    eprintln!("  --keep-ignore-file            Don't remove ignore.txt from output");
    eprintln!("  -h, --help                    Show this help message");
    eprintln!();
    eprintln!("Size format: <number>B|KB|MB|GB (e.g., 100MB, 50KB, 1GB)");
    eprintln!();
    eprintln!("Archive Control Files:");
    eprintln!("  process.txt   Regex patterns for JAR files to process (one per line)");
    eprintln!("  ignore.txt    Regex patterns for files to exclude (one per line)");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  {program} input.jar -o output.jar -p '%%__USER__%%' 'alice'");
    eprintln!("  {program} bundle.zip -p '%%__NONCE__%%' 'abc123'");
}

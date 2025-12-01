use PlacyRust::{config::Config, process_jar_workflow, process_zip_workflow};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

fn main() -> Result<()> {
    let config = parse_arguments()?;
    config.validate()?;

    println!("PlacyRust - JAR/ZIP Placeholder Replacement Tool");
    println!("================================================\n");

    let total_start = Instant::now();

    if config.is_zip_workflow() {
        process_zip_workflow(&config)?;
    } else {
        process_jar_workflow(&config)?;
    }

    println!("\n✓ Total processing time: {:?}", total_start.elapsed());
    println!("✓ Output written to: {}", config.output_path.display());

    Ok(())
}

/// Parses command-line arguments and creates a Config
fn parse_arguments() -> Result<Config> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage(&args[0]);
        std::process::exit(1);
    }

    let input_path = PathBuf::from(&args[1]);

    // Create config with default placeholders
    let placeholders = create_default_placeholders();

    let mut config = Config::new(input_path).with_placeholders(placeholders);

    // Parse optional arguments
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                if i + 1 >= args.len() {
                    anyhow::bail!("Missing value for --output flag");
                }
                config = config.with_output_path(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--max-files" => {
                if i + 1 >= args.len() {
                    anyhow::bail!("Missing value for --max-files flag");
                }
                let max_files: usize = args[i + 1]
                    .parse()
                    .context("Invalid value for --max-files")?;
                config = config.with_maximum_allowed_files(max_files);
                i += 2;
            }
            "--max-zip-size" => {
                if i + 1 >= args.len() {
                    anyhow::bail!("Missing value for --max-zip-size flag");
                }
                let max_size =
                    PlacyRust::config::parse_size(&args[i + 1]).context("Invalid max-zip-size")?;
                config = config.with_maximum_zip_size(max_size);
                i += 2;
            }
            "--max-file-size" => {
                if i + 1 >= args.len() {
                    anyhow::bail!("Missing value for --max-file-size flag");
                }
                let max_size =
                    PlacyRust::config::parse_size(&args[i + 1]).context("Invalid max-file-size")?;
                config = config.with_maximum_file_size(max_size);
                i += 2;
            }
            "--keep-process-file" => {
                config = config.with_delete_process_file(false);
                i += 1;
            }
            "-p" | "--placeholder" => {
                if i + 2 >= args.len() {
                    anyhow::bail!("--placeholder requires two arguments: key and value");
                }
                config = config.add_placeholder(args[i + 1].clone(), args[i + 2].clone());
                i += 3;
            }
            _ => {
                anyhow::bail!("Unknown argument: {}", args[i]);
            }
        }
    }

    Ok(config)
}

/// Creates default placeholder values
fn create_default_placeholders() -> HashMap<String, String> {
    let mut placeholders = HashMap::new();
    placeholders.insert("%%__USERNAME__%%".to_string(), "harfull".to_string());
    placeholders.insert("%%__TIMESTAMP__%%".to_string(), "1696118400".to_string());
    placeholders.insert(
        "%%__NONCE__%%".to_string(),
        "a94f3bc19d7e4fa0b2c4e8fd91a7c3de".to_string(),
    );
    placeholders
}

/// Prints usage information
fn print_usage(program_name: &str) {
    eprintln!("Usage: {} <input.jar|input.zip> [OPTIONS]", program_name);
    eprintln!();
    eprintln!("Arguments:");
    eprintln!("  <input>              Input JAR or ZIP file to process");
    eprintln!();
    eprintln!("Options:");
    eprintln!(
        "  -o, --output <path>           Output file path (default: output.jar or output.zip)"
    );
    eprintln!("  -p, --placeholder <key> <val> Add a placeholder replacement");
    eprintln!("  --max-files <n>               Maximum files in ZIP (default: 20)");
    eprintln!("  --max-zip-size <size>         Maximum ZIP size (default: 100MB)");
    eprintln!("  --max-file-size <size>        Maximum file size (default: 10MB)");
    eprintln!("  --keep-process-file           Don't delete process.txt after processing");
    eprintln!();
    eprintln!("Size format: <number>KB|MB|GB (e.g., 100MB, 50MB, 1GB)");
    eprintln!();
    eprintln!("Workflows:");
    eprintln!("  JAR: Processes a single JAR file and outputs result");
    eprintln!("  ZIP: Extracts ZIP, processes JARs listed in process.txt, creates output ZIP");
}

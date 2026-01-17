//! Tylax CLI - High-performance bidirectional LaTeX ↔ Typst converter

#[cfg(feature = "cli")]
use clap::{Parser, Subcommand, ValueEnum};
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use tylax::{
    convert_auto, convert_auto_document, detect_format,
    diagnostics::{check_latex, format_diagnostics},
    latex_document_to_typst, latex_to_typst, latex_to_typst_with_diagnostics,
    tikz::{convert_cetz_to_tikz, convert_tikz_to_cetz, is_cetz_code},
    typst_document_to_latex, typst_to_latex, typst_to_latex_with_diagnostics, CliDiagnostic,
    T2LOptions,
};

#[cfg(feature = "cli")]
#[derive(Parser)]
#[command(name = "t2l")]
#[command(author = "SciPenAI")]
#[command(version)]
#[command(about = "Tylax - High-performance bidirectional LaTeX ↔ Typst converter", long_about = None)]
struct Cli {
    /// Subcommand to run
    #[command(subcommand)]
    command: Option<Commands>,

    /// Input file path (reads from stdin if not provided)
    input_file: Option<String>,

    /// Output file path (writes to stdout if not provided)
    #[arg(short, long)]
    output: Option<String>,

    /// Conversion direction
    #[arg(short, long, value_enum, default_value_t = Direction::Auto)]
    direction: Direction,

    /// Full document mode (convert entire document, not just math)
    #[arg(short = 'f', long)]
    full_document: bool,

    /// Pretty print the output
    #[arg(short, long)]
    pretty: bool,

    /// Detect and print the input format without converting
    #[arg(long)]
    detect: bool,

    /// Check mode - analyze LaTeX for potential issues without converting
    #[arg(long)]
    check: bool,

    /// Use colored output (for check mode)
    #[arg(long, default_value_t = true)]
    color: bool,

    /// Disable MiniEval preprocessing for Typst scripting features (loops, functions)
    /// By default, MiniEval is enabled for T2L conversions to expand #let, #for, #if, etc.
    #[arg(long)]
    no_eval: bool,

    /// Strict mode: exit with error if any conversion warnings occur
    #[arg(long)]
    strict: bool,

    /// Quiet mode: suppress warning output to stderr
    #[arg(short, long)]
    quiet: bool,

    /// Embed warnings as comments in the output file
    #[arg(long)]
    embed_warnings: bool,
}

#[cfg(feature = "cli")]
#[derive(Subcommand)]
enum Commands {
    /// Check LaTeX for potential conversion issues
    Check {
        /// Input file to check
        input: Option<String>,

        /// Disable colored output
        #[arg(long)]
        no_color: bool,
    },

    /// Convert a file (default action)
    Convert {
        /// Input file path
        input: Option<String>,

        /// Output file path
        #[arg(short, long)]
        output: Option<String>,

        /// Conversion direction
        #[arg(short, long, value_enum, default_value_t = Direction::Auto)]
        direction: Direction,

        /// Full document mode
        #[arg(short = 'f', long)]
        full_document: bool,
    },

    /// Convert TikZ to CeTZ or vice versa
    Tikz {
        /// Input file containing TikZ or CeTZ code
        input: Option<String>,

        /// Output file path
        #[arg(short, long)]
        output: Option<String>,

        /// Direction (auto-detected by default)
        #[arg(short, long, value_enum, default_value_t = TikzDirection::Auto)]
        direction: TikzDirection,
    },

    /// Batch convert multiple files
    Batch {
        /// Input directory or glob pattern
        input: String,

        /// Output directory
        #[arg(short, long)]
        output_dir: String,

        /// Conversion direction
        #[arg(short, long, value_enum, default_value_t = Direction::L2t)]
        direction: Direction,

        /// Full document mode
        #[arg(short = 'f', long)]
        full_document: bool,

        /// File extension for output files
        #[arg(short, long)]
        extension: Option<String>,
    },

    /// Show version and feature info
    Info,
}

#[cfg(feature = "cli")]
#[derive(Clone, ValueEnum)]
enum TikzDirection {
    /// Auto-detect based on content
    Auto,
    /// TikZ to CeTZ
    TikzToCetz,
    /// CeTZ to TikZ
    CetzToTikz,
}

#[cfg(feature = "cli")]
#[derive(Clone, ValueEnum)]
enum Direction {
    /// Auto-detect based on file extension or content
    Auto,
    /// LaTeX to Typst
    L2t,
    /// Typst to LaTeX
    T2l,
}

#[cfg(feature = "cli")]
fn main() -> io::Result<()> {
    let cli = Cli::parse();

    // Handle subcommands first
    if let Some(cmd) = cli.command {
        return handle_subcommand(cmd);
    }

    // Read input
    let (input, filename) = match cli.input_file {
        Some(ref path) => (fs::read_to_string(path)?, Some(path.clone())),
        None => {
            let mut buffer = String::new();
            io::stdin().read_to_string(&mut buffer)?;
            (buffer, None)
        }
    };

    // If detect mode, just print format and exit
    if cli.detect {
        let format = detect_format(&input);
        println!("{}", format);
        return Ok(());
    }

    // If check mode, analyze and report issues
    if cli.check {
        let result = check_latex(&input);
        let output = format_diagnostics(&result, cli.color);
        println!("{}", output);

        // Exit with error code if there are errors
        if result.has_errors() {
            std::process::exit(1);
        }
        return Ok(());
    }

    // Determine direction
    let direction = match cli.direction {
        Direction::Auto => {
            if let Some(ref name) = filename {
                if name.ends_with(".typ") {
                    Direction::T2l
                } else if name.ends_with(".tex") {
                    Direction::L2t
                } else {
                    // Use content-based detection
                    let format = detect_format(&input);
                    if format == "latex" {
                        Direction::L2t
                    } else {
                        Direction::T2l
                    }
                }
            } else {
                // Use content-based detection
                let format = detect_format(&input);
                if format == "latex" {
                    Direction::L2t
                } else {
                    Direction::T2l
                }
            }
        }
        d => d,
    };

    // Determine if this is a full document based on content or flag
    let is_full_document = cli.full_document || is_latex_document(&input);

    // Convert with diagnostics - collect warnings as unified CliDiagnostic
    let (result, diagnostics): (String, Vec<CliDiagnostic>) = match direction {
        Direction::L2t => {
            let conv_result = latex_to_typst_with_diagnostics(&input);
            let diags = conv_result
                .warnings
                .into_iter()
                .map(CliDiagnostic::from)
                .collect();
            (conv_result.output, diags)
        }
        Direction::T2l => {
            let options = if is_full_document {
                T2LOptions::full_document()
            } else {
                T2LOptions::default()
            };
            if !cli.no_eval {
                let conv_result = typst_to_latex_with_diagnostics(&input, &options);
                let diags = conv_result
                    .warnings
                    .into_iter()
                    .map(CliDiagnostic::from)
                    .collect();
                (conv_result.output, diags)
            } else {
                let output = if is_full_document {
                    typst_document_to_latex(&input)
                } else {
                    typst_to_latex(&input)
                };
                (output, Vec::new())
            }
        }
        Direction::Auto => {
            let (output, _) = if is_full_document {
                convert_auto_document(&input)
            } else {
                convert_auto(&input)
            };
            (output, Vec::new())
        }
    };

    // Print diagnostics to stderr (unless quiet mode)
    if !cli.quiet && !diagnostics.is_empty() {
        print_diagnostics_to_stderr(&diagnostics, cli.color);
    }

    // Check strict mode
    if cli.strict && !diagnostics.is_empty() {
        eprintln!(
            "Error: {} conversion warning(s) in strict mode",
            diagnostics.len()
        );
        std::process::exit(1);
    }

    // Embed diagnostics as comments if requested
    let result = if cli.embed_warnings && !diagnostics.is_empty() {
        embed_diagnostics_as_comments(&result, &diagnostics)
    } else {
        result
    };

    // Pretty print if requested
    let result = if cli.pretty {
        pretty_print(&result)
    } else {
        result
    };

    // Output
    match cli.output {
        Some(path) => {
            let mut file = fs::File::create(&path)?;
            writeln!(file, "{}", result)?;
            if diagnostics.is_empty() {
                eprintln!("✓ Output written to: {}", path);
            } else {
                eprintln!(
                    "⚠ Output written to: {} ({} warning(s))",
                    path,
                    diagnostics.len()
                );
            }
        }
        None => {
            println!("{}", result);
        }
    }

    Ok(())
}

#[cfg(feature = "cli")]
fn handle_subcommand(cmd: Commands) -> io::Result<()> {
    match cmd {
        Commands::Check { input, no_color } => {
            let content = match input {
                Some(path) => fs::read_to_string(&path)?,
                None => {
                    let mut buffer = String::new();
                    io::stdin().read_to_string(&mut buffer)?;
                    buffer
                }
            };

            let result = check_latex(&content);
            let output = format_diagnostics(&result, !no_color);
            println!("{}", output);

            if result.has_errors() {
                std::process::exit(1);
            }
        }

        Commands::Convert {
            input,
            output,
            direction,
            full_document,
        } => {
            let (content, filename) = match input {
                Some(ref path) => (fs::read_to_string(path)?, Some(path.clone())),
                None => {
                    let mut buffer = String::new();
                    io::stdin().read_to_string(&mut buffer)?;
                    (buffer, None)
                }
            };

            let direction = match direction {
                Direction::Auto => {
                    if let Some(ref name) = filename {
                        if name.ends_with(".typ") {
                            Direction::T2l
                        } else if name.ends_with(".tex") {
                            Direction::L2t
                        } else {
                            let format = detect_format(&content);
                            if format == "latex" {
                                Direction::L2t
                            } else {
                                Direction::T2l
                            }
                        }
                    } else {
                        let format = detect_format(&content);
                        if format == "latex" {
                            Direction::L2t
                        } else {
                            Direction::T2l
                        }
                    }
                }
                d => d,
            };

            let result = if full_document {
                match direction {
                    Direction::L2t => latex_document_to_typst(&content),
                    Direction::T2l => typst_document_to_latex(&content),
                    Direction::Auto => convert_auto_document(&content).0,
                }
            } else {
                match direction {
                    Direction::L2t => latex_to_typst(&content),
                    Direction::T2l => typst_to_latex(&content),
                    Direction::Auto => convert_auto(&content).0,
                }
            };

            match output {
                Some(path) => {
                    let mut file = fs::File::create(&path)?;
                    writeln!(file, "{}", result)?;
                    eprintln!("✓ Output written to: {}", path);
                }
                None => {
                    println!("{}", result);
                }
            }
        }

        Commands::Tikz {
            input,
            output,
            direction,
        } => {
            let content = match input {
                Some(path) => fs::read_to_string(&path)?,
                None => {
                    let mut buffer = String::new();
                    io::stdin().read_to_string(&mut buffer)?;
                    buffer
                }
            };

            let direction = match direction {
                TikzDirection::Auto => {
                    if is_cetz_code(&content) {
                        TikzDirection::CetzToTikz
                    } else {
                        TikzDirection::TikzToCetz
                    }
                }
                d => d,
            };

            let result = match direction {
                TikzDirection::TikzToCetz => convert_tikz_to_cetz(&content),
                TikzDirection::CetzToTikz => convert_cetz_to_tikz(&content),
                TikzDirection::Auto => unreachable!(),
            };

            match output {
                Some(path) => {
                    let mut file = fs::File::create(&path)?;
                    writeln!(file, "{}", result)?;
                    eprintln!("✓ TikZ/CeTZ conversion written to: {}", path);
                }
                None => {
                    println!("{}", result);
                }
            }
        }

        Commands::Batch {
            input,
            output_dir,
            direction,
            full_document,
            extension,
        } => {
            // Create output directory if it doesn't exist
            fs::create_dir_all(&output_dir)?;

            // Determine output extension
            let out_ext = extension.unwrap_or_else(|| match direction {
                Direction::L2t => "typ".to_string(),
                Direction::T2l => "tex".to_string(),
                Direction::Auto => "out".to_string(),
            });

            // Find input files
            let input_path = Path::new(&input);
            let files: Vec<_> = if input_path.is_dir() {
                // Read all .tex or .typ files from directory
                fs::read_dir(input_path)?
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        let path = e.path();
                        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                        matches!(direction, Direction::L2t) && ext == "tex"
                            || matches!(direction, Direction::T2l) && ext == "typ"
                            || matches!(direction, Direction::Auto)
                    })
                    .map(|e| e.path())
                    .collect()
            } else {
                // Single file
                vec![input_path.to_path_buf()]
            };

            let mut success_count = 0;
            let mut error_count = 0;

            for file_path in files {
                let filename = file_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("output");

                let output_path = Path::new(&output_dir).join(format!("{}.{}", filename, out_ext));

                match fs::read_to_string(&file_path) {
                    Ok(content) => {
                        let result = if full_document {
                            match direction {
                                Direction::L2t => latex_document_to_typst(&content),
                                Direction::T2l => typst_document_to_latex(&content),
                                Direction::Auto => convert_auto_document(&content).0,
                            }
                        } else {
                            match direction {
                                Direction::L2t => latex_to_typst(&content),
                                Direction::T2l => typst_to_latex(&content),
                                Direction::Auto => convert_auto(&content).0,
                            }
                        };

                        match fs::write(&output_path, &result) {
                            Ok(_) => {
                                eprintln!("✓ {}", output_path.display());
                                success_count += 1;
                            }
                            Err(e) => {
                                eprintln!("✗ {} - write error: {}", output_path.display(), e);
                                error_count += 1;
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("✗ {} - read error: {}", file_path.display(), e);
                        error_count += 1;
                    }
                }
            }

            eprintln!(
                "\nBatch conversion complete: {} succeeded, {} failed",
                success_count, error_count
            );

            if error_count > 0 {
                std::process::exit(1);
            }
        }

        Commands::Info => {
            println!("Tylax - High-performance bidirectional LaTeX ↔ Typst converter");
            println!("Version: {}", env!("CARGO_PKG_VERSION"));
            println!();
            println!("Features:");
            println!("  ✓ LaTeX → Typst conversion (math + documents)");
            println!("  ✓ Typst → LaTeX conversion (math + documents)");
            println!("  ✓ TikZ ↔ CeTZ graphics conversion");
            println!("  ✓ Batch file processing");
            println!("  ✓ LaTeX diagnostics and checking");
            println!("  ✓ Auto-detection of input format");
            println!();
            println!("Supported packages:");
            println!("  - amsmath, amssymb, mathtools");
            println!("  - graphicx, hyperref, biblatex");
            println!("  - tikz, pgf (basic features)");
            println!("  - siunitx, mhchem");
            println!();
            println!("Repository: https://github.com/scipenai/tylax");
            println!();
        }
    }

    Ok(())
}

/// Detect if input is a full LaTeX document (vs math snippet)
#[cfg(feature = "cli")]
fn is_latex_document(input: &str) -> bool {
    // Check for document structure indicators
    input.contains("\\documentclass")
        || input.contains("\\begin{document}")
        || input.contains("\\section")
        || input.contains("\\chapter")
        || input.contains("\\title")
        || input.contains("\\maketitle")
        || input.contains("\\usepackage")
}

#[cfg(feature = "cli")]
fn pretty_print(input: &str) -> String {
    // Simple pretty printing: normalize indentation and spacing
    let mut result = String::new();
    let mut indent_level: usize = 0;

    for line in input.lines() {
        let trimmed = line.trim();

        // Decrease indent before closing braces/brackets
        if trimmed.starts_with('}') || trimmed.starts_with(']') || trimmed.starts_with(')') {
            indent_level = indent_level.saturating_sub(1);
        }
        if trimmed.starts_with("\\end{") {
            indent_level = indent_level.saturating_sub(1);
        }

        // Add indentation
        for _ in 0..indent_level {
            result.push_str("  ");
        }
        result.push_str(trimmed);
        result.push('\n');

        // Increase indent after opening braces/brackets
        if trimmed.ends_with('{') || trimmed.ends_with('[') {
            indent_level += 1;
        }
        if trimmed.starts_with("\\begin{") {
            indent_level += 1;
        }
    }

    result.trim().to_string()
}

/// Print diagnostics to stderr with optional color coding (unified for L2T and T2L).
#[cfg(feature = "cli")]
fn print_diagnostics_to_stderr(diagnostics: &[CliDiagnostic], use_color: bool) {
    eprintln!();
    eprintln!(
        "{}Conversion Warnings ({}):{}",
        if use_color { "\x1b[33m" } else { "" },
        diagnostics.len(),
        if use_color { "\x1b[0m" } else { "" }
    );
    eprintln!();

    for diag in diagnostics {
        let color = if use_color { diag.color_code() } else { "" };
        let reset = if use_color { "\x1b[0m" } else { "" };

        if let Some(ref loc) = diag.location {
            eprintln!(
                "  {}[{}]{} {}: {}",
                color, diag.kind, reset, loc, diag.message
            );
        } else {
            eprintln!("  {}[{}]{} {}", color, diag.kind, reset, diag.message);
        }
    }
    eprintln!();
}

/// Embed diagnostics as comments at the end of the output (unified for L2T and T2L).
#[cfg(feature = "cli")]
fn embed_diagnostics_as_comments(output: &str, diagnostics: &[CliDiagnostic]) -> String {
    let mut result = output.to_string();
    result.push_str("\n\n// ═══════════════════════════════════════════════════════════════\n");
    result.push_str("// Conversion Warnings\n");
    result.push_str("// ═══════════════════════════════════════════════════════════════\n");

    for diag in diagnostics {
        if let Some(ref loc) = diag.location {
            result.push_str(&format!("// [{}] {}: {}\n", diag.kind, loc, diag.message));
        } else {
            result.push_str(&format!("// [{}] {}\n", diag.kind, diag.message));
        }
    }

    result
}

#[cfg(not(feature = "cli"))]
fn main() {
    eprintln!("CLI feature not enabled. Build with --features cli");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  cargo install tylax --features cli");
    eprintln!("  t2l [OPTIONS] [INPUT_FILE]");
}

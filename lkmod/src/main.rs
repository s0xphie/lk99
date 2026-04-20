fn cmd_source_map(bf_file: &PathBuf) {
    let source = load_bf(bf_file);
    patcher::print_output_regions(&source);
}
use clap::{Parser, Subcommand};
use lkmod::{interpreter, patcher, runner, strings, ternlsb, tui};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "lkmod", about = "Lost Kingdom BF game modding toolkit")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Analyze BF source and print output string regions (for source mapping).
    SourceMap {
        /// Path to the BF source file.
        bf_file: PathBuf,
    },
    /// Run a BF program with input from a file or stdin.
    Run {
        /// Path to the BF source file.
        bf_file: PathBuf,
        /// Optional save/input file (one command per line).
        #[arg(short, long)]
        input: Option<PathBuf>,
        /// Enable tape tracing at each input instruction.
        #[arg(short, long)]
        trace: bool,
    },

    /// Test a save file: run the game and report score, inventory, rank.
    Test {
        /// Path to the BF source file.
        bf_file: PathBuf,
        /// Path to the save file (one command per line).
        save_file: PathBuf,
    },

    /// Extract strings from the game by running multiple scenarios.
    Strings {
        /// Path to the BF source file.
        bf_file: PathBuf,
        /// Optional search pattern (case-insensitive substring).
        #[arg(short, long)]
        find: Option<String>,
    },

    /// Locate a string in the BF source and show its byte positions.
    Locate {
        /// Path to the BF source file.
        bf_file: PathBuf,
        /// The string to locate.
        target: String,
        /// Input to feed the game so it outputs the target string.
        #[arg(short, long)]
        input: Option<PathBuf>,
    },

    /// Patch a string in a BF source file.
    Patch {
        /// Path to the BF source file (will be modified in-place unless --output is given).
        bf_file: PathBuf,
        /// The original string to find and replace.
        original: String,
        /// The replacement string (must be <= original length).
        replacement: String,
        /// Write patched output to this file instead of modifying in-place.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Input to feed the game so it outputs the target string.
        #[arg(short, long)]
        input: Option<PathBuf>,
    },

    /// Raw patch: replace bytes at a given offset.
    PatchRaw {
        /// Path to the BF source file.
        bf_file: PathBuf,
        /// Byte offset in the source.
        #[arg(short, long)]
        offset: usize,
        /// Replacement bytes as a string of BF chars.
        replacement: String,
        /// Write patched output to this file.
        #[arg(short = 'O', long)]
        output: Option<PathBuf>,
    },

    /// Encode a BF program (and optional save) into a TernLSB PNG.
    Encode {
        /// Source PNG image.
        source_png: PathBuf,
        /// BF source file.
        bf_file: PathBuf,
        /// Output PNG file.
        output_png: PathBuf,
        /// Optional save file to embed.
        #[arg(short, long)]
        save: Option<PathBuf>,
    },

    /// Decode a BF program (and optional save) from a TernLSB PNG.
    Decode {
        /// Encoded TernLSB PNG.
        encoded_png: PathBuf,
        /// Write decoded BF source to this file.
        #[arg(short, long)]
        bf_output: Option<PathBuf>,
        /// Write decoded save data to this file.
        #[arg(short, long)]
        save_output: Option<PathBuf>,
    },

    /// Show BF source structure: op count, input count, bracket depth.
    Info {
        /// Path to the BF source file.
        bf_file: PathBuf,
    },

    /// Decode a BF program from a bfsteg-format PNG (low 3 bits).
    DecodeBfsteg {
        /// Encoded bfsteg PNG.
        encoded_png: PathBuf,
        /// Write decoded BF source to this file.
        #[arg(short, long)]
        bf_output: Option<PathBuf>,
    },

    /// Encode a BF program into a bfsteg-format PNG (low 3 bits).
    EncodeBfsteg {
        /// Source PNG image.
        source_png: PathBuf,
        /// BF source file.
        bf_file: PathBuf,
        /// Output PNG file.
        output_png: PathBuf,
    },

    /// Auto-detect PNG format (ternlsb or bfsteg) and decode.
    AutoDecode {
        /// Encoded PNG (ternlsb or bfsteg).
        encoded_png: PathBuf,
        /// Write decoded BF source to this file.
        #[arg(short, long)]
        bf_output: Option<PathBuf>,
        /// Write decoded save data to this file (ternlsb only).
        #[arg(short, long)]
        save_output: Option<PathBuf>,
    },

    /// Databend: XOR two BF encodings to visualise the mod's diff on the carrier image.
    Databend {
        /// Carrier PNG image.
        source_png: PathBuf,
        /// Original BF source file.
        original: PathBuf,
        /// Modded BF source file.
        modded: PathBuf,
        /// Output PNG file.
        output_png: PathBuf,
        /// Visualisation mode: xor, amplify, or blend.
        #[arg(short, long, default_value = "blend")]
        mode: String,
    },

    /// Apply a databend XOR patch to reconstruct a modded PNG from the original.
    ApplyDatabend {
        /// Original encoded PNG (ternlsb).
        original_png: PathBuf,
        /// XOR patch PNG (from `databend --mode xor`).
        xor_patch: PathBuf,
        /// Output modded PNG.
        output_png: PathBuf,
    },

    /// Interactive TUI: navigate source map and edit strings.
    Tui {
        /// Path to the BF source file.
        bf_file: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run { bf_file, input, trace } => cmd_run(&bf_file, input.as_deref(), trace),
        Commands::Test { bf_file, save_file } => cmd_test(&bf_file, &save_file),
        Commands::Strings { bf_file, find } => cmd_strings(&bf_file, find.as_deref()),
        Commands::Locate { bf_file, target, input } => cmd_locate(&bf_file, &target, input.as_deref()),
        Commands::Patch { bf_file, original, replacement, output, input } => {
            cmd_patch(&bf_file, &original, &replacement, output.as_deref(), input.as_deref())
        }
        Commands::PatchRaw { bf_file, offset, replacement, output } => {
            cmd_patch_raw(&bf_file, offset, &replacement, output.as_deref())
        }
        Commands::Encode { source_png, bf_file, output_png, save } => {
            cmd_encode(&source_png, &bf_file, &output_png, save.as_deref())
        }
        Commands::Decode { encoded_png, bf_output, save_output } => {
            cmd_decode(&encoded_png, bf_output.as_deref(), save_output.as_deref())
        }
        Commands::DecodeBfsteg { encoded_png, bf_output } => {
            cmd_decode_bfsteg(&encoded_png, bf_output.as_deref())
        }
        Commands::EncodeBfsteg { source_png, bf_file, output_png } => {
            cmd_encode_bfsteg(&source_png, &bf_file, &output_png)
        }
        Commands::AutoDecode { encoded_png, bf_output, save_output } => {
            cmd_auto_decode(&encoded_png, bf_output.as_deref(), save_output.as_deref())
        }
        Commands::Databend { source_png, original, modded, output_png, mode } => {
            cmd_databend(&source_png, &original, &modded, &output_png, &mode)
        }
        Commands::ApplyDatabend { original_png, xor_patch, output_png } => {
            cmd_apply_databend(&original_png, &xor_patch, &output_png)
        }
        Commands::Info { bf_file } => cmd_info(&bf_file),
        Commands::Tui { bf_file } => {
            if let Err(e) = tui::run(bf_file) {
                eprintln!("TUI error: {}", e);
                std::process::exit(1);
            }
        }
        Commands::SourceMap { bf_file } => {
            cmd_source_map(&bf_file);
        }
        }
}

fn load_bf(path: &PathBuf) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| {
        eprintln!("Error reading {}: {}", path.display(), e);
        std::process::exit(1);
    })
}

fn cmd_run(bf_file: &PathBuf, input_file: Option<&std::path::Path>, trace: bool) {
    let source = load_bf(bf_file);
    let prog = interpreter::Program::compile(&source);
    eprintln!("Compiled: {} ops", prog.op_count());

    let input = if let Some(path) = input_file {
        std::fs::read(path).unwrap_or_else(|e| {
            eprintln!("Error reading input: {}", e);
            std::process::exit(1);
        })
    } else {
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut std::io::stdin(), &mut buf).unwrap();
        buf
    };

    let result = prog.run(&input, trace);
    std::io::Write::write_all(&mut std::io::stdout(), &result.output).unwrap();

    if trace {
        eprintln!("\n--- Tape Snapshots ({}) ---", result.tape_snapshots.len());
        for snap in &result.tape_snapshots {
            let out_str = String::from_utf8_lossy(&snap.output_since);
            let out_preview: String = out_str.chars().take(80).collect();
            eprintln!(
                "INPUT #{} char={} '{}' ptr={} src={} out=|{}|",
                snap.input_index,
                snap.input_char,
                snap.input_char as char,
                snap.ptr,
                snap.src_pos,
                out_preview.replace('\n', "\\n"),
            );
            let nonzero: Vec<String> = snap
                .nearby
                .iter()
                .map(|(off, val)| {
                    if *val >= 32 && *val < 127 {
                        format!("{:+}={}'{}'", off, val, *val as char)
                    } else {
                        format!("{:+}={}", off, val)
                    }
                })
                .collect();
            eprintln!("  NONZERO: {}", nonzero.join(" "));
        }
    }

    eprintln!("Inputs consumed: {}", result.inputs_consumed);
}

fn cmd_test(bf_file: &PathBuf, save_file: &PathBuf) {
    let source = load_bf(bf_file);
    let prog = interpreter::Program::compile(&source);
    let save_text = std::fs::read_to_string(save_file).unwrap_or_else(|e| {
        eprintln!("Error reading save: {}", e);
        std::process::exit(1);
    });

    let result = runner::test_save(&prog, &save_text);
    print!("{}", result);
}

fn cmd_strings(bf_file: &PathBuf, find: Option<&str>) {
    let source = load_bf(bf_file);
    let prog = interpreter::Program::compile(&source);
    let scenarios = strings::lk_default_scenarios();
    let scenario_refs: Vec<(&str, Vec<u8>)> = scenarios.iter().map(|(n, d)| (*n, d.clone())).collect();
    let extracted = strings::extract_multi(&prog, &scenario_refs);

    if let Some(pattern) = find {
        let matches = strings::find_string(&extracted, pattern);
        for s in matches {
            println!("[{}] {}", s.scenario, s.text);
        }
    } else {
        for s in &extracted {
            println!("[{}] {}", s.scenario, s.text);
        }
    }
    eprintln!("Total strings: {}", extracted.len());
}

fn cmd_locate(bf_file: &PathBuf, target: &str, input_file: Option<&std::path::Path>) {
    let source = load_bf(bf_file);

    let input = if let Some(path) = input_file {
        std::fs::read(path).unwrap_or_else(|e| {
            eprintln!("Error reading input: {}", e);
            std::process::exit(1);
        })
    } else {
        b"N\n".to_vec()
    };

    match patcher::locate_string(&source, &input, target) {
        Some(loc) => {
            println!("Found \"{}\" ({} chars)", target, loc.chars.len());
            for (i, (&pos, &ch)) in loc.dot_positions.iter().zip(loc.chars.iter()).enumerate() {
                println!(
                    "  [{}] '{}' (0x{:02x}) at source byte {}",
                    i, ch as char, ch, pos
                );
            }
        }
        None => {
            eprintln!("String \"{}\" not found in game output", target);
            std::process::exit(1);
        }
    }
}

fn cmd_patch(
    bf_file: &PathBuf,
    original: &str,
    replacement: &str,
    output: Option<&std::path::Path>,
    input_file: Option<&std::path::Path>,
) {
    let mut source = load_bf(bf_file);

    let input = if let Some(path) = input_file {
        std::fs::read(path).unwrap_or_else(|e| {
            eprintln!("Error reading input: {}", e);
            std::process::exit(1);
        })
    } else {
        b"N\n".to_vec()
    };

    let location = match patcher::locate_string(&source, &input, original) {
        Some(loc) => loc,
        None => {
            eprintln!("String \"{}\" not found in game output", original);
            std::process::exit(1);
        }
    };

    if let Err(e) = patcher::patch_string(&mut source, &location, replacement) {
        eprintln!("Patch error: {}", e);
        std::process::exit(1);
    }

    let out_path = output.unwrap_or(bf_file.as_path());
    std::fs::write(out_path, &source).unwrap_or_else(|e| {
        eprintln!("Error writing: {}", e);
        std::process::exit(1);
    });
    eprintln!(
        "Patched \"{}\" -> \"{}\" in {}",
        original,
        replacement,
        out_path.display()
    );
}

fn cmd_patch_raw(bf_file: &PathBuf, offset: usize, replacement: &str, output: Option<&std::path::Path>) {
    let mut source = load_bf(bf_file);
    if let Err(e) = patcher::patch_raw(&mut source, offset, replacement.as_bytes()) {
        eprintln!("Patch error: {}", e);
        std::process::exit(1);
    }
    let out_path = output.unwrap_or(bf_file.as_path());
    std::fs::write(out_path, &source).unwrap_or_else(|e| {
        eprintln!("Error writing: {}", e);
        std::process::exit(1);
    });
    eprintln!("Raw-patched {} bytes at offset {} in {}", replacement.len(), offset, out_path.display());
}

fn cmd_encode(source_png: &PathBuf, bf_file: &PathBuf, output_png: &PathBuf, save_file: Option<&std::path::Path>) {
    let bf_source = load_bf(bf_file);
    let save_text = save_file.map(|p| {
        std::fs::read_to_string(p).unwrap_or_else(|e| {
            eprintln!("Error reading save: {}", e);
            std::process::exit(1);
        })
    });

    if let Err(e) = ternlsb::encode(source_png, &bf_source, save_text.as_deref(), output_png) {
        eprintln!("Encode error: {}", e);
        std::process::exit(1);
    }
    eprintln!("Encoded to {}", output_png.display());
}

fn cmd_decode(encoded_png: &PathBuf, bf_output: Option<&std::path::Path>, save_output: Option<&std::path::Path>) {
    match ternlsb::decode(encoded_png) {
        Ok((bf_source, save_text)) => {
            if let Some(path) = bf_output {
                std::fs::write(path, &bf_source).unwrap_or_else(|e| {
                    eprintln!("Error writing BF: {}", e);
                    std::process::exit(1);
                });
                eprintln!("BF source: {} bytes -> {}", bf_source.len(), path.display());
            } else {
                eprintln!("BF source: {} bytes (use -b to save)", bf_source.len());
            }

            if let Some(save) = &save_text {
                if let Some(path) = save_output {
                    std::fs::write(path, save).unwrap_or_else(|e| {
                        eprintln!("Error writing save: {}", e);
                        std::process::exit(1);
                    });
                    eprintln!("Save data: {} bytes -> {}", save.len(), path.display());
                } else {
                    eprintln!("Save data: {} bytes (use -s to save)", save.len());
                }
            } else {
                eprintln!("No save data embedded.");
            }
        }
        Err(e) => {
            eprintln!("Decode error: {}", e);
            std::process::exit(1);
        }
    }
}

fn cmd_decode_bfsteg(encoded_png: &PathBuf, bf_output: Option<&std::path::Path>) {
    match ternlsb::bfsteg_decode(encoded_png) {
        Ok(bf_source) => {
            if let Some(path) = bf_output {
                std::fs::write(path, &bf_source).unwrap_or_else(|e| {
                    eprintln!("Error writing BF: {}", e);
                    std::process::exit(1);
                });
                eprintln!("BF source: {} bytes -> {}", bf_source.len(), path.display());
            } else {
                eprintln!("BF source: {} bytes (use -b to save)", bf_source.len());
            }
        }
        Err(e) => {
            eprintln!("bfsteg decode error: {}", e);
            std::process::exit(1);
        }
    }
}

fn cmd_encode_bfsteg(source_png: &PathBuf, bf_file: &PathBuf, output_png: &PathBuf) {
    let bf_source = load_bf(bf_file);
    if let Err(e) = ternlsb::bfsteg_encode(source_png, &bf_source, output_png) {
        eprintln!("bfsteg encode error: {}", e);
        std::process::exit(1);
    }
    eprintln!("Encoded (bfsteg) to {}", output_png.display());
}

fn cmd_auto_decode(encoded_png: &PathBuf, bf_output: Option<&std::path::Path>, save_output: Option<&std::path::Path>) {
    match ternlsb::auto_decode(encoded_png) {
        Ok((bf_source, save_text, format)) => {
            eprintln!("Detected format: {}", format);
            if let Some(path) = bf_output {
                std::fs::write(path, &bf_source).unwrap_or_else(|e| {
                    eprintln!("Error writing BF: {}", e);
                    std::process::exit(1);
                });
                eprintln!("BF source: {} bytes -> {}", bf_source.len(), path.display());
            } else {
                eprintln!("BF source: {} bytes (use -b to save)", bf_source.len());
            }

            if let Some(save) = &save_text {
                if let Some(path) = save_output {
                    std::fs::write(path, save).unwrap_or_else(|e| {
                        eprintln!("Error writing save: {}", e);
                        std::process::exit(1);
                    });
                    eprintln!("Save data: {} bytes -> {}", save.len(), path.display());
                } else {
                    eprintln!("Save data: {} bytes (use -s to save)", save.len());
                }
            }
        }
        Err(e) => {
            eprintln!("Auto-decode error: {}", e);
            std::process::exit(1);
        }
    }
}

fn cmd_databend(
    source_png: &PathBuf,
    original: &PathBuf,
    modded: &PathBuf,
    output_png: &PathBuf,
    mode: &str,
) {
    let orig_bf = load_bf(original);
    let mod_bf = load_bf(modded);
    match ternlsb::databend(source_png, &orig_bf, &mod_bf, output_png, mode) {
        Ok(stats) => {
            eprintln!("Databend ({}) -> {}", mode, output_png.display());
            eprintln!("Original BF: {} instructions", stats.original_bf_len);
            eprintln!("Modded BF:   {} instructions", stats.modded_bf_len);
            eprintln!("Insertion delta: {:+} bytes", stats.insertion_delta);
            eprintln!("Pixel diff: {}/{} bytes ({:.2}%)",
                stats.diff_bytes, stats.total_bytes,
                stats.diff_bytes as f64 / stats.total_bytes as f64 * 100.0);
            if let (Some(first), Some(last)) = (stats.first_diff_byte, stats.last_diff_byte) {
                eprintln!("Diff range: byte {} to {} (span: {})",
                    first, last, last - first + 1);
            }
        }
        Err(e) => {
            eprintln!("Databend error: {}", e);
            std::process::exit(1);
        }
    }
}

fn cmd_apply_databend(
    original_png: &PathBuf,
    xor_patch: &PathBuf,
    output_png: &PathBuf,
) {
    match ternlsb::apply_databend(original_png, xor_patch, output_png) {
        Ok(diff_count) => {
            eprintln!("Applied XOR patch: {} bytes modified -> {}", diff_count, output_png.display());
        }
        Err(e) => {
            eprintln!("Apply databend error: {}", e);
            std::process::exit(1);
        }
    }
}

fn cmd_info(bf_file: &PathBuf) {
    let source = load_bf(bf_file);
    let prog = interpreter::Program::compile(&source);

    let bf_count = source.iter().filter(|&&c| b"+-<>.,[]".contains(&c)).count();
    let input_count = source.iter().filter(|&&c| c == b',').count();
    let output_count = source.iter().filter(|&&c| c == b'.').count();

    let mut max_depth = 0u32;
    let mut depth = 0u32;
    for &c in &source {
        if c == b'[' { depth += 1; max_depth = max_depth.max(depth); }
        if c == b']' { depth = depth.saturating_sub(1); }
    }

    println!("File: {}", bf_file.display());
    println!("Raw size: {} bytes", source.len());
    println!("BF instructions: {}", bf_count);
    println!("Compiled ops: {}", prog.op_count());
    println!("Input (,) instructions: {}", input_count);
    println!("Output (.) instructions: {}", output_count);
    println!("Max bracket depth: {}", max_depth);
}

use anyhow::Result;
use clap::Parser as ClapParser;
use std::path::PathBuf;

mod analyzer;
mod collections;
mod cpi_helpers;
mod emitter;
mod idl;
mod ir;
mod parser;
mod transformer;
mod zero_copy;

#[derive(ClapParser, Debug)]
#[command(name = "uncpi")]
#[command(
    about = "\"ok unc, let me show you how to optimize\" - Transpile Anchor to Pinocchio for 85%+ size reduction"
)]
struct Args {
    /// Input Anchor program (can be a lib.rs file or a program directory)
    #[arg(required = true)]
    input: PathBuf,

    /// Output directory for Pinocchio program
    #[arg(short, long, default_value = "output")]
    output: PathBuf,

    /// Use no_allocator! for maximum size reduction
    #[arg(long)]
    no_alloc: bool,

    /// Use lazy_program_entrypoint! for on-demand parsing
    #[arg(long)]
    lazy_entrypoint: bool,

    /// Inline CPI calls where possible
    #[arg(long)]
    inline_cpi: bool,

    /// Generate IDL-compatible discriminators (8-byte Anchor style) - enabled by default
    #[arg(long, default_value = "true")]
    anchor_compat: bool,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Generate IDL JSON file
    #[arg(long)]
    idl: bool,

    /// Program ID for IDL metadata
    #[arg(long)]
    program_id: Option<String>,

    /// Strip msg!() calls for smaller binary size
    #[arg(long)]
    no_logs: bool,

    /// Use unchecked math operations for smaller binary (unsafe but faster)
    #[arg(long)]
    unsafe_math: bool,

    /// Verify generated IDL against original Anchor IDL
    #[arg(long)]
    verify_idl: Option<PathBuf>,
}

fn main() -> Result<()> {
    // Configure rayon thread pool to use 75% of available cores globally
    let num_cores = num_cpus::get();
    let target_threads = (num_cores as f32 * 0.75).ceil() as usize;

    rayon::ThreadPoolBuilder::new()
        .num_threads(target_threads)
        .build_global()
        .ok(); // Ignore if already initialized

    let args = Args::parse();

    // Resolve input path - if it's a directory, look for src/lib.rs
    let input_file = if args.input.is_dir() {
        let lib_path = args.input.join("src").join("lib.rs");
        if !lib_path.exists() {
            anyhow::bail!(
                "Input is a directory but src/lib.rs not found. Expected: {:?}",
                lib_path
            );
        }
        lib_path
    } else {
        args.input.clone()
    };

    // Resolve output path - if input was a folder, derive output name from folder
    let output_dir = if args.output.as_os_str() == "output" && args.input.is_dir() {
        // Default output case - create a better default based on input folder name
        let folder_name = args
            .input
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("output");
        std::path::PathBuf::from("/tmp").join(format!("{}-pino", folder_name))
    } else {
        args.output.clone()
    };

    if args.verbose {
        println!("uncpi v{}", env!("CARGO_PKG_VERSION"));
        println!("Input:  {:?}", input_file);
        println!("Output: {:?}", output_dir);
    }

    // Phase 1: Parse Anchor source
    if args.verbose {
        println!("\n[1/4] Parsing Anchor program...");
    }
    let anchor_program = parser::parse_anchor_file(&input_file)?;

    if args.verbose {
        println!("  Found {} instructions", anchor_program.instructions.len());
        println!(
            "  Found {} account structs",
            anchor_program.account_structs.len()
        );
        println!(
            "  Found {} state structs",
            anchor_program.state_structs.len()
        );
    }

    // Phase 2: Analyze
    if args.verbose {
        println!("\n[2/4] Analyzing program...");
    }
    let analysis = analyzer::analyze(&anchor_program)?;

    if args.verbose {
        println!("  PDAs: {}", analysis.pdas.len());
        println!("  CPIs: {}", analysis.cpi_calls.len());
    }

    // Phase 3: Transform to Pinocchio IR
    if args.verbose {
        println!("\n[3/4] Transforming to Pinocchio IR...");
    }
    let config = transformer::Config {
        no_alloc: args.no_alloc,
        lazy_entrypoint: args.lazy_entrypoint,
        inline_cpi: args.inline_cpi,
        anchor_compat: args.anchor_compat,
        no_logs: args.no_logs,
        unsafe_math: args.unsafe_math,
    };
    let pinocchio_ir = transformer::transform(&anchor_program, &analysis, &config)?;

    // Phase 3.5: Extract constants and helpers
    if args.verbose {
        println!("\n[3.5/4] Extracting constants and helpers...");
    }
    let extras = parser::parse_extras(&input_file)?;
    if args.verbose {
        println!("  Constants: {}", extras.constants.len());
        println!("  Helper functions: {}", extras.helper_functions.len());
    }

    // Phase 4: Emit Pinocchio code
    if args.verbose {
        println!("\n[4/4] Emitting Pinocchio code...");
    }
    emitter::emit_with_extras(&pinocchio_ir, &output_dir, Some(&extras))?;

    // Phase 5: Generate IDL if requested
    if args.idl || args.verify_idl.is_some() {
        if args.verbose {
            println!("\n[5/5] Generating IDL...");
        }
        let idl = idl::generate_idl(&pinocchio_ir, args.program_id.as_deref());
        let idl_path = output_dir.join("idl.json");
        let idl_json = serde_json::to_string_pretty(&idl)?;
        std::fs::write(&idl_path, &idl_json)?;
        if args.verbose {
            println!("  IDL written to {:?}", idl_path);
        }

        // Verify against original IDL if provided
        if let Some(original_idl_path) = &args.verify_idl {
            if args.verbose {
                println!("\n[6/6] Verifying IDL compatibility...");
            }
            let verification = idl::verify_idl(&idl, original_idl_path)?;
            if verification.is_compatible {
                println!("\n✅ IDL VERIFICATION PASSED");
                println!(
                    "  Instructions: {}/{} match",
                    verification.matching_instructions, verification.total_instructions
                );
                println!(
                    "  Accounts: {}/{} match",
                    verification.matching_accounts, verification.total_accounts
                );
                println!(
                    "  Errors: {}/{} match",
                    verification.matching_errors, verification.total_errors
                );
            } else {
                println!("\n❌ IDL VERIFICATION FAILED");
                for issue in &verification.issues {
                    println!("  - {}", issue);
                }
                std::process::exit(1);
            }
        }
    }

    println!("\nSuccess! Pinocchio program written to {:?}", output_dir);
    println!("\nNext steps:");
    println!("  1. cd {:?}", output_dir);
    println!("  2. cargo build-sbf");
    println!("  3. Compare .so sizes!");

    Ok(())
}

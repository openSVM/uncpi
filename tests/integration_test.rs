use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

fn anchor2pinocchio_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");
    path.push("debug");
    path.push("anchor2pinocchio");
    path
}

fn get_test_input() -> PathBuf {
    // Use the idl-stableswap program as test input
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop(); // tools
    path.pop(); // idlhub
    path.push("programs");
    path.push("idl-stableswap");
    path.push("src");
    path.push("lib.rs");
    path
}

#[test]
fn test_transpiler_runs() {
    let output_dir = TempDir::new().unwrap();
    let input = get_test_input();

    if !input.exists() {
        eprintln!("Skipping test - input file not found: {:?}", input);
        return;
    }

    let status = Command::new(anchor2pinocchio_path())
        .arg(&input)
        .arg("-o")
        .arg(output_dir.path())
        .status()
        .expect("Failed to run anchor2pinocchio");

    assert!(status.success(), "Transpiler should succeed");
}

#[test]
fn test_output_structure() {
    let output_dir = TempDir::new().unwrap();
    let input = get_test_input();

    if !input.exists() {
        eprintln!("Skipping test - input file not found: {:?}", input);
        return;
    }

    let status = Command::new(anchor2pinocchio_path())
        .arg(&input)
        .arg("-o")
        .arg(output_dir.path())
        .status()
        .expect("Failed to run anchor2pinocchio");

    assert!(status.success());

    // Check expected output files
    assert!(
        output_dir.path().join("Cargo.toml").exists(),
        "Should generate Cargo.toml"
    );
    assert!(
        output_dir.path().join("src").join("lib.rs").exists(),
        "Should generate src/lib.rs"
    );
    assert!(
        output_dir.path().join("src").join("state.rs").exists(),
        "Should generate src/state.rs"
    );
    assert!(
        output_dir.path().join("src").join("error.rs").exists(),
        "Should generate src/error.rs"
    );
    assert!(
        output_dir.path().join("src").join("helpers.rs").exists(),
        "Should generate src/helpers.rs"
    );
    assert!(
        output_dir.path().join("src").join("instructions").exists(),
        "Should generate instructions dir"
    );
}

#[test]
fn test_generated_code_compiles() {
    let output_dir = TempDir::new().unwrap();
    let input = get_test_input();

    if !input.exists() {
        eprintln!("Skipping test - input file not found: {:?}", input);
        return;
    }

    let status = Command::new(anchor2pinocchio_path())
        .arg(&input)
        .arg("-o")
        .arg(output_dir.path())
        .status()
        .expect("Failed to run anchor2pinocchio");

    assert!(status.success());

    // Try to compile the generated code with SBF toolchain
    // Note: requires solana-platform-tools to be installed
    let compile_status = Command::new("cargo")
        .arg("build-sbf")
        .current_dir(output_dir.path())
        .status();

    match compile_status {
        Ok(status) => {
            assert!(
                status.success(),
                "Generated code should compile without errors"
            );
        }
        Err(e) => {
            eprintln!(
                "Skipping SBF compile test - cargo build-sbf not available: {}",
                e
            );
        }
    }
}

#[test]
fn test_generated_code_has_no_warnings() {
    let output_dir = TempDir::new().unwrap();
    let input = get_test_input();

    if !input.exists() {
        eprintln!("Skipping test - input file not found: {:?}", input);
        return;
    }

    let status = Command::new(anchor2pinocchio_path())
        .arg(&input)
        .arg("-o")
        .arg(output_dir.path())
        .status()
        .expect("Failed to run anchor2pinocchio");

    assert!(status.success());

    // Try to compile with deny warnings using SBF toolchain
    let compile_output = Command::new("cargo")
        .arg("build-sbf")
        .env("RUSTFLAGS", "-D warnings")
        .current_dir(output_dir.path())
        .output();

    match compile_output {
        Ok(output) => {
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!("Compiler warnings/errors:\n{}", stderr);
            }
            assert!(
                output.status.success(),
                "Generated code should have no warnings"
            );
        }
        Err(e) => {
            eprintln!(
                "Skipping SBF warnings test - cargo build-sbf not available: {}",
                e
            );
        }
    }
}

#[test]
fn test_instruction_count() {
    let output_dir = TempDir::new().unwrap();
    let input = get_test_input();

    if !input.exists() {
        eprintln!("Skipping test - input file not found: {:?}", input);
        return;
    }

    let status = Command::new(anchor2pinocchio_path())
        .arg(&input)
        .arg("-o")
        .arg(output_dir.path())
        .status()
        .expect("Failed to run anchor2pinocchio");

    assert!(status.success());

    // Count instruction files
    let instructions_dir = output_dir.path().join("src").join("instructions");
    let instruction_files: Vec<_> = std::fs::read_dir(&instructions_dir)
        .expect("Should read instructions dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ext| ext == "rs").unwrap_or(false))
        .filter(|e| e.file_name() != "mod.rs")
        .collect();

    // The stableswap has 22 instructions (excluding mod.rs)
    assert!(
        instruction_files.len() >= 20,
        "Should generate at least 20 instruction files, got {}",
        instruction_files.len()
    );
}

#[test]
fn test_discriminators_are_unique() {
    let output_dir = TempDir::new().unwrap();
    let input = get_test_input();

    if !input.exists() {
        eprintln!("Skipping test - input file not found: {:?}", input);
        return;
    }

    let status = Command::new(anchor2pinocchio_path())
        .arg(&input)
        .arg("-o")
        .arg(output_dir.path())
        .status()
        .expect("Failed to run anchor2pinocchio");

    assert!(status.success());

    // Read lib.rs and extract discriminators
    let lib_content = std::fs::read_to_string(output_dir.path().join("src").join("lib.rs"))
        .expect("Should read lib.rs");

    let mut discriminators: Vec<String> = Vec::new();
    for line in lib_content.lines() {
        if line.contains("_DISC: [u8; 8]") {
            // Extract the discriminator value
            if let Some(start) = line.find('[') {
                if let Some(end) = line.rfind(']') {
                    discriminators.push(line[start..=end].to_string());
                }
            }
        }
    }

    // Check uniqueness
    let unique_count = {
        let mut sorted = discriminators.clone();
        sorted.sort();
        sorted.dedup();
        sorted.len()
    };

    assert_eq!(
        discriminators.len(),
        unique_count,
        "All discriminators should be unique. Found {} total, {} unique",
        discriminators.len(),
        unique_count
    );
}

#[test]
fn test_state_structs_generated() {
    let output_dir = TempDir::new().unwrap();
    let input = get_test_input();

    if !input.exists() {
        eprintln!("Skipping test - input file not found: {:?}", input);
        return;
    }

    let status = Command::new(anchor2pinocchio_path())
        .arg(&input)
        .arg("-o")
        .arg(output_dir.path())
        .status()
        .expect("Failed to run anchor2pinocchio");

    assert!(status.success());

    // Read state.rs and check for structs
    let state_content = std::fs::read_to_string(output_dir.path().join("src").join("state.rs"))
        .expect("Should read state.rs");

    assert!(
        state_content.contains("pub struct StablePool"),
        "Should have StablePool struct"
    );
    assert!(
        state_content.contains("pub struct FarmingPeriod"),
        "Should have FarmingPeriod struct"
    );
    assert!(
        state_content.contains("pub struct UserFarmingPosition"),
        "Should have UserFarmingPosition struct"
    );

    // Check for SIZE constants
    assert!(
        state_content.contains("const SIZE:"),
        "State structs should have SIZE constants"
    );

    // Check for from_account_info methods
    assert!(
        state_content.contains("fn from_account_info"),
        "State structs should have from_account_info"
    );
    assert!(
        state_content.contains("fn from_account_info_mut"),
        "State structs should have from_account_info_mut"
    );
}

#[test]
fn test_error_enum_generated() {
    let output_dir = TempDir::new().unwrap();
    let input = get_test_input();

    if !input.exists() {
        eprintln!("Skipping test - input file not found: {:?}", input);
        return;
    }

    let status = Command::new(anchor2pinocchio_path())
        .arg(&input)
        .arg("-o")
        .arg(output_dir.path())
        .status()
        .expect("Failed to run anchor2pinocchio");

    assert!(status.success());

    // Read error.rs and check for errors
    let error_content = std::fs::read_to_string(output_dir.path().join("src").join("error.rs"))
        .expect("Should read error.rs");

    assert!(
        error_content.contains("pub enum Error"),
        "Should have Error enum"
    );
    assert!(
        error_content.contains("MathOverflow"),
        "Should have MathOverflow error"
    );
    assert!(
        error_content.contains("SlippageExceeded"),
        "Should have SlippageExceeded error"
    );
    assert!(
        error_content.contains("PoolPaused"),
        "Should have PoolPaused error"
    );
}

#[test]
fn test_verbose_output() {
    let output_dir = TempDir::new().unwrap();
    let input = get_test_input();

    if !input.exists() {
        eprintln!("Skipping test - input file not found: {:?}", input);
        return;
    }

    let output = Command::new(anchor2pinocchio_path())
        .arg(&input)
        .arg("-o")
        .arg(output_dir.path())
        .arg("-v")
        .output()
        .expect("Failed to run anchor2pinocchio");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("[1/4]"),
        "Verbose output should show phase 1"
    );
    assert!(
        stdout.contains("[2/4]"),
        "Verbose output should show phase 2"
    );
    assert!(
        stdout.contains("[3/4]"),
        "Verbose output should show phase 3"
    );
    assert!(
        stdout.contains("[4/4]"),
        "Verbose output should show phase 4"
    );
    assert!(
        stdout.contains("instructions"),
        "Verbose output should mention instructions"
    );
}

#[test]
fn test_binary_size_reduction() {
    let output_dir = TempDir::new().unwrap();
    let input = get_test_input();

    if !input.exists() {
        eprintln!("Skipping test - input file not found: {:?}", input);
        return;
    }

    let status = Command::new(anchor2pinocchio_path())
        .arg(&input)
        .arg("-o")
        .arg(output_dir.path())
        .status()
        .expect("Failed to run anchor2pinocchio");

    assert!(status.success());

    // Build with SBF toolchain
    let compile_status = Command::new("cargo")
        .arg("build-sbf")
        .current_dir(output_dir.path())
        .status();

    match compile_status {
        Ok(status) if status.success() => {
            // Find the .so file
            let deploy_dir = output_dir.path().join("target").join("deploy");
            if deploy_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&deploy_dir) {
                    for entry in entries.filter_map(|e| e.ok()) {
                        let path = entry.path();
                        if path.extension().is_some_and(|ext| ext == "so") {
                            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

                            // Pinocchio binary should be under 100KB (typically 87KB)
                            // This is a regression test to catch size increases
                            assert!(
                                size < 150_000,
                                "Binary size {} bytes exceeds 150KB limit. \
                                Check for optimization regressions.",
                                size
                            );

                            // Verify it's a substantial program (not empty/stub)
                            assert!(
                                size > 10_000,
                                "Binary size {} bytes is suspiciously small. \
                                Verify program content.",
                                size
                            );

                            println!(
                                "Binary size: {} bytes ({:.1}KB)",
                                size,
                                size as f64 / 1024.0
                            );
                        }
                    }
                }
            }
        }
        _ => {
            eprintln!("Skipping binary size test - cargo build-sbf not available");
        }
    }
}

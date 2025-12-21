# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

uncpi is a transpiler that converts Anchor programs to Pinocchio programs, achieving 85-90% binary size reduction for Solana programs. This translates to significant deployment cost savings (~90%) and reduced compute units (60-75%).

## Build and Test Commands

```bash
# Build the transpiler
cargo build

# Build release version
cargo build --release

# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run a specific test
cargo test test_transpiler_runs

# Install locally
cargo install --path .

# Run the transpiler
cargo run -- input.rs -o output/

# Run with verbose output
cargo run -- input.rs -o output/ --verbose
```

## CLI Usage

```bash
# Basic transpilation
uncpi programs/my_program/src/lib.rs -o programs/my_program_pino/

# With optimization flags
uncpi input.rs \
    --no-alloc \           # Use no_allocator! for max size savings
    --inline-cpi \         # Inline CPI calls
    --lazy-entrypoint \    # Use lazy_program_entrypoint!
    --no-logs \            # Strip msg!() calls
    --unsafe-math \        # Use unchecked math operations
    -o output/

# Generate IDL
uncpi input.rs --idl --program-id "YourProgramIDHere" -o output/

# Verify IDL compatibility
uncpi input.rs --verify-idl path/to/original-idl.json -o output/
```

## Architecture

The transpiler follows a four-phase pipeline:

### 1. Parser (`src/parser/mod.rs`)
- Parses Anchor source code using `syn` crate
- Extracts program metadata, instructions, account structs, and state structs
- Extracts constants and helper functions via `parse_extras()`
- Outputs: `AnchorProgram` IR + `SourceExtras`

### 2. Analyzer (`src/analyzer/mod.rs`)
- Analyzes the parsed Anchor program structure
- Extracts PDA information from account constraints
- Identifies CPI calls in instruction bodies
- Calculates account sizes for state structs
- Outputs: `ProgramAnalysis` with PDAs, CPI calls, and sizes

### 3. Transformer (`src/transformer/mod.rs`)
- Transforms Anchor IR to Pinocchio IR
- Converts account constraints to explicit validations
- Generates instruction discriminators (1-byte or 8-byte Anchor-compatible)
- Maps Anchor constraints to Pinocchio validation checks
- Applies optimization flags (no_alloc, lazy_entrypoint, inline_cpi, etc.)
- Outputs: `PinocchioProgram` IR

### 4. Emitter (`src/emitter/mod.rs`)
- Generates Pinocchio Rust code from IR
- Emits modular structure:
  - `src/lib.rs` - Program entrypoint and instruction dispatcher
  - `src/state.rs` - State structs with `#[repr(C)]`
  - `src/error.rs` - Custom error types
  - `src/helpers.rs` - Constants and helper functions
  - `src/instructions/*.rs` - Individual instruction handlers
  - `Cargo.toml` - Pinocchio dependency configuration
  - `security.json` - Program metadata
- Uses `prettyplease` for code formatting

### Intermediate Representation (`src/ir.rs`)

The IR module defines all data structures used across phases:

**Anchor IR**: `AnchorProgram`, `AnchorInstruction`, `AnchorAccountStruct`, `AccountConstraint`
**Analysis**: `ProgramAnalysis`, `PdaInfo`, `CpiCall`, `AccountSize`
**Pinocchio IR**: `PinocchioProgram`, `PinocchioInstruction`, `Validation`, `PinocchioState`

All IR types are serializable via serde for debugging and intermediate output.

### Supporting Modules

- `src/cpi_helpers.rs` - CPI call detection and transformation helpers
- `src/idl.rs` - IDL generation and verification against original Anchor IDL

## Key Transformations

### Account Constraints → Validations

| Anchor Constraint | Pinocchio Validation |
|-------------------|---------------------|
| `#[account(mut)]` | `Validation::IsWritable` |
| `#[account(signer)]` | `Validation::IsSigner` |
| `#[account(seeds = [...], bump)]` | `Validation::PdaCheck` with seed expressions |
| `#[account(init, payer, space)]` | CPI to `create_account()` |
| `#[account(constraint = expr @ Error)]` | `Validation::Custom` with manual check |

### Discriminators

- Default: 1-byte discriminator (index-based)
- With `--anchor-compat`: 8-byte SHA256 discriminator matching Anchor IDL
- Generated in transformer, used in both dispatcher and IDL

### State Structs

Anchor `#[account]` structs become Pinocchio `#[repr(C)]` structs with:
- Explicit size calculation (`SIZE` constant)
- Manual deserialization (`from_account_info()` method)
- Field offsets calculated from sizes

## Development Patterns

### Adding New Constraint Support

1. Add constraint variant to `AccountConstraint` enum in `src/ir.rs`
2. Parse it in `src/parser/mod.rs` (look for `parse_account_constraint()`)
3. Analyze it in `src/analyzer/mod.rs` if needed
4. Transform to `Validation` in `src/transformer/mod.rs`
5. Emit validation code in `src/emitter/mod.rs`

### Adding CLI Flags

1. Add field to `Args` struct in `src/main.rs`
2. Add to `Config` struct in `src/transformer/mod.rs`
3. Use in transformation logic
4. Update `README.md` examples

### Testing

Integration tests use tempfile to verify:
- Transpiler runs without errors
- Output directory structure is correct
- Generated code compiles (requires Solana toolchain)

Tests reference external test files (see `get_test_input()` in tests), so some may be skipped if inputs aren't available.

## Code Style

- Use descriptive error messages with context (anyhow)
- Preserve code structure where possible during transformation
- Generate human-readable output with proper formatting (prettyplease)
- Keep IR types serializable for debugging
- Verbose mode should show phase-by-phase progress

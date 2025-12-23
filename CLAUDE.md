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

# Run a specific test by name pattern
cargo test discriminator

# Check code with clippy
cargo clippy

# Fix clippy warnings automatically
cargo clippy --fix

# Format code
cargo fmt

# Check formatting without changing files
cargo fmt -- --check

# Install locally
cargo install --path .

# Run the transpiler
cargo run -- input.rs -o output/

# Run with verbose output
cargo run -- input.rs -o output/ --verbose

# Build Pinocchio output with Solana toolchain
cd output/
cargo build-sbf
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
- Two parsing functions:
  - `parse_anchor_file()` → `AnchorProgram` IR (main program structure)
  - `parse_extras()` → `SourceExtras` (constants, helper functions to preserve)
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
- `src/collections.rs` - Vec/VecDeque transformation logic (v0.4.0)
- `src/zero_copy.rs` - AccountLoader/zero-copy transformation (v0.4.0)

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

## Critical Transformation Patterns

### Pinocchio API Differences from Anchor

**CPI Calls:**
- Pinocchio CPI structs take `AccountInfo` directly, NOT `.key()`
- `.invoke()` takes NO parameters (accounts are in struct fields)
- `.invoke_signed()` takes ONLY signer seeds, no account arrays

```rust
// ❌ Wrong (Anchor-style)
Transfer {
    from: user.key(),
    to: vault.key(),
    authority: user.key(),
}.invoke(&[user, vault, authority])?;

// ✅ Correct (Pinocchio)
Transfer {
    from: user,
    to: vault,
    authority: user,
}.invoke()?;
```

**Dereferencing:**
- `try_borrow_mut_lamports()` returns `RefMut<&mut u64>` - use single `*`
- `.key()` returns `&[u8; 32]` - dereference for comparisons with `[u8; 32]`
- State fields are values, not references - no dereference needed
- Option literals (`None`, `Some`) are NOT references

**Field Access:**
- Token account fields use helpers: `get_token_mint()`, `get_token_balance()`, `get_token_owner()`
- State account fields require deserialization first: `account.field` → `account_state.field`
- AccountInfo methods stay on AccountInfo: `account.key()`, `account.is_writable()`

**Spaced Syntax:**
- Generated code uses `account . field ()` not `account.field()`
- All transformations must handle both spaced and non-spaced patterns

**Unsafe Operations:**
- `AccountInfo::assign()` is unsafe - wrap in `unsafe {}` blocks

### Transformation Order Matters

Apply transformations in this order to avoid conflicts:
1. Field access (token/state)
2. State access transformation
3. Comparisons and dereferencing
4. CPI patterns
5. Final cleanup

## Code Style

- Use descriptive error messages with context (anyhow)
- Preserve code structure where possible during transformation
- Generate human-readable output with proper formatting (prettyplease)
- Keep IR types serializable for debugging
- Verbose mode should show phase-by-phase progress
- Optimize for performance: minimize string allocations in hot paths
- Run `cargo fmt` before committing
- Ensure `cargo clippy` passes without warnings

## v0.4.0 Advanced Features (In Development)

Four major features are being developed to increase transpilation success rate from 80% to 95%+:

### 1. Vec<T> Support (`src/collections.rs`, `docs/VEC_SUPPORT_DESIGN.md`)
**Status**: Detection implemented, transformation pending

- Transform `Vec<T>` to fixed-size arrays with length tracking
- Detects `Vec<T>` types and `#[max_len(N)]` attributes in parser
- Will transform to `[T; N]` array + `_len: u8` field
- Enables multisig programs and dynamic lists

**IR Extensions**:
- `VecField` struct tracks element type and max length
- `StateField.is_vec` and `StateField.vec_info` populated by parser

**Implementation**:
- `is_vec_type()` - Detects Vec<T> in AST (✅ Working)
- `extract_max_len_for_vec()` - Parses #[max_len(N)] (✅ Working)
- `transform_vec_operations()` - Transforms push/iter/len (⏳ TODO)
- `generate_vec_helpers()` - Helper methods (⏳ TODO)

### 2. AccountLoader Equivalent (`src/zero_copy.rs`, `docs/ACCOUNT_LOADER_DESIGN.md`)
**Status**: Design complete, implementation pending

- Zero-copy deserialization for large state accounts (10KB+)
- Detects `#[account(zero_copy(unsafe))]` and `#[repr(C, packed)]`
- Generates `unsafe fn load()` and `unsafe fn load_mut()` methods
- Critical for Raydium CLMM PoolState pattern

**IR Extensions**:
- `AnchorStateStruct.is_zero_copy`, `.is_packed`, `.is_unsafe` flags

### 3. VecDeque Transformation (`docs/VECDEQUE_DESIGN.md`)
**Status**: Design complete, implementation pending

- Transform `VecDeque<T>` to circular arrays with head/tail pointers
- Enables multi-tick crossing in CLMM swap operations
- Uses modulo arithmetic for wraparound

### 4. Advanced CPI Patterns (`docs/ADVANCED_CPI_DESIGN.md`)
**Status**: Design complete, implementation pending

- Support for `ctx.remaining_accounts`
- Dynamic account selection patterns
- `Interface<'info, T>` runtime dispatch

**Priority**: AccountLoader > VecDeque > Vec > Advanced CPI (for full CLMM support)

## Release Process

For creating releases and publishing to crates.io, see `.github/RELEASE.md`.

Quick summary:
1. Update version in `Cargo.toml`
2. Commit changes
3. Create and push a git tag (e.g., `v0.1.1`)
4. CI automatically builds binaries and publishes to crates.io

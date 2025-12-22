//! Emit Pinocchio code from IR

use anyhow::Result;
use std::fs;
use std::path::Path;

use crate::ir::*;
use crate::parser::SourceExtras;

pub fn emit_with_extras(
    program: &PinocchioProgram,
    output_dir: &Path,
    extras: Option<&SourceExtras>,
) -> Result<()> {
    fs::create_dir_all(output_dir)?;

    // Emit Cargo.toml
    emit_cargo_toml(program, output_dir)?;

    // Emit src/lib.rs
    let src_dir = output_dir.join("src");
    fs::create_dir_all(&src_dir)?;
    emit_lib_rs(program, &src_dir, extras.is_some())?;

    // Emit src/state.rs
    emit_state_rs(program, &src_dir)?;

    // Emit src/error.rs
    emit_error_rs(program, &src_dir)?;

    // Emit src/helpers.rs (if we have extras)
    if let Some(extras) = extras {
        emit_helpers_rs(extras, &src_dir)?;
    }

    // Emit src/instructions/
    emit_instructions(program, &src_dir)?;

    // Emit security.json for program metadata
    emit_security_json(program, output_dir)?;

    Ok(())
}

fn emit_security_json(program: &PinocchioProgram, output_dir: &Path) -> Result<()> {
    let security = serde_json::json!({
        "name": program.name,
        "project_url": "",
        "contacts": [],
        "policy": "",
        "preferred_languages": ["en"],
        "source_code": "",
        "source_revision": "",
        "source_release": "",
        "encryption": "",
        "auditors": [],
        "acknowledgements": "",
        "expiry": ""
    });

    let content = serde_json::to_string_pretty(&security)?;
    fs::write(output_dir.join("security.json"), content)?;
    Ok(())
}

fn emit_helpers_rs(extras: &SourceExtras, src_dir: &Path) -> Result<()> {
    let mut content = String::new();

    content.push_str("//! Constants and helper functions extracted from original source\n\n");

    // Emit constants
    if !extras.constants.is_empty() {
        content.push_str("// Constants\n");
        for c in &extras.constants {
            content.push_str(&format!("pub const {}: {} = {};\n", c.name, c.ty, c.value));
        }
        content.push('\n');
    }

    // Emit helper functions
    if !extras.helper_functions.is_empty() {
        content.push_str("// Helper functions\n");
        content.push_str("use crate::state::*;\n");
        content.push_str("use crate::error::Error;\n");
        content.push_str("use pinocchio::program_error::ProgramError;\n");
        content.push_str("use pinocchio::sysvars::{clock::Clock, Sysvar};\n");
        content.push_str("use pinocchio::account_info::AccountInfo;\n\n");

        // Add token account helpers
        content.push_str("use pinocchio::pubkey::Pubkey;\n\n");

        content.push_str("/// Get token account balance from account info\n");
        content.push_str("#[inline(always)]\n");
        content.push_str(
            "pub fn get_token_balance(account: &AccountInfo) -> Result<u64, ProgramError> {\n",
        );
        content.push_str("    let data = account.try_borrow_data()?;\n");
        content.push_str("    if data.len() < 72 {\n");
        content.push_str("        return Err(ProgramError::InvalidAccountData);\n");
        content.push_str("    }\n");
        content.push_str(
            "    // Token account amount is at offset 64 (after mint and owner pubkeys)\n",
        );
        content.push_str("    Ok(u64::from_le_bytes(data[64..72].try_into().unwrap()))\n");
        content.push_str("}\n\n");

        content.push_str("/// Get token account mint from account info\n");
        content.push_str("#[inline(always)]\n");
        content.push_str(
            "pub fn get_token_mint(account: &AccountInfo) -> Result<Pubkey, ProgramError> {\n",
        );
        content.push_str("    let data = account.try_borrow_data()?;\n");
        content.push_str("    if data.len() < 32 {\n");
        content.push_str("        return Err(ProgramError::InvalidAccountData);\n");
        content.push_str("    }\n");
        content.push_str("    // Token account mint is at offset 0\n");
        content.push_str("    let bytes: [u8; 32] = data[0..32].try_into().unwrap();\n");
        content.push_str("    Ok(Pubkey::from(bytes))\n");
        content.push_str("}\n\n");

        content.push_str("/// Get token account owner from account info\n");
        content.push_str("#[inline(always)]\n");
        content.push_str(
            "pub fn get_token_owner(account: &AccountInfo) -> Result<Pubkey, ProgramError> {\n",
        );
        content.push_str("    let data = account.try_borrow_data()?;\n");
        content.push_str("    if data.len() < 64 {\n");
        content.push_str("        return Err(ProgramError::InvalidAccountData);\n");
        content.push_str("    }\n");
        content.push_str("    // Token account owner is at offset 32\n");
        content.push_str("    let bytes: [u8; 32] = data[32..64].try_into().unwrap();\n");
        content.push_str("    Ok(Pubkey::from(bytes))\n");
        content.push_str("}\n\n");

        // Add compute_hash helper (SHA256)
        content.push_str("/// Hash result type\n");
        content.push_str("pub struct Hash {\n");
        content.push_str("    bytes: [u8; 32],\n");
        content.push_str("}\n\n");
        content.push_str("impl Hash {\n");
        content.push_str("    pub fn to_bytes(&self) -> [u8; 32] {\n");
        content.push_str("        self.bytes\n");
        content.push_str("    }\n");
        content.push_str("}\n\n");
        content.push_str("/// Compute SHA256 hash of data using solana syscall\n");
        content.push_str("#[inline(always)]\n");
        content.push_str("pub fn compute_hash(data: &[u8]) -> Hash {\n");
        content.push_str("    use pinocchio::syscalls::sol_sha256;\n");
        content.push_str("    let mut hash_result = [0u8; 32];\n");
        content.push_str("    unsafe {\n");
        content.push_str(
            "        sol_sha256(data.as_ptr(), data.len() as u64, hash_result.as_mut_ptr());\n",
        );
        content.push_str("    }\n");
        content.push_str("    Hash { bytes: hash_result }\n");
        content.push_str("}\n\n");

        for f in &extras.helper_functions {
            // Clean up the signature and body
            let mut sig = clean_helper_signature(&f.signature);
            // Make sure it's public
            if !sig.starts_with("pub ") {
                sig = format!("pub {}", sig);
            }
            let mut body = clean_helper_body(&f.body);
            // Apply unsafe math optimization for smaller binary
            body = apply_unsafe_math_to_helpers(&body);
            content.push_str(&format!("{} {}\n\n", sig, body));
        }
    }

    fs::write(src_dir.join("helpers.rs"), content)?;
    Ok(())
}

fn clean_helper_signature(sig: &str) -> String {
    let mut result = sig.to_string();
    // Fix Result types
    result = result.replace("Result < () >", "Result<(), ProgramError>");
    result = result.replace("Result<()>", "Result<(), ProgramError>");
    // Handle Result<T> -> Result<T, ProgramError>
    // Pattern: Result < u64 > -> Result<u64, ProgramError>
    result = result.replace("Result < u64 >", "Result<u64, ProgramError>");
    result = result.replace("Result<u64>", "Result<u64, ProgramError>");
    result = result.replace("Result < u128 >", "Result<u128, ProgramError>");
    result = result.replace("Result<u128>", "Result<u128, ProgramError>");
    // Fix type references
    result = result.replace("& StablePool", "&StablePool");
    result = result.replace("& mut ", "&mut ");
    result
}

fn clean_helper_body(body: &str) -> String {
    let mut result = body.to_string();
    result = result.replace("StableSwapError :: ", "Error::");
    result = result.replace("StableSwapError::", "Error::");
    // Fix spacing in Clock::get()
    result = result.replace("Clock :: get () ?", "Clock::get()?");
    result = result.replace("Clock :: get ()", "Clock::get()");
    // Fix other common spacing issues
    result = result.replace(" :: ", "::");
    result = result.replace(" . ", ".");
    // Replace std:: with core:: for no_std compatibility
    result = result.replace("std::cmp::", "core::cmp::");
    result = result.replace("std::mem::", "core::mem::");
    result = result.replace("std::ptr::", "core::ptr::");
    result
}

/// Apply unsafe math transformations to helper body for smaller binary
/// NOTE: This is a no-op for now because the transformations are complex
/// and break the code. The checked_* -> wrapping_* conversion needs more work.
fn apply_unsafe_math_to_helpers(body: &str) -> String {
    // For now, just return the body as-is
    // The unsafe_math optimization in transformer handles instruction bodies
    // Helper functions have complex chained operations that need special handling
    body.to_string()
}

/// Fix multi-line msg! macros and handle format arguments
/// In no_std pinocchio, msg! only supports simple strings, not format args
fn fix_msg_macros(body: &str) -> String {
    let mut result = String::new();
    let mut i = 0;
    let chars: Vec<char> = body.chars().collect();
    let len = chars.len();

    while i < len {
        // Look for msg! or msg ! pattern
        let remaining = &body[i..];
        let msg_start = if remaining.starts_with("msg!(") {
            Some(5)
        } else if remaining.starts_with("msg ! (") {
            Some(7)
        } else {
            None
        };

        if let Some(offset) = msg_start {
            i += offset;

            // Collect the entire msg! content until matching )
            let mut msg_content = String::new();
            let mut depth = 1;
            while i < len && depth > 0 {
                let c = chars[i];
                match c {
                    '(' => {
                        depth += 1;
                        msg_content.push(c);
                    }
                    ')' => {
                        depth -= 1;
                        if depth > 0 {
                            msg_content.push(c);
                        }
                    }
                    '\n' => {
                        // Replace newlines with spaces
                        if !msg_content.ends_with(' ') {
                            msg_content.push(' ');
                        }
                    }
                    _ => {
                        // Skip leading whitespace after newlines
                        if c.is_whitespace() && msg_content.ends_with(' ') {
                            // Skip
                        } else {
                            msg_content.push(c);
                        }
                    }
                }
                i += 1;
            }

            // Check if the msg! has format arguments (contains a comma outside of string literals)
            let has_format_args = has_comma_outside_strings(&msg_content);

            if has_format_args {
                // Comment out msg! with format args - pinocchio no_std doesn't support them
                result.push_str("// msg!(");
                result.push_str(&msg_content);
                result.push(')');
            } else {
                result.push_str("msg!(");
                result.push_str(&msg_content);
                result.push(')');
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

/// Check if a string has a comma outside of string literals
fn has_comma_outside_strings(s: &str) -> bool {
    let mut in_string = false;
    let mut escape = false;

    for c in s.chars() {
        if escape {
            escape = false;
            continue;
        }
        match c {
            '\\' => escape = true,
            '"' => in_string = !in_string,
            ',' if !in_string => return true,
            _ => {}
        }
    }
    false
}

fn emit_cargo_toml(program: &PinocchioProgram, output_dir: &Path) -> Result<()> {
    let content = format!(
        r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "lib"]

[features]
no-entrypoint = []
cpi = ["no-entrypoint"]

[dependencies]
pinocchio = "0.8"
{}

[profile.release]
overflow-checks = false
lto = "fat"
codegen-units = 1
opt-level = "z"
strip = true
panic = "abort"
debug = false
debug-assertions = false
incremental = false
"#,
        program.name,
        if program.config.no_alloc {
            ""
        } else {
            "pinocchio-token = \"0.3\""
        }
    );

    fs::write(output_dir.join("Cargo.toml"), content)?;
    Ok(())
}

fn emit_lib_rs(program: &PinocchioProgram, src_dir: &Path, has_helpers: bool) -> Result<()> {
    let mut content = String::new();

    // Use no_std for smallest binary size
    content.push_str("#![no_std]\n");
    content.push_str("#![allow(unexpected_cfgs)]\n\n");

    content.push_str("use pinocchio::{\n");
    content.push_str("    account_info::AccountInfo,\n");
    content.push_str("    program_error::ProgramError,\n");
    content.push_str("    pubkey::Pubkey,\n");
    content.push_str("    ProgramResult,\n");
    content.push_str("};\n\n");

    // Modules
    content.push_str("mod state;\n");
    content.push_str("mod error;\n");
    if has_helpers {
        content.push_str("mod helpers;\n");
    }
    content.push_str("mod instructions;\n\n");

    content.push_str("pub use state::*;\n");
    content.push_str("pub use error::*;\n");
    if has_helpers {
        content.push_str("pub use helpers::*;\n");
    }
    content.push('\n');

    // Program ID as bytes (Pinocchio uses [u8; 32])
    if let Some(id) = &program.program_id {
        content.push_str(&format!("/// Program ID: {}\n", id));
        content.push_str("pub const ID: [u8; 32] = [\n");
        // Decode base58 to bytes
        if let Ok(bytes) = bs58_decode(id) {
            for chunk in bytes.chunks(8) {
                content.push_str("    ");
                for b in chunk {
                    content.push_str(&format!("{:#04x}, ", b));
                }
                content.push('\n');
            }
        } else {
            content.push_str("    0; 32 // TODO: Decode program ID\n");
        }
        content.push_str("];\n\n");
    }

    // Entrypoint
    content.push_str("#[cfg(not(feature = \"no-entrypoint\"))]\n");
    content.push_str("use pinocchio::entrypoint;\n");
    content.push_str("#[cfg(not(feature = \"no-entrypoint\"))]\n");
    content.push_str("entrypoint!(process_instruction);\n\n");

    // Note: Pinocchio provides its own panic handler, so we don't define one here

    // Discriminator constants
    content.push_str("// Instruction discriminators (Anchor-compatible)\n");
    for inst in &program.instructions {
        let disc_bytes: Vec<String> = inst
            .discriminator
            .iter()
            .map(|b| format!("{:#04x}", b))
            .collect();
        content.push_str(&format!(
            "const {}_DISC: [u8; 8] = [{}];\n",
            to_screaming_snake_str(&inst.name),
            disc_bytes.join(", ")
        ));
    }
    content.push('\n');

    // Main dispatch function
    content.push_str("pub fn process_instruction(\n");
    content.push_str("    program_id: &Pubkey,\n");
    content.push_str("    accounts: &[AccountInfo],\n");
    content.push_str("    instruction_data: &[u8],\n");
    content.push_str(") -> ProgramResult {\n");
    content.push_str("    if instruction_data.len() < 8 {\n");
    content.push_str("        return Err(ProgramError::InvalidInstructionData);\n");
    content.push_str("    }\n\n");

    content.push_str("    let (disc, data) = instruction_data.split_at(8);\n");
    content.push_str("    let disc: [u8; 8] = disc.try_into().unwrap();\n\n");

    content.push_str("    match disc {\n");

    for inst in &program.instructions {
        content.push_str(&format!(
            "        {}_DISC => instructions::{}(program_id, accounts, data),\n",
            to_screaming_snake_str(&inst.name),
            inst.name
        ));
    }

    content.push_str("        _ => Err(ProgramError::InvalidInstructionData),\n");
    content.push_str("    }\n");
    content.push_str("}\n");

    fs::write(src_dir.join("lib.rs"), content)?;
    Ok(())
}

fn to_screaming_snake_str(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(c.to_uppercase().next().unwrap());
    }
    result
}

fn bs58_decode(s: &str) -> Result<Vec<u8>> {
    // Simple base58 decode for Solana addresses
    const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

    let mut result: Vec<u8> = vec![0; 32];
    let mut scratch: Vec<u64> = vec![0; 44]; // Enough for 32 bytes

    for c in s.bytes() {
        let mut val = ALPHABET
            .iter()
            .position(|&x| x == c)
            .ok_or_else(|| anyhow::anyhow!("Invalid base58 character"))?
            as u64;

        for digit in scratch.iter_mut() {
            val += *digit * 58;
            *digit = val & 0xFF;
            val >>= 8;
        }
    }

    // Convert scratch to result
    for (i, &b) in scratch.iter().take(32).enumerate() {
        result[31 - i] = b as u8;
    }

    // Handle leading zeros
    let leading_zeros = s.bytes().take_while(|&c| c == b'1').count();
    for item in result.iter_mut().take(leading_zeros) {
        *item = 0;
    }

    Ok(result)
}

fn emit_state_rs(program: &PinocchioProgram, src_dir: &Path) -> Result<()> {
    let mut content = String::new();

    content
        .push_str("use pinocchio::{account_info::AccountInfo, program_error::ProgramError};\n\n");

    for state in &program.state_structs {
        // Struct definition
        content.push_str("#[repr(C)]\n");
        content.push_str("#[derive(Clone, Copy)]\n");
        content.push_str(&format!("pub struct {} {{\n", state.name));

        for field in &state.fields {
            content.push_str(&format!("    pub {}: {},\n", field.name, field.ty));
        }

        content.push_str("}\n\n");

        // Impl block
        content.push_str(&format!("impl {} {{\n", state.name));
        content.push_str(&format!("    pub const SIZE: usize = {};\n\n", state.size));

        // from_account_info
        content.push_str("    #[inline(always)]\n");
        content.push_str(
            "    pub fn from_account_info(info: &AccountInfo) -> Result<&Self, ProgramError> {\n",
        );
        content.push_str("        let data = info.try_borrow_data()?;\n");
        content.push_str("        if data.len() < 8 + Self::SIZE {\n");
        content.push_str("            return Err(ProgramError::InvalidAccountData);\n");
        content.push_str("        }\n");
        content.push_str("        // Skip 8-byte discriminator\n");
        content.push_str("        Ok(unsafe { &*(data[8..].as_ptr() as *const Self) })\n");
        content.push_str("    }\n\n");

        // from_account_info_mut
        content.push_str("    #[inline(always)]\n");
        content.push_str("    pub fn from_account_info_mut(info: &AccountInfo) -> Result<&mut Self, ProgramError> {\n");
        content.push_str("        let mut data = info.try_borrow_mut_data()?;\n");
        content.push_str("        if data.len() < 8 + Self::SIZE {\n");
        content.push_str("            return Err(ProgramError::InvalidAccountData);\n");
        content.push_str("        }\n");
        content.push_str("        Ok(unsafe { &mut *(data[8..].as_mut_ptr() as *mut Self) })\n");
        content.push_str("    }\n");

        content.push_str("}\n\n");
    }

    fs::write(src_dir.join("state.rs"), content)?;
    Ok(())
}

fn emit_error_rs(program: &PinocchioProgram, src_dir: &Path) -> Result<()> {
    let mut content = String::new();

    content.push_str("use pinocchio::program_error::ProgramError;\n\n");

    content.push_str("#[repr(u32)]\n");
    content.push_str("#[derive(Clone, Copy, Debug)]\n");
    content.push_str("pub enum Error {\n");

    for error in &program.errors {
        content.push_str(&format!("    /// {}\n", error.msg));
        content.push_str(&format!("    {} = {},\n", error.name, error.code));
    }

    content.push_str("}\n\n");

    // Impl From<Error> for ProgramError
    content.push_str("impl From<Error> for ProgramError {\n");
    content.push_str("    fn from(e: Error) -> Self {\n");
    content.push_str("        ProgramError::Custom(e as u32)\n");
    content.push_str("    }\n");
    content.push_str("}\n");

    fs::write(src_dir.join("error.rs"), content)?;
    Ok(())
}

fn emit_instructions(program: &PinocchioProgram, src_dir: &Path) -> Result<()> {
    let inst_dir = src_dir.join("instructions");
    fs::create_dir_all(&inst_dir)?;

    // mod.rs
    let mut mod_content = String::new();
    for inst in &program.instructions {
        mod_content.push_str(&format!("mod {};\n", inst.name));
    }
    mod_content.push('\n');
    for inst in &program.instructions {
        mod_content.push_str(&format!("pub use {}::{};\n", inst.name, inst.name));
    }

    fs::write(inst_dir.join("mod.rs"), mod_content)?;

    // Individual instruction files
    for inst in &program.instructions {
        emit_instruction(inst, program, &inst_dir)?;
    }

    Ok(())
}

fn emit_instruction(
    inst: &PinocchioInstruction,
    program: &PinocchioProgram,
    inst_dir: &Path,
) -> Result<()> {
    let mut content = String::new();

    content.push_str("#![allow(unused_variables, unused_imports)]\n\n");
    content.push_str("use pinocchio::{\n");
    content.push_str("    account_info::AccountInfo,\n");
    content.push_str("    msg,\n");
    content.push_str("    program_error::ProgramError,\n");
    content.push_str("    pubkey::Pubkey,\n");
    content.push_str("    ProgramResult,\n");
    content.push_str("    sysvars::{clock::Clock, Sysvar},\n");
    content.push_str("};\n");

    // Add pinocchio_token if the instruction uses token operations
    let needs_token_imports = inst.body.contains("token::")
        || inst.body.contains("Transfer")
        || inst.body.contains("mint_to")
        || inst.body.contains("burn")
        || inst
            .accounts
            .iter()
            .any(|acc| acc.is_init && acc.token_mint.is_some());

    if needs_token_imports {
        let mut imports = vec!["Transfer", "MintTo", "Burn"];

        // Add InitializeAccount2 if we're initializing token accounts
        if inst
            .accounts
            .iter()
            .any(|acc| acc.is_init && acc.token_mint.is_some())
        {
            imports.push("InitializeAccount2");
        }

        content.push_str(&format!(
            "use pinocchio_token::instructions::{{{}}};\n",
            imports.join(", ")
        ));
    }
    content.push('\n');

    content.push_str("use crate::error::Error;\n");
    content.push_str("use crate::helpers::*;\n");

    // Import state structs if referenced in body or validations
    let mut imported_states = std::collections::HashSet::new();
    for state in &program.state_structs {
        // Check if referenced in instruction body
        if inst.body.contains(&state.name) {
            imported_states.insert(state.name.clone());
        }

        // Check if referenced in validations (for early deserialization)
        for validation in &inst.validations {
            let validation_str = match validation {
                Validation::PdaCheck { seeds, .. } => seeds.join(" "),
                Validation::Custom { code } => code.clone(),
                _ => String::new(),
            };

            // Check if any account fields are referenced that would require this state type
            for acc in &inst.accounts {
                if state.name.to_lowercase().contains(&acc.name.to_lowercase())
                    && validation_str.contains(&format!("{} . ", acc.name))
                {
                    imported_states.insert(state.name.clone());
                }
            }
        }
    }

    for state_name in &imported_states {
        content.push_str(&format!("use crate::state::{};\n", state_name));
    }
    content.push('\n');

    // Check if we need Rent sysvar for token account initialization
    let needs_rent_sysvar = inst
        .accounts
        .iter()
        .any(|acc| acc.is_init && acc.token_mint.is_some());

    let rent_sysvar_index = if needs_rent_sysvar {
        inst.accounts.len()
    } else {
        0
    };

    // Account indices as constants for clarity
    if !inst.accounts.is_empty() {
        content.push_str("// Account indices\n");
        for acc in &inst.accounts {
            content.push_str(&format!(
                "const {}: usize = {};\n",
                to_screaming_snake(&acc.name),
                acc.index
            ));
        }
        if needs_rent_sysvar {
            content.push_str(&format!(
                "const RENT_SYSVAR: usize = {};\n",
                rent_sysvar_index
            ));
        }
        content.push('\n');
    }

    // Function signature
    content.push_str(&format!(
        "pub fn {}(\n    program_id: &Pubkey,\n    accounts: &[AccountInfo],\n    data: &[u8],\n) -> ProgramResult {{\n",
        inst.name
    ));

    if inst.accounts.is_empty() {
        content.push_str("    // No accounts required\n");
        content.push_str("    Ok(())\n");
        content.push_str("}\n");
        fs::write(inst_dir.join(format!("{}.rs", inst.name)), content)?;
        return Ok(());
    }

    // Account validation
    let min_accounts = if needs_rent_sysvar {
        inst.accounts.len() + 1
    } else {
        inst.accounts.len()
    };

    content.push_str(&format!(
        "    // Validate account count\n    if accounts.len() < {} {{\n        return Err(ProgramError::NotEnoughAccountKeys);\n    }}\n\n",
        min_accounts
    ));

    // Get account references with better naming
    content.push_str("    // Get accounts\n");
    for acc in &inst.accounts {
        content.push_str(&format!(
            "    let {} = &accounts[{}];\n",
            acc.name,
            to_screaming_snake(&acc.name)
        ));
    }
    if needs_rent_sysvar {
        content.push_str("    let rent_sysvar = &accounts[RENT_SYSVAR];\n");
    }
    content.push('\n');

    // Detect which instruction args are used in PDA seeds and parse them early
    let mut args_used_in_pda: Vec<String> = Vec::new();
    for validation in &inst.validations {
        if let Validation::PdaCheck { seeds, .. } = validation {
            for seed in seeds {
                // Check if any instruction arg names appear in the seed
                for arg in &inst.args {
                    if seed.contains(&arg.name) && !args_used_in_pda.contains(&arg.name) {
                        args_used_in_pda.push(arg.name.clone());
                    }
                }
            }
        }
    }

    // Parse args needed for PDA seeds BEFORE account validation
    if !args_used_in_pda.is_empty() && !inst.args.is_empty() {
        content.push_str("    // Parse instruction arguments needed for PDA verification\n");
        let mut offset = 0usize;
        for arg in &inst.args {
            let (size, parse_code) = get_arg_parse_code(&arg.ty, offset, &arg.name);
            if args_used_in_pda.contains(&arg.name) {
                content.push_str(&format!("    {}\n", parse_code));
            }
            offset += size;
        }
        content.push('\n');
    }

    // Deserialize state accounts early if their fields are referenced in validations
    let mut state_accounts_to_deserialize = Vec::new();
    for validation in &inst.validations {
        let validation_str = match validation {
            Validation::PdaCheck { seeds, .. } => seeds.join(" "),
            Validation::Custom { code } => code.clone(),
            _ => String::new(),
        };

        // Check if any account's fields are referenced (e.g., "pool.bump", "pool.authority")
        for acc in &inst.accounts {
            // Check if this account has a state type
            // Match if account name is contained in state name (e.g., "pool" in "StablePool")
            let has_state_type = program
                .state_structs
                .iter()
                .any(|s| s.name.to_lowercase().contains(&acc.name.to_lowercase()));
            if has_state_type
                && validation_str.contains(&format!("{} . ", acc.name))
                && !state_accounts_to_deserialize.contains(&acc.name)
            {
                state_accounts_to_deserialize.push(acc.name.clone());
            }
        }
    }

    if !state_accounts_to_deserialize.is_empty() {
        content.push_str("    // Deserialize state accounts needed for validation\n");
        for acc_name in &state_accounts_to_deserialize {
            // Find the matching state struct (account name should be contained in state name)
            if let Some(state) = program
                .state_structs
                .iter()
                .find(|s| s.name.to_lowercase().contains(&acc_name.to_lowercase()))
            {
                content.push_str(&format!(
                    "    let {}_state = {}::from_account_info({})?;\n",
                    acc_name, state.name, acc_name
                ));
            }
        }
        content.push('\n');
    }

    // Emit validations
    let mut has_validations = false;
    for validation in &inst.validations {
        match validation {
            Validation::IsSigner { account_idx } => {
                if !has_validations {
                    content.push_str("    // Validate accounts\n");
                    has_validations = true;
                }
                let acc = &inst.accounts[*account_idx];
                content.push_str(&format!(
                    "    if !{}.is_signer() {{\n        return Err(ProgramError::MissingRequiredSignature);\n    }}\n",
                    acc.name
                ));
            }
            Validation::IsWritable { account_idx } => {
                if !has_validations {
                    content.push_str("    // Validate accounts\n");
                    has_validations = true;
                }
                let acc = &inst.accounts[*account_idx];
                content.push_str(&format!(
                    "    if !{}.is_writable() {{\n        return Err(ProgramError::Immutable);\n    }}\n",
                    acc.name
                ));
            }
            Validation::PdaCheck {
                account_idx,
                seeds,
                bump,
            } => {
                if !has_validations {
                    content.push_str("    // Validate accounts\n");
                    has_validations = true;
                }
                let acc = &inst.accounts[*account_idx];
                // Generate actual PDA validation code
                let mut seeds_code: Vec<String> = seeds
                    .iter()
                    .map(|s| {
                        let mut seed = s.clone();

                        // Transform state field references: "pool . bump" -> "pool_state.bump"
                        for state_acc in &state_accounts_to_deserialize {
                            let pattern = format!("{} . ", state_acc);
                            if seed.contains(&pattern) {
                                seed = seed.replace(&pattern, &format!("{}_state.", state_acc));
                            }
                        }

                        if seed.starts_with("b\"") {
                            format!("{}.as_ref()", seed)
                        } else if seed.contains(".key()") {
                            let acc_name = seed
                                .replace(".key()", "")
                                .replace(".as_ref()", "")
                                .replace(" ", "");
                            format!("{}.key().as_ref()", acc_name)
                        } else if seed.contains("as_ref") {
                            seed
                        } else {
                            format!("{}.as_ref()", seed)
                        }
                    })
                    .collect();

                // If bump is explicitly provided, add it to seeds (with state field transformation)
                if let Some(bump_var) = bump {
                    let mut transformed_bump = bump_var.clone();
                    // Transform state field references in bump
                    for state_acc in &state_accounts_to_deserialize {
                        let pattern = format!("{} . ", state_acc);
                        if transformed_bump.contains(&pattern) {
                            transformed_bump = transformed_bump
                                .replace(&pattern, &format!("{}_state.", state_acc));
                        }
                    }
                    seeds_code.push(format!("&[{}]", transformed_bump));
                }

                // Generate the PDA verification code
                content.push_str(&format!("    // Verify PDA for {}\n", acc.name));

                // Check if this PDA references its own state fields (self-referential)
                // Check in ORIGINAL seeds OR bump for the pattern "accountname . "
                let is_self_referential = seeds
                    .iter()
                    .any(|s| s.contains(&format!("{} . ", acc.name)))
                    || bump
                        .as_ref()
                        .is_some_and(|b| b.contains(&format!("{} . ", acc.name)));

                // If account is being initialized, self-referential, or bump not provided, use find_program_address
                if acc.is_init || bump.is_none() || is_self_referential {
                    // For find_program_address, don't include the bump in seeds (it's what we're finding)
                    // Remove the last seed if it contains a bump reference
                    let mut find_seeds = seeds_code.clone();
                    if let Some(last) = find_seeds.last() {
                        // Remove if it's a bump seed (contains .bump or is a byte array reference)
                        if last.contains(".bump")
                            || (last.starts_with("&[") && !last.contains("b\""))
                        {
                            find_seeds.pop();
                        }
                    }

                    // Find the bump (needed for init, self-reference, or when bump not provided)
                    content.push_str(&format!(
                        "    let (expected_{}, _bump_{}) = pinocchio::pubkey::find_program_address(\n",
                        acc.name, acc.name
                    ));
                    content.push_str(&format!("        &[{}],\n", find_seeds.join(", ")));
                    content.push_str("        program_id,\n");
                    content.push_str("    );\n");
                } else {
                    // If bump is provided from another account's field, use create_program_address
                    content.push_str(&format!(
                        "    let expected_{} = pinocchio::pubkey::create_program_address(\n",
                        acc.name
                    ));
                    content.push_str(&format!("        &[{}],\n", seeds_code.join(", ")));
                    content.push_str("        program_id,\n");
                    content.push_str("    )?;\n");
                }
                content.push_str(&format!(
                    "    if {}.key() != &expected_{} {{\n",
                    acc.name, acc.name
                ));
                content.push_str("        return Err(ProgramError::InvalidSeeds);\n");
                content.push_str("    }\n");
            }
            Validation::Custom { code } => {
                if !has_validations {
                    content.push_str("    // Validate accounts\n");
                    has_validations = true;
                }
                // Transform state field references in custom validation code
                let mut transformed_code = code.clone();
                for state_acc in &state_accounts_to_deserialize {
                    let pattern = format!("{} . ", state_acc);
                    transformed_code =
                        transformed_code.replace(&pattern, &format!("{}_state.", state_acc));
                }

                // Transform token account field access: account.mint -> get_token_mint(account)?
                for acc in &inst.accounts {
                    // Check if this looks like a token account (commonly named with token types)
                    let is_token_account = acc.name.contains("user_")
                        || acc.name.contains("_token")
                        || acc.name == "position"
                        || acc.token_mint.is_some();

                    if is_token_account {
                        transformed_code = transformed_code.replace(
                            &format!("{} . mint", acc.name),
                            &format!("get_token_mint({})?", acc.name),
                        );
                        transformed_code = transformed_code.replace(
                            &format!("{} . owner", acc.name),
                            &format!("get_token_owner({})?", acc.name),
                        );
                    }
                }

                // Fix Pubkey comparisons: add dereference for .key() in comparisons
                // Replace " == X.key ()" patterns
                for acc in &inst.accounts {
                    // Pattern: == account.key () -> == *account.key()
                    transformed_code = transformed_code.replace(
                        &format!(" == {} . key ()", acc.name),
                        &format!(" == *{}.key()", acc.name),
                    );
                    transformed_code = transformed_code.replace(
                        &format!(" != {} . key ()", acc.name),
                        &format!(" != *{}.key()", acc.name),
                    );
                    transformed_code = transformed_code.replace(
                        &format!("{} . key () == ", acc.name),
                        &format!("*{}.key() == ", acc.name),
                    );
                    transformed_code = transformed_code.replace(
                        &format!("{} . key () != ", acc.name),
                        &format!("*{}.key() != ", acc.name),
                    );
                }

                content.push_str(&format!("    {}\n", transformed_code));
            }
            _ => {}
        }
    }

    if has_validations {
        content.push('\n');
    }

    // Parse remaining instruction arguments (skip those already parsed for PDA seeds)
    let remaining_args: Vec<_> = inst
        .args
        .iter()
        .filter(|arg| !args_used_in_pda.contains(&arg.name))
        .collect();

    if !remaining_args.is_empty() {
        content.push_str("    // Parse instruction arguments\n");

        let mut offset = 0usize;
        for arg in &inst.args {
            let (size, parse_code) = get_arg_parse_code(&arg.ty, offset, &arg.name);
            // Only emit if not already parsed for PDA seeds
            if !args_used_in_pda.contains(&arg.name) {
                content.push_str(&format!("    {}\n", parse_code));
            }
            offset += size;
        }
        content.push('\n');
    }

    // Generate token account initialization code if needed
    for acc in &inst.accounts {
        if acc.is_init && acc.token_mint.is_some() && acc.token_authority.is_some() {
            content.push_str(&format!("    // Initialize token account: {}\n", acc.name));
            let mint_name = acc.token_mint.as_ref().unwrap();
            let authority_name = acc.token_authority.as_ref().unwrap();
            let default_payer = "authority".to_string();
            let payer_name = acc.init_payer.as_ref().unwrap_or(&default_payer);

            // Verify rent sysvar address
            content.push_str("    // Verify Rent sysvar\n");
            content.push_str("    const RENT_SYSVAR_ID: [u8; 32] = [\n");
            content.push_str(
                "        6, 167, 213, 23, 24, 199, 116, 201, 40, 86, 99, 152, 105, 29,\n",
            );
            content
                .push_str("        94, 182, 139, 94, 184, 163, 155, 75, 109, 92, 115, 85, 91,\n");
            content.push_str("        33, 0, 0, 0, 0,\n");
            content.push_str("    ];\n");
            content.push_str("    if rent_sysvar.key().to_bytes() != RENT_SYSVAR_ID {\n");
            content.push_str("        return Err(ProgramError::InvalidArgument);\n");
            content.push_str("    }\n\n");

            // Add create_account CPI if this is a PDA (needs to be created)
            if acc.is_pda && acc.pda_seeds.is_some() {
                content.push_str("    // Create PDA account for token account\n");
                content.push_str(
                    "    const TOKEN_ACCOUNT_SIZE: usize = 165; // SPL Token Account size\n",
                );
                content.push_str("    let rent = pinocchio::sysvars::rent::Rent::get()?;\n");
                content.push_str(
                    "    let rent_lamports = rent.minimum_balance(TOKEN_ACCOUNT_SIZE);\n\n",
                );

                content.push_str("    // Transfer lamports from payer to new account\n");
                content.push_str(&format!(
                    "    **{}.try_borrow_mut_lamports()? -= rent_lamports;\n",
                    payer_name
                ));
                content.push_str(&format!(
                    "    **{}.try_borrow_mut_lamports()? += rent_lamports;\n\n",
                    acc.name
                ));

                content.push_str("    // Allocate space and assign owner\n");
                content.push_str(&format!("    {}.assign(&pinocchio_token::ID);\n", acc.name));
                content.push_str(&format!(
                    "    {}.realloc(TOKEN_ACCOUNT_SIZE, false)?;\n\n",
                    acc.name
                ));
            }

            content.push_str(&format!(
                "    pinocchio_token::instructions::InitializeAccount2 {{\n        account: {},\n        mint: {},\n        owner: {},\n        rent_sysvar: rent_sysvar,\n    }}.invoke()?;\n\n",
                acc.name, mint_name, authority_name
            ));
        }
    }

    // Add transformed body or placeholder
    let body_ends_with_ok =
        inst.body.trim().ends_with("Ok (())") || inst.body.trim().ends_with("Ok(())");

    if !inst.body.is_empty() && inst.body != "{}" {
        content.push_str("    // Transformed instruction logic\n");
        // Add the transformed body (will have some TODO markers)
        // First, fix any multi-line msg! macros
        let fixed_body = fix_msg_macros(&inst.body);
        for line in fixed_body.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                // Skip duplicate Ok(()) if body already has it
                if body_ends_with_ok && (trimmed == "Ok (())" || trimmed == "Ok(())") {
                    continue;
                }
                content.push_str(&format!("    {}\n", trimmed));
            }
        }
    } else {
        content.push_str("    // TODO: Implement instruction logic\n");
    }

    // Only add Ok(()) if body doesn't already have it
    if !body_ends_with_ok {
        content.push_str("\n    Ok(())\n");
    } else {
        content.push_str("    Ok(())\n");
    }
    content.push_str("}\n");

    fs::write(inst_dir.join(format!("{}.rs", inst.name)), content)?;
    Ok(())
}

fn to_screaming_snake(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(c.to_uppercase().next().unwrap());
    }
    result
}

/// Returns (size, parse_code) for a given type
fn get_arg_parse_code(ty: &str, offset: usize, name: &str) -> (usize, String) {
    let ty_clean = ty.replace(" ", "").to_lowercase();

    match ty_clean.as_str() {
        "u8" => (1, format!(
            "let {} = data.get({}).copied().ok_or(ProgramError::InvalidInstructionData)?;",
            name, offset
        )),
        "i8" => (1, format!(
            "let {} = data.get({}).map(|&b| b as i8).ok_or(ProgramError::InvalidInstructionData)?;",
            name, offset
        )),
        "u16" => (2, format!(
            "let {} = u16::from_le_bytes(data.get({}..{}).ok_or(ProgramError::InvalidInstructionData)?.try_into().unwrap());",
            name, offset, offset + 2
        )),
        "i16" => (2, format!(
            "let {} = i16::from_le_bytes(data.get({}..{}).ok_or(ProgramError::InvalidInstructionData)?.try_into().unwrap());",
            name, offset, offset + 2
        )),
        "u32" => (4, format!(
            "let {} = u32::from_le_bytes(data.get({}..{}).ok_or(ProgramError::InvalidInstructionData)?.try_into().unwrap());",
            name, offset, offset + 4
        )),
        "i32" => (4, format!(
            "let {} = i32::from_le_bytes(data.get({}..{}).ok_or(ProgramError::InvalidInstructionData)?.try_into().unwrap());",
            name, offset, offset + 4
        )),
        "u64" => (8, format!(
            "let {} = u64::from_le_bytes(data.get({}..{}).ok_or(ProgramError::InvalidInstructionData)?.try_into().unwrap());",
            name, offset, offset + 8
        )),
        "i64" => (8, format!(
            "let {} = i64::from_le_bytes(data.get({}..{}).ok_or(ProgramError::InvalidInstructionData)?.try_into().unwrap());",
            name, offset, offset + 8
        )),
        "u128" => (16, format!(
            "let {} = u128::from_le_bytes(data.get({}..{}).ok_or(ProgramError::InvalidInstructionData)?.try_into().unwrap());",
            name, offset, offset + 16
        )),
        "i128" => (16, format!(
            "let {} = i128::from_le_bytes(data.get({}..{}).ok_or(ProgramError::InvalidInstructionData)?.try_into().unwrap());",
            name, offset, offset + 16
        )),
        "bool" => (1, format!(
            "let {} = data.get({}).copied().ok_or(ProgramError::InvalidInstructionData)? != 0;",
            name, offset
        )),
        "pubkey" => (32, format!(
            "let {}: &[u8; 32] = data.get({}..{}).ok_or(ProgramError::InvalidInstructionData)?.try_into().unwrap();",
            name, offset, offset + 32
        )),
        // Fixed-size byte arrays
        "[u8;32]" => (32, format!(
            "let {}: [u8; 32] = data.get({}..{}).ok_or(ProgramError::InvalidInstructionData)?.try_into().unwrap();",
            name, offset, offset + 32
        )),
        "[u8;64]" => (64, format!(
            "let {}: [u8; 64] = data.get({}..{}).ok_or(ProgramError::InvalidInstructionData)?.try_into().unwrap();",
            name, offset, offset + 64
        )),
        _ => {
            // Check for generic [u8; N] pattern
            if ty_clean.starts_with("[u8;") && ty_clean.ends_with("]") {
                if let Some(n_str) = ty_clean.strip_prefix("[u8;").and_then(|s| s.strip_suffix("]")) {
                    if let Ok(n) = n_str.parse::<usize>() {
                        return (n, format!(
                            "let {}: [u8; {}] = data.get({}..{}).ok_or(ProgramError::InvalidInstructionData)?.try_into().unwrap();",
                            name, n, offset, offset + n
                        ));
                    }
                }
            }
            // Default: assume it's a custom struct or unknown type
            (0, format!("// TODO: Parse {} of type {} at offset {}", name, ty, offset))
        }
    }
}

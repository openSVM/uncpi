//! Analyze Anchor program structure

use anyhow::Result;
use crate::ir::*;

pub fn analyze(program: &AnchorProgram) -> Result<ProgramAnalysis> {
    let pdas = extract_pdas(program);
    let cpi_calls = extract_cpi_calls(program);
    let account_sizes = calculate_sizes(program);

    Ok(ProgramAnalysis {
        pdas,
        cpi_calls,
        account_sizes,
    })
}

fn extract_pdas(program: &AnchorProgram) -> Vec<PdaInfo> {
    let mut pdas = Vec::new();

    for account_struct in &program.account_structs {
        for account in &account_struct.accounts {
            let mut seeds: Option<Vec<String>> = None;
            let mut bump_source: Option<String> = None;

            for constraint in &account.constraints {
                match constraint {
                    AccountConstraint::Seeds(s) => seeds = Some(s.clone()),
                    AccountConstraint::Bump(b) => bump_source = b.clone(),
                    _ => {}
                }
            }

            if let Some(seeds) = seeds {
                pdas.push(PdaInfo {
                    account_name: account.name.clone(),
                    seeds,
                    bump_source,
                    program_id: "program_id".to_string(),
                });
            }
        }
    }

    pdas
}

fn extract_cpi_calls(program: &AnchorProgram) -> Vec<CpiCall> {
    let mut calls = Vec::new();

    for instruction in &program.instructions {
        // Look for common CPI patterns in the body
        let body = &instruction.body;

        // Token transfers
        if body.contains("token::transfer") {
            calls.push(CpiCall {
                target_program: "token_program".to_string(),
                instruction: "transfer".to_string(),
                accounts: vec!["from".to_string(), "to".to_string(), "authority".to_string()],
            });
        }

        if body.contains("token::mint_to") {
            calls.push(CpiCall {
                target_program: "token_program".to_string(),
                instruction: "mint_to".to_string(),
                accounts: vec!["mint".to_string(), "to".to_string(), "authority".to_string()],
            });
        }

        if body.contains("token::burn") {
            calls.push(CpiCall {
                target_program: "token_program".to_string(),
                instruction: "burn".to_string(),
                accounts: vec!["mint".to_string(), "from".to_string(), "authority".to_string()],
            });
        }

        // System program
        if body.contains("system_program::transfer") || body.contains("Transfer") {
            calls.push(CpiCall {
                target_program: "system_program".to_string(),
                instruction: "transfer".to_string(),
                accounts: vec!["from".to_string(), "to".to_string()],
            });
        }
    }

    calls
}

fn calculate_sizes(program: &AnchorProgram) -> Vec<AccountSize> {
    let mut sizes = Vec::new();

    for state in &program.state_structs {
        let mut total_size = 8; // Discriminator
        let mut fields = Vec::new();

        for field in &state.fields {
            let field_size = estimate_field_size(&field.ty);
            fields.push((field.name.clone(), field_size));
            total_size += field_size;
        }

        sizes.push(AccountSize {
            struct_name: state.name.clone(),
            size: total_size,
            fields,
        });
    }

    sizes
}

fn estimate_field_size(ty: &str) -> usize {
    let ty = ty.replace(" ", "").to_lowercase();

    // Handle Option<T>
    if ty.starts_with("option<") {
        let inner = &ty[7..ty.len() - 1];
        return 1 + estimate_field_size(inner); // 1 byte discriminator + inner
    }

    // Handle Vec<T> - can't estimate, use placeholder
    if ty.starts_with("vec<") {
        return 4; // Just the length prefix
    }

    // Handle String
    if ty == "string" {
        return 4; // Length prefix (content is variable)
    }

    match ty.as_str() {
        // Primitive types
        "bool" => 1,
        "u8" | "i8" => 1,
        "u16" | "i16" => 2,
        "u32" | "i32" => 4,
        "u64" | "i64" => 8,
        "u128" | "i128" => 16,
        "f32" => 4,
        "f64" => 8,

        // Solana types
        "pubkey" => 32,
        "publickey" => 32,

        // Unknown - estimate
        _ => 32, // Conservative estimate for unknown types
    }
}

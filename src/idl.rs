//! IDL Generator for Pinocchio programs
//! Generates Anchor-compatible IDL JSON from the transpiled program

use crate::ir::{
    PinocchioError, PinocchioField, PinocchioInstruction, PinocchioProgram, PinocchioState,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Serialize, Deserialize)]
pub struct Idl {
    pub version: String,
    pub name: String,
    pub instructions: Vec<IdlInstruction>,
    pub accounts: Vec<IdlAccount>,
    pub errors: Vec<IdlError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<IdlMetadata>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IdlMetadata {
    pub address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IdlInstruction {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docs: Option<Vec<String>>,
    pub accounts: Vec<IdlAccountItem>,
    pub args: Vec<IdlArg>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discriminator: Option<Vec<u8>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IdlAccountItem {
    pub name: String,
    #[serde(rename = "isMut")]
    pub is_mut: bool,
    #[serde(rename = "isSigner")]
    pub is_signer: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docs: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IdlArg {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: IdlType,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum IdlType {
    Simple(String),
    Array { array: (Box<IdlType>, usize) },
    Option { option: Box<IdlType> },
    Vec { vec: Box<IdlType> },
    Defined { defined: String },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IdlAccount {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: IdlAccountType,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IdlAccountType {
    pub kind: String,
    pub fields: Vec<IdlField>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IdlField {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: IdlType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docs: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IdlError {
    pub code: u32,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msg: Option<String>,
}

/// Generate IDL from a PinocchioProgram
pub fn generate_idl(program: &PinocchioProgram, program_id: Option<&str>) -> Idl {
    let instructions: Vec<IdlInstruction> = program
        .instructions
        .iter()
        .map(instruction_to_idl)
        .collect();

    let accounts: Vec<IdlAccount> = program
        .state_structs
        .iter()
        .map(state_to_idl_account)
        .collect();

    let errors: Vec<IdlError> = program
        .errors
        .iter()
        .enumerate()
        .map(|(i, err)| error_to_idl(err, 6000 + i as u32))
        .collect();

    let metadata = program_id.map(|addr| IdlMetadata {
        address: addr.to_string(),
        origin: Some("anchor2pinocchio".to_string()),
    });

    Idl {
        version: "0.1.0".to_string(),
        name: to_snake_case(&program.name),
        instructions,
        accounts,
        errors,
        metadata,
    }
}

fn instruction_to_idl(inst: &PinocchioInstruction) -> IdlInstruction {
    // Calculate discriminator
    let disc = calculate_discriminator("global", &to_snake_case(&inst.name));

    let accounts: Vec<IdlAccountItem> = inst
        .accounts
        .iter()
        .map(|acc| IdlAccountItem {
            name: to_camel_case(&acc.name),
            is_mut: acc.is_writable,
            is_signer: acc.is_signer,
            docs: None,
        })
        .collect();

    let args: Vec<IdlArg> = inst
        .args
        .iter()
        .map(|arg| IdlArg {
            name: to_camel_case(&arg.name),
            ty: rust_type_to_idl_type(&arg.ty),
        })
        .collect();

    IdlInstruction {
        name: to_camel_case(&inst.name),
        docs: None,
        accounts,
        args,
        discriminator: Some(disc.to_vec()),
    }
}

fn state_to_idl_account(state: &PinocchioState) -> IdlAccount {
    let fields: Vec<IdlField> = state
        .fields
        .iter()
        .map(|f: &PinocchioField| IdlField {
            name: to_camel_case(&f.name),
            ty: rust_type_to_idl_type(&f.ty),
            docs: None,
        })
        .collect();

    IdlAccount {
        name: state.name.clone(),
        ty: IdlAccountType {
            kind: "struct".to_string(),
            fields,
        },
    }
}

fn error_to_idl(err: &PinocchioError, code: u32) -> IdlError {
    IdlError {
        code,
        name: err.name.clone(),
        msg: Some(err.msg.clone()),
    }
}

fn rust_type_to_idl_type(ty: &str) -> IdlType {
    let ty = ty.trim();

    // Handle Option<T>
    if ty.starts_with("Option<") && ty.ends_with(">") {
        let inner = &ty[7..ty.len() - 1];
        return IdlType::Option {
            option: Box::new(rust_type_to_idl_type(inner)),
        };
    }

    // Handle Vec<T>
    if ty.starts_with("Vec<") && ty.ends_with(">") {
        let inner = &ty[4..ty.len() - 1];
        return IdlType::Vec {
            vec: Box::new(rust_type_to_idl_type(inner)),
        };
    }

    // Handle [u8; N] arrays
    if ty.starts_with("[u8;") && ty.ends_with("]") {
        if let Some(n_str) = ty.strip_prefix("[u8;").and_then(|s| s.strip_suffix("]")) {
            if let Ok(n) = n_str.trim().parse::<usize>() {
                // Special case: [u8; 32] is often a Pubkey
                if n == 32 {
                    return IdlType::Simple("publicKey".to_string());
                }
                return IdlType::Array {
                    array: (Box::new(IdlType::Simple("u8".to_string())), n),
                };
            }
        }
    }

    // Handle [u8; 32] with spaces
    if ty.contains("[u8") && ty.contains("32") && ty.contains("]") {
        return IdlType::Simple("publicKey".to_string());
    }

    // Simple types
    match ty {
        "u8" => IdlType::Simple("u8".to_string()),
        "u16" => IdlType::Simple("u16".to_string()),
        "u32" => IdlType::Simple("u32".to_string()),
        "u64" => IdlType::Simple("u64".to_string()),
        "u128" => IdlType::Simple("u128".to_string()),
        "i8" => IdlType::Simple("i8".to_string()),
        "i16" => IdlType::Simple("i16".to_string()),
        "i32" => IdlType::Simple("i32".to_string()),
        "i64" => IdlType::Simple("i64".to_string()),
        "i128" => IdlType::Simple("i128".to_string()),
        "bool" => IdlType::Simple("bool".to_string()),
        "String" | "&str" | "str" => IdlType::Simple("string".to_string()),
        "Pubkey" | "pubkey::Pubkey" => IdlType::Simple("publicKey".to_string()),
        _ => {
            // Check if it's a defined type (struct reference)
            if ty.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                IdlType::Defined {
                    defined: ty.to_string(),
                }
            } else {
                IdlType::Simple(ty.to_string())
            }
        }
    }
}

fn calculate_discriminator(namespace: &str, name: &str) -> [u8; 8] {
    let preimage = format!("{}:{}", namespace, name);
    let mut hasher = Sha256::new();
    hasher.update(preimage.as_bytes());
    let result = hasher.finalize();
    let mut disc = [0u8; 8];
    disc.copy_from_slice(&result[..8]);
    disc
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_lowercase().next().unwrap());
        } else {
            result.push(c);
        }
    }
    result
}

fn to_camel_case(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = false;

    for (i, c) in s.chars().enumerate() {
        if c == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_uppercase().next().unwrap());
            capitalize_next = false;
        } else if i == 0 {
            result.push(c.to_lowercase().next().unwrap());
        } else {
            result.push(c);
        }
    }
    result
}

/// IDL verification result
pub struct IdlVerification {
    pub is_compatible: bool,
    pub total_instructions: usize,
    pub matching_instructions: usize,
    pub total_accounts: usize,
    pub matching_accounts: usize,
    pub total_errors: usize,
    pub matching_errors: usize,
    pub issues: Vec<String>,
}

/// Verify generated IDL against original Anchor IDL
pub fn verify_idl(
    generated: &Idl,
    original_path: &std::path::Path,
) -> anyhow::Result<IdlVerification> {
    let original_content = std::fs::read_to_string(original_path)?;
    let original: serde_json::Value = serde_json::from_str(&original_content)?;

    let mut verification = IdlVerification {
        is_compatible: true,
        total_instructions: 0,
        matching_instructions: 0,
        total_accounts: 0,
        matching_accounts: 0,
        total_errors: 0,
        matching_errors: 0,
        issues: Vec::new(),
    };

    // Verify instructions
    if let Some(orig_instructions) = original.get("instructions").and_then(|v| v.as_array()) {
        verification.total_instructions = orig_instructions.len();

        for (i, orig_inst) in orig_instructions.iter().enumerate() {
            let orig_name = orig_inst.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let orig_accounts = orig_inst
                .get("accounts")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            let orig_args = orig_inst
                .get("args")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);

            if let Some(gen_inst) = generated.instructions.get(i) {
                let mut matches = true;

                if gen_inst.name != orig_name {
                    verification.issues.push(format!(
                        "Instruction {}: name mismatch '{}' vs '{}'",
                        i, gen_inst.name, orig_name
                    ));
                    matches = false;
                }

                if gen_inst.accounts.len() != orig_accounts {
                    verification.issues.push(format!(
                        "Instruction '{}': account count mismatch {} vs {}",
                        orig_name,
                        gen_inst.accounts.len(),
                        orig_accounts
                    ));
                    matches = false;
                }

                if gen_inst.args.len() != orig_args {
                    verification.issues.push(format!(
                        "Instruction '{}': arg count mismatch {} vs {}",
                        orig_name,
                        gen_inst.args.len(),
                        orig_args
                    ));
                    matches = false;
                }

                if matches {
                    verification.matching_instructions += 1;
                } else {
                    verification.is_compatible = false;
                }
            } else {
                verification.issues.push(format!(
                    "Instruction '{}' missing from generated IDL",
                    orig_name
                ));
                verification.is_compatible = false;
            }
        }
    }

    // Verify accounts (state structs)
    if let Some(orig_accounts) = original.get("accounts").and_then(|v| v.as_array()) {
        verification.total_accounts = orig_accounts.len();

        for (i, orig_acc) in orig_accounts.iter().enumerate() {
            let orig_name = orig_acc.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let orig_fields = orig_acc
                .get("type")
                .and_then(|t| t.get("fields"))
                .and_then(|f| f.as_array())
                .map(|a| a.len())
                .unwrap_or(0);

            if let Some(gen_acc) = generated.accounts.get(i) {
                let mut matches = true;

                if gen_acc.name != orig_name {
                    verification.issues.push(format!(
                        "Account {}: name mismatch '{}' vs '{}'",
                        i, gen_acc.name, orig_name
                    ));
                    matches = false;
                }

                if gen_acc.ty.fields.len() != orig_fields {
                    verification.issues.push(format!(
                        "Account '{}': field count mismatch {} vs {}",
                        orig_name,
                        gen_acc.ty.fields.len(),
                        orig_fields
                    ));
                    matches = false;
                }

                if matches {
                    verification.matching_accounts += 1;
                } else {
                    verification.is_compatible = false;
                }
            } else {
                verification.issues.push(format!(
                    "Account '{}' missing from generated IDL",
                    orig_name
                ));
                verification.is_compatible = false;
            }
        }
    }

    // Verify errors
    if let Some(orig_errors) = original.get("errors").and_then(|v| v.as_array()) {
        verification.total_errors = orig_errors.len();

        if generated.errors.len() == orig_errors.len() {
            verification.matching_errors = orig_errors.len();
        } else {
            verification.is_compatible = false;
            verification.issues.push(format!(
                "Error count mismatch: {} vs {}",
                generated.errors.len(),
                orig_errors.len()
            ));
        }
    }

    Ok(verification)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("CreatePool"), "create_pool");
        assert_eq!(to_snake_case("addLiquidity"), "add_liquidity");
        assert_eq!(to_snake_case("IDL"), "i_d_l");
    }

    #[test]
    fn test_to_camel_case() {
        assert_eq!(to_camel_case("create_pool"), "createPool");
        assert_eq!(to_camel_case("add_liquidity"), "addLiquidity");
        assert_eq!(to_camel_case("pool"), "pool");
    }

    #[test]
    fn test_rust_type_to_idl_type() {
        match rust_type_to_idl_type("u64") {
            IdlType::Simple(s) => assert_eq!(s, "u64"),
            _ => panic!("Expected simple type"),
        }

        match rust_type_to_idl_type("bool") {
            IdlType::Simple(s) => assert_eq!(s, "bool"),
            _ => panic!("Expected simple type"),
        }

        match rust_type_to_idl_type("[u8; 32]") {
            IdlType::Simple(s) => assert_eq!(s, "publicKey"),
            _ => panic!("Expected publicKey for [u8; 32]"),
        }

        match rust_type_to_idl_type("Option<u64>") {
            IdlType::Option { option } => match *option {
                IdlType::Simple(s) => assert_eq!(s, "u64"),
                _ => panic!("Expected simple u64 inside Option"),
            },
            _ => panic!("Expected Option type"),
        }
    }

    #[test]
    fn test_discriminator_calculation() {
        let disc = calculate_discriminator("global", "create_pool");
        // Should be deterministic
        assert_eq!(disc.len(), 8);
        assert_ne!(disc, [0u8; 8]);

        // Different names should have different discriminators
        let disc2 = calculate_discriminator("global", "add_liquidity");
        assert_ne!(disc, disc2);
    }
}

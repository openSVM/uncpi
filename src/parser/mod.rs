//! Parse Anchor programs into IR

use anyhow::{Context, Result};
use quote::ToTokens;
use std::path::Path;
use syn::{parse_file, Attribute, Field, Item, ItemMod, ItemStruct, Type};

use crate::ir::*;

/// Extracted constants and helper functions from the source
#[derive(Debug, Clone, Default)]
pub struct SourceExtras {
    pub constants: Vec<ConstantDef>,
    pub helper_functions: Vec<HelperFunction>,
}

#[derive(Debug, Clone)]
pub struct ConstantDef {
    pub name: String,
    pub ty: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct HelperFunction {
    #[allow(dead_code)]
    pub name: String,
    pub signature: String,
    pub body: String,
}

pub fn parse_anchor_file(path: &Path) -> Result<AnchorProgram> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("Failed to read {:?}", path))?;

    parse_anchor_source(&content)
}

/// Parse source and extract constants and helper functions
pub fn parse_extras(path: &Path) -> Result<SourceExtras> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("Failed to read {:?}", path))?;

    let file = parse_file(&content).with_context(|| "Failed to parse Rust source")?;

    let mut extras = SourceExtras::default();

    for item in &file.items {
        match item {
            Item::Const(c) => {
                extras.constants.push(ConstantDef {
                    name: c.ident.to_string(),
                    ty: type_to_string(&c.ty),
                    value: tokens_to_string(&c.expr),
                });
            }
            Item::Fn(f) => {
                // Only include non-instruction helper functions
                if !matches!(f.vis, syn::Visibility::Public(_)) {
                    extras.helper_functions.push(HelperFunction {
                        name: f.sig.ident.to_string(),
                        signature: tokens_to_string(&f.sig),
                        body: tokens_to_string(&f.block),
                    });
                }
            }
            _ => {}
        }
    }

    Ok(extras)
}

pub fn parse_anchor_source(source: &str) -> Result<AnchorProgram> {
    let file = parse_file(source).with_context(|| "Failed to parse Rust source")?;

    let mut program = AnchorProgram {
        name: String::new(),
        program_id: None,
        instructions: Vec::new(),
        account_structs: Vec::new(),
        state_structs: Vec::new(),
        errors: Vec::new(),
    };

    // Find declare_id!
    for item in &file.items {
        if let Item::Macro(mac) = item {
            if mac.mac.path.is_ident("declare_id") {
                let tokens = mac.mac.tokens.to_string();
                program.program_id = Some(tokens.trim_matches('"').to_string());
            }
        }
    }

    // Find #[program] module
    for item in &file.items {
        if let Item::Mod(module) = item {
            if has_attribute(&module.attrs, "program") {
                program.name = module.ident.to_string();
                parse_program_module(module, &mut program)?;
            }
        }
    }

    // Find account structs with #[derive(Accounts)]
    for item in &file.items {
        if let Item::Struct(s) = item {
            if has_derive(&s.attrs, "Accounts") {
                program.account_structs.push(parse_account_struct(s)?);
            } else if has_attribute(&s.attrs, "account") {
                program.state_structs.push(parse_state_struct(s)?);
            }
        }
    }

    // Find #[error_code] enums
    for item in &file.items {
        if let Item::Enum(e) = item {
            if has_attribute(&e.attrs, "error_code") {
                program.errors = parse_error_enum(e)?;
            }
        }
    }

    Ok(program)
}

fn parse_program_module(module: &ItemMod, program: &mut AnchorProgram) -> Result<()> {
    if let Some((_, items)) = &module.content {
        for item in items {
            if let Item::Fn(func) = item {
                if matches!(func.vis, syn::Visibility::Public(_)) {
                    let instruction = parse_instruction(func)?;
                    program.instructions.push(instruction);
                }
            }
        }
    }
    Ok(())
}

fn parse_instruction(func: &syn::ItemFn) -> Result<AnchorInstruction> {
    let name = func.sig.ident.to_string();

    let mut accounts_struct = String::new();
    let mut args = Vec::new();

    for input in &func.sig.inputs {
        if let syn::FnArg::Typed(pat_type) = input {
            let param_name = match &*pat_type.pat {
                syn::Pat::Ident(ident) => ident.ident.to_string(),
                _ => continue,
            };

            let ty_str = type_to_string(&pat_type.ty);

            if ty_str.contains("Context") {
                // Extract T from Context<T>
                if let Type::Path(type_path) = &*pat_type.ty {
                    for seg in &type_path.path.segments {
                        if seg.ident == "Context" {
                            if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                                if let Some(syn::GenericArgument::Type(Type::Path(inner))) =
                                    args.args.first()
                                {
                                    accounts_struct = inner
                                        .path
                                        .segments
                                        .last()
                                        .map(|s| s.ident.to_string())
                                        .unwrap_or_default();
                                }
                            }
                        }
                    }
                }
            } else {
                args.push(InstructionArg {
                    name: param_name,
                    ty: ty_str.replace(" ", ""),
                });
            }
        }
    }

    let body = tokens_to_string(&func.block);

    Ok(AnchorInstruction {
        name,
        accounts_struct,
        args,
        body,
    })
}

fn parse_account_struct(s: &ItemStruct) -> Result<AnchorAccountStruct> {
    let name = s.ident.to_string();
    let instruction_args = Vec::new(); // TODO: parse #[instruction(...)]

    let mut accounts = Vec::new();

    if let syn::Fields::Named(fields) = &s.fields {
        for field in &fields.named {
            accounts.push(parse_anchor_account(field)?);
        }
    }

    Ok(AnchorAccountStruct {
        name,
        instruction_args,
        accounts,
    })
}

fn parse_anchor_account(field: &Field) -> Result<AnchorAccount> {
    let name = field
        .ident
        .as_ref()
        .map(|i| i.to_string())
        .unwrap_or_default();

    let ty = parse_account_type(&field.ty);
    let constraints = parse_account_constraints(&field.attrs);

    Ok(AnchorAccount {
        name,
        ty,
        constraints,
    })
}

fn parse_account_type(ty: &Type) -> AccountType {
    let ty_str = type_to_string(ty).replace(" ", "");

    if ty_str.contains("Signer") {
        AccountType::Signer
    } else if ty_str.contains("SystemAccount") {
        AccountType::SystemAccount
    } else if ty_str.contains("UncheckedAccount") {
        AccountType::UncheckedAccount
    } else if ty_str.contains("Program") {
        let inner = extract_generic(&ty_str, "Program");
        AccountType::Program { inner }
    } else if ty_str.contains("Sysvar") {
        let inner = extract_generic(&ty_str, "Sysvar");
        AccountType::Sysvar { inner }
    } else if ty_str.contains("TokenAccount") {
        AccountType::TokenAccount
    } else if ty_str.contains("Mint") {
        AccountType::Mint
    } else if ty_str.starts_with("Box<") {
        let inner_str = ty_str.trim_start_matches("Box<").trim_end_matches('>');
        let inner = parse_account_type_str(inner_str);
        AccountType::Box {
            inner: Box::new(inner),
        }
    } else if ty_str.contains("Account") {
        let inner = extract_generic(&ty_str, "Account");
        AccountType::Account { inner }
    } else {
        AccountType::Account { inner: ty_str }
    }
}

fn parse_account_type_str(s: &str) -> AccountType {
    if s.contains("Account") {
        let inner = extract_generic(s, "Account");
        AccountType::Account { inner }
    } else if s.contains("Mint") {
        AccountType::Mint
    } else if s.contains("TokenAccount") {
        AccountType::TokenAccount
    } else {
        AccountType::Account {
            inner: s.to_string(),
        }
    }
}

fn extract_generic(ty_str: &str, wrapper: &str) -> String {
    if let Some(start) = ty_str.find(&format!("{}<", wrapper)) {
        let rest = &ty_str[start + wrapper.len() + 1..];
        if let Some(end) = rest.rfind('>') {
            let inner = &rest[..end];
            if inner.contains(',') {
                return inner.split(',').last().unwrap_or(inner).trim().to_string();
            }
            return inner.trim().to_string();
        }
    }
    String::new()
}

fn parse_account_constraints(attrs: &[Attribute]) -> Vec<AccountConstraint> {
    let mut constraints = Vec::new();

    for attr in attrs {
        if !attr.path().is_ident("account") {
            continue;
        }

        let tokens = attr_to_string(attr);

        if tokens.contains("mut") {
            constraints.push(AccountConstraint::Mut);
        }

        if tokens.contains("init,") || tokens.contains("init_if_needed") {
            let payer = extract_value(&tokens, "payer");
            let space = extract_value(&tokens, "space");
            if tokens.contains("init_if_needed") {
                constraints.push(AccountConstraint::InitIfNeeded { payer, space });
            } else {
                constraints.push(AccountConstraint::Init { payer, space });
            }
        }

        if tokens.contains("seeds") {
            let seeds = extract_seeds(&tokens);
            constraints.push(AccountConstraint::Seeds(seeds));
        }

        if tokens.contains("bump") {
            let bump = extract_value_optional(&tokens, "bump");
            constraints.push(AccountConstraint::Bump(bump));
        }

        if tokens.contains("token::mint") {
            let mint = extract_value(&tokens, "token::mint");
            constraints.push(AccountConstraint::TokenMint(mint));
        }

        if tokens.contains("token::authority") {
            let auth = extract_value(&tokens, "token::authority");
            constraints.push(AccountConstraint::TokenAuthority(auth));
        }

        if tokens.contains("constraint") {
            let (expr, error) = extract_constraint(&tokens);
            constraints.push(AccountConstraint::Constraint { expr, error });
        }

        if tokens.contains("has_one") {
            let (field, error) = extract_has_one(&tokens);
            constraints.push(AccountConstraint::HasOne { field, error });
        }

        if tokens.contains("close") {
            let target = extract_value(&tokens, "close");
            constraints.push(AccountConstraint::Close(target));
        }
    }

    constraints
}

fn parse_state_struct(s: &ItemStruct) -> Result<AnchorStateStruct> {
    let name = s.ident.to_string();
    let has_init_space = has_derive(&s.attrs, "InitSpace");

    let mut fields = Vec::new();

    if let syn::Fields::Named(named) = &s.fields {
        for field in &named.named {
            let field_name = field
                .ident
                .as_ref()
                .map(|i| i.to_string())
                .unwrap_or_default();
            let field_ty = type_to_string(&field.ty);

            fields.push(StateField {
                name: field_name,
                ty: field_ty,
            });
        }
    }

    Ok(AnchorStateStruct {
        name,
        fields,
        has_init_space,
    })
}

fn parse_error_enum(e: &syn::ItemEnum) -> Result<Vec<AnchorError>> {
    let mut errors = Vec::new();
    let mut code = 6000u32;

    for variant in &e.variants {
        let name = variant.ident.to_string();
        let msg = extract_msg_attr(&variant.attrs);

        errors.push(AnchorError {
            name,
            code: Some(code),
            msg,
        });
        code += 1;
    }

    Ok(errors)
}

// Helper functions

fn has_attribute(attrs: &[Attribute], name: &str) -> bool {
    attrs.iter().any(|a| a.path().is_ident(name))
}

fn has_derive(attrs: &[Attribute], derive_name: &str) -> bool {
    attrs.iter().any(|a| {
        if a.path().is_ident("derive") {
            attr_to_string(a).contains(derive_name)
        } else {
            false
        }
    })
}

fn type_to_string(ty: &Type) -> String {
    let mut tokens = proc_macro2::TokenStream::new();
    ty.to_tokens(&mut tokens);
    tokens.to_string()
}

fn tokens_to_string<T: ToTokens>(t: &T) -> String {
    let mut tokens = proc_macro2::TokenStream::new();
    t.to_tokens(&mut tokens);
    tokens.to_string()
}

fn attr_to_string(attr: &Attribute) -> String {
    let mut tokens = proc_macro2::TokenStream::new();
    attr.to_tokens(&mut tokens);
    tokens.to_string()
}

fn extract_value(s: &str, key: &str) -> String {
    if let Some(idx) = s.find(key) {
        let rest = &s[idx + key.len()..];
        if let Some(eq_idx) = rest.find('=') {
            let value_start = rest[eq_idx + 1..].trim_start();
            let end = value_start
                .find(|c| c == ',' || c == ')' || c == '@')
                .unwrap_or(value_start.len());
            return value_start[..end].trim().to_string();
        }
    }
    String::new()
}

fn extract_value_optional(s: &str, key: &str) -> Option<String> {
    let val = extract_value(s, key);
    if val.is_empty() {
        None
    } else {
        Some(val)
    }
}

fn extract_seeds(s: &str) -> Vec<String> {
    if let Some(start) = s.find("seeds") {
        if let Some(bracket_start) = s[start..].find('[') {
            let rest = &s[start + bracket_start..];
            if let Some(bracket_end) = rest.find(']') {
                let inner = &rest[1..bracket_end];
                return inner
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
        }
    }
    Vec::new()
}

fn extract_constraint(s: &str) -> (String, Option<String>) {
    let expr = extract_value(s, "constraint");
    let error = if expr.contains('@') {
        expr.split('@').nth(1).map(|s| s.trim().to_string())
    } else {
        None
    };
    let clean_expr = expr.split('@').next().unwrap_or(&expr).trim().to_string();
    (clean_expr, error)
}

fn extract_has_one(s: &str) -> (String, Option<String>) {
    let val = extract_value(s, "has_one");
    if val.contains('@') {
        let parts: Vec<&str> = val.split('@').collect();
        (
            parts[0].trim().to_string(),
            Some(parts[1].trim().to_string()),
        )
    } else {
        (val, None)
    }
}

fn extract_msg_attr(attrs: &[Attribute]) -> String {
    for attr in attrs {
        if attr.path().is_ident("msg") {
            let tokens = attr_to_string(attr);
            if let Some(start) = tokens.find('"') {
                if let Some(end) = tokens.rfind('"') {
                    return tokens[start + 1..end].to_string();
                }
            }
        }
    }
    String::new()
}

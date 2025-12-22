# AccountLoader Equivalent Design Document

**Feature**: Zero-copy account deserialization for large state accounts
**Target**: uncpi v0.4.0
**Status**: Design Complete, Ready for Implementation

---

## Overview

Anchor's `AccountLoader<'info, T>` enables zero-copy deserialization of large accounts (10KB+) using `#[account(zero_copy)]`. This feature generates equivalent Pinocchio code with unsafe load methods.

---

## Use Cases

### Large State Accounts (Raydium CLMM)
```rust
// Anchor
#[account(zero_copy(unsafe))]
#[repr(C, packed)]
pub struct PoolState {
    // 10KB+ of fields
    pub tick_array_bitmap: [u64; 16],
    pub reward_infos: [RewardInfo; 3],
    // ... many more fields
}

pub pool_state: AccountLoader<'info, PoolState>,
```

### Usage in Instructions
```rust
// Load immutable reference
let pool = pool_state.load()?;

// Load mutable reference
let mut pool = pool_state.load_mut()?;

// Access fields
pool.liquidity += new_liquidity;
```

---

## Transformation Strategy

### Input Detection

**Pattern 1: Zero-Copy Attribute**
```rust
#[account(zero_copy(unsafe))]
#[repr(C, packed)]
pub struct PoolState {
    // fields
}
```

**Pattern 2: AccountLoader in Accounts Struct**
```rust
#[derive(Accounts)]
pub struct Swap<'info> {
    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,
}
```

### Output Generation

**Transformed State**
```rust
#[repr(C, packed)]
pub struct PoolState {
    // Same fields
}

impl PoolState {
    /// Zero-copy immutable load
    pub unsafe fn load(account: &AccountInfo) -> Result<&Self, ProgramError> {
        let data = account.try_borrow_data()?;
        if data.len() < core::mem::size_of::<Self>() {
            return Err(ProgramError::InvalidAccountData);
        }
        Ok(&*(data.as_ptr() as *const Self))
    }

    /// Zero-copy mutable load
    pub unsafe fn load_mut(account: &AccountInfo) -> Result<&mut Self, ProgramError> {
        let data = account.try_borrow_mut_data()?;
        if data.len() < core::mem::size_of::<Self>() {
            return Err(ProgramError::InvalidAccountData);
        }
        Ok(&mut *(data.as_mut_ptr() as *mut Self))
    }
}
```

**Transformed Usage**
```rust
// In instruction
let pool = unsafe { PoolState::load(pool_state)? };
let mut pool_mut = unsafe { PoolState::load_mut(pool_state)? };
```

---

## IR Extensions

### New Types in `src/ir.rs`

```rust
/// Represents a zero-copy account
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZeroCopyAccount {
    /// Account name
    pub name: String,

    /// Fields in the account
    pub fields: Vec<StateField>,

    /// Whether it uses #[repr(C, packed)]
    pub is_packed: bool,

    /// Whether it's marked unsafe
    pub is_unsafe: bool,

    /// Total size in bytes
    pub size: usize,
}

/// Add to AccountType
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AccountType {
    Account { inner: String },
    Signer,
    SystemProgram,
    TokenProgram,
    Program { name: String },
    UncheckedAccount,
    Sysvar { name: String },
    Interface { program: String },

    // NEW: Zero-copy account loader
    AccountLoader { inner: String },
}

/// Add to AnchorStateStruct
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnchorStateStruct {
    pub name: String,
    pub fields: Vec<StateField>,
    pub has_init_space: bool,

    // NEW: Zero-copy attributes
    #[serde(default)]
    pub is_zero_copy: bool,
    #[serde(default)]
    pub is_packed: bool,
    #[serde(default)]
    pub is_unsafe: bool,
}
```

---

## Parser Changes

### File: `src/parser/mod.rs`

```rust
/// Detect zero_copy attribute
fn has_zero_copy_attribute(attrs: &[Attribute]) -> (bool, bool) {
    for attr in attrs {
        if attr.path().is_ident("account") {
            let tokens = attr_to_string(attr);
            if tokens.contains("zero_copy") {
                let is_unsafe = tokens.contains("unsafe");
                return (true, is_unsafe);
            }
        }
    }
    (false, false)
}

/// Detect repr attribute
fn has_repr_packed(attrs: &[Attribute]) -> bool {
    for attr in attrs {
        if attr.path().is_ident("repr") {
            let tokens = attr_to_string(attr);
            if tokens.contains("packed") || tokens.contains("C, packed") {
                return true;
            }
        }
    }
    false
}

/// Parse state struct with zero-copy detection
fn parse_state_struct(s: &ItemStruct) -> Result<AnchorStateStruct> {
    let name = s.ident.to_string();
    let has_init_space = has_derive(&s.attrs, "InitSpace");

    // Check for zero-copy
    let (is_zero_copy, is_unsafe) = has_zero_copy_attribute(&s.attrs);
    let is_packed = has_repr_packed(&s.attrs);

    let mut fields = Vec::new();

    if let syn::Fields::Named(named) = &s.fields {
        for field in &named.named {
            // Parse fields...
        }
    }

    Ok(AnchorStateStruct {
        name,
        fields,
        has_init_space,
        is_zero_copy,
        is_packed,
        is_unsafe,
    })
}

/// Detect AccountLoader type
fn is_account_loader_type(ty: &Type) -> Option<String> {
    if let Type::Path(TypePath { path, .. }) = ty {
        if let Some(segment) = path.segments.last() {
            if segment.ident == "AccountLoader" {
                if let PathArguments::AngleBracketed(args) = &segment.arguments {
                    // Skip lifetime, get second arg (the type)
                    if let Some(GenericArgument::Type(inner_ty)) = args.args.iter().nth(1) {
                        return Some(quote!(#inner_ty).to_string());
                    }
                }
            }
        }
    }
    None
}

/// Parse account type with AccountLoader detection
fn parse_account_type(field: &Field) -> AccountType {
    let ty = &field.ty;

    // Check for AccountLoader
    if let Some(inner) = is_account_loader_type(ty) {
        return AccountType::AccountLoader { inner };
    }

    // Existing type detection...
}
```

---

## Transformer Changes

### File: `src/transformer/mod.rs`

```rust
/// Transform AccountLoader usage to unsafe load calls
fn transform_account_loader_usage(
    body: &str,
    loader_accounts: &[(String, String)], // (account_name, type_name)
) -> String {
    let mut result = body.to_string();

    for (account_name, type_name) in loader_accounts {
        // Transform .load()
        let load_pattern = format!("{}.load()", account_name);
        let load_replacement = format!(
            "unsafe {{ {}::load({})? }}",
            type_name, account_name
        );
        result = result.replace(&load_pattern, &load_replacement);

        // Transform .load_mut()
        let load_mut_pattern = format!("{}.load_mut()", account_name);
        let load_mut_replacement = format!(
            "unsafe {{ {}::load_mut({})? }}",
            type_name, account_name
        );
        result = result.replace(&load_mut_pattern, &load_mut_replacement);
    }

    result
}

/// Generate instruction transformation with AccountLoader
fn transform_instruction(
    anchor_inst: &AnchorInstruction,
    account_struct: &AnchorAccountStruct,
    // ...
) -> PinocchioInstruction {
    // Collect AccountLoader accounts
    let mut loader_accounts = Vec::new();

    for account in &account_struct.accounts {
        if let AccountType::AccountLoader { inner } = &account.ty {
            loader_accounts.push((account.name.clone(), inner.clone()));
        }
    }

    // Transform body with loader replacements
    let mut transformed_body = transform_context_usage(&anchor_inst.body, account_struct);
    transformed_body = transform_account_loader_usage(&transformed_body, &loader_accounts);

    // ... rest of transformation
}
```

---

## Emitter Changes

### File: `src/emitter/mod.rs`

```rust
/// Emit zero-copy state struct with load methods
pub fn emit_zero_copy_state(state: &AnchorStateStruct) -> String {
    let mut content = String::new();

    // Preserve repr attribute
    if state.is_packed {
        content.push_str("#[repr(C, packed)]\n");
    } else {
        content.push_str("#[repr(C)]\n");
    }

    content.push_str(&format!("pub struct {} {{\n", state.name));

    for field in &state.fields {
        content.push_str(&format!("    pub {}: {},\n", field.name, field.ty));
    }

    content.push_str("}\n\n");

    // Add load methods
    content.push_str(&format!("impl {} {{\n", state.name));

    // Immutable load
    content.push_str(&format!("
    /// Zero-copy immutable load from account
    ///
    /// # Safety
    /// This function performs unsafe pointer casts. The caller must ensure:
    /// - Account data is properly initialized
    /// - Account data size matches struct size
    /// - Account data alignment is correct
    pub unsafe fn load(account: &AccountInfo) -> Result<&Self, ProgramError> {{
        let data = account.try_borrow_data()?;
        if data.len() < core::mem::size_of::<Self>() {{
            return Err(ProgramError::InvalidAccountData);
        }}
        Ok(&*(data.as_ptr() as *const Self))
    }}
"));

    // Mutable load
    content.push_str(&format!("
    /// Zero-copy mutable load from account
    ///
    /// # Safety
    /// This function performs unsafe pointer casts. The caller must ensure:
    /// - Account data is properly initialized
    /// - Account data size matches struct size
    /// - Account data alignment is correct
    /// - No other references to this data exist
    pub unsafe fn load_mut(account: &AccountInfo) -> Result<&mut Self, ProgramError> {{
        let data = account.try_borrow_mut_data()?;
        if data.len() < core::mem::size_of::<Self>() {{
            return Err(ProgramError::InvalidAccountData);
        }}
        Ok(&mut *(data.as_mut_ptr() as *mut Self))
    }}
"));

    content.push_str("}\n");

    content
}

/// Emit state.rs with zero-copy support
pub fn emit_state_rs(program: &PinocchioProgram) -> String {
    let mut content = String::new();

    content.push_str("use pinocchio::{account_info::AccountInfo, program_error::ProgramError};\n\n");

    for state in &program.states {
        if state.is_zero_copy {
            content.push_str(&emit_zero_copy_state(state));
        } else {
            content.push_str(&emit_regular_state(state));
        }
        content.push_str("\n");
    }

    content
}
```

---

## Zero-Copy Module

### File: `src/zero_copy.rs`

```rust
//! Zero-copy account deserialization support

use crate::ir::{AnchorStateStruct, ZeroCopyAccount, StateField};

/// Check if a state struct should use zero-copy
pub fn should_use_zero_copy(state: &AnchorStateStruct) -> bool {
    state.is_zero_copy || estimate_state_size(state) > 10240 // > 10KB
}

/// Estimate size of a state struct
pub fn estimate_state_size(state: &AnchorStateStruct) -> usize {
    state.fields.iter().map(|f| estimate_field_size(&f.ty)).sum()
}

/// Estimate size of a field type
fn estimate_field_size(ty: &str) -> usize {
    match ty {
        "Pubkey" | "[u8; 32]" => 32,
        "u64" | "i64" => 8,
        "u32" | "i32" => 4,
        "u16" | "i16" => 2,
        "u8" | "i8" | "bool" => 1,
        "u128" | "i128" => 16,
        _ => {
            // Try to parse array types [T; N]
            if ty.starts_with('[') && ty.ends_with(']') {
                if let Some(semicolon_pos) = ty.rfind(';') {
                    let element_ty = &ty[1..semicolon_pos].trim();
                    let count_str = ty[semicolon_pos + 1..ty.len() - 1].trim();
                    if let Ok(count) = count_str.parse::<usize>() {
                        return estimate_field_size(element_ty) * count;
                    }
                }
            }
            // Unknown type - conservative estimate
            32
        }
    }
}

/// Generate safety documentation
pub fn generate_safety_doc(is_packed: bool) -> String {
    let mut doc = String::from("/// # Safety\n");
    doc.push_str("/// This function performs unsafe pointer casts. The caller must ensure:\n");
    doc.push_str("/// - Account data is properly initialized\n");
    doc.push_str("/// - Account data size matches struct size\n");

    if is_packed {
        doc.push_str("/// - Struct uses #[repr(C, packed)] for correct layout\n");
        doc.push_str("/// - Be aware of alignment issues with packed structs\n");
    } else {
        doc.push_str("/// - Account data alignment is correct\n");
    }

    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_field_size() {
        assert_eq!(estimate_field_size("Pubkey"), 32);
        assert_eq!(estimate_field_size("u64"), 8);
        assert_eq!(estimate_field_size("[u64; 16]"), 128);
    }

    #[test]
    fn test_should_use_zero_copy_large_state() {
        let state = AnchorStateStruct {
            name: "LargeState".to_string(),
            fields: vec![
                StateField {
                    name: "data".to_string(),
                    ty: "[u8; 20000]".to_string(),
                    max_len: None,
                    is_vec: false,
                    vec_info: None,
                },
            ],
            has_init_space: false,
            is_zero_copy: false,
            is_packed: false,
            is_unsafe: false,
        };

        assert!(should_use_zero_copy(&state));
    }
}
```

---

## Example Transformation

### Input (Anchor)

```rust
#[account(zero_copy(unsafe))]
#[repr(C, packed)]
pub struct PoolState {
    pub liquidity: u128,
    pub sqrt_price_x64: u128,
    pub tick_array_bitmap: [u64; 16],
    pub reward_infos: [RewardInfo; 3],
}

#[derive(Accounts)]
pub struct Swap<'info> {
    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,
    pub user: Signer<'info>,
}

pub fn swap(ctx: Context<Swap>, amount_in: u64) -> Result<()> {
    let mut pool = ctx.accounts.pool_state.load_mut()?;
    pool.liquidity += amount_in as u128;
    Ok(())
}
```

### Output (Pinocchio)

```rust
#[repr(C, packed)]
pub struct PoolState {
    pub liquidity: u128,
    pub sqrt_price_x64: u128,
    pub tick_array_bitmap: [u64; 16],
    pub reward_infos: [RewardInfo; 3],
}

impl PoolState {
    /// Zero-copy mutable load from account
    ///
    /// # Safety
    /// This function performs unsafe pointer casts. The caller must ensure:
    /// - Account data is properly initialized
    /// - Account data size matches struct size
    /// - Struct uses #[repr(C, packed)] for correct layout
    /// - Be aware of alignment issues with packed structs
    pub unsafe fn load_mut(account: &AccountInfo) -> Result<&mut Self, ProgramError> {
        let data = account.try_borrow_mut_data()?;
        if data.len() < core::mem::size_of::<Self>() {
            return Err(ProgramError::InvalidAccountData);
        }
        Ok(&mut *(data.as_mut_ptr() as *mut Self))
    }
}

pub fn swap(
    accounts: &[AccountInfo],
    data: &[u8],
) -> Result<(), ProgramError> {
    let pool_state = &accounts[0];
    let user = &accounts[1];

    // Validation
    if !pool_state.is_writable() {
        return Err(ProgramError::Immutable);
    }
    if !user.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Parse args
    let amount_in = u64::from_le_bytes(
        data.get(0..8)
            .ok_or(ProgramError::InvalidInstructionData)?
            .try_into()
            .unwrap()
    );

    // Zero-copy load
    let mut pool = unsafe { PoolState::load_mut(pool_state)? };
    pool.liquidity += amount_in as u128;

    Ok(())
}
```

---

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_zero_copy_detection() {
        let code = r#"
            #[account(zero_copy(unsafe))]
            #[repr(C, packed)]
            pub struct PoolState {
                pub data: [u8; 10000],
            }
        "#;

        let parsed = parse_anchor_file(code).unwrap();
        let state = &parsed.state_structs[0];

        assert!(state.is_zero_copy);
        assert!(state.is_packed);
        assert!(state.is_unsafe);
    }

    #[test]
    fn test_account_loader_transformation() {
        let body = "let pool = pool_state.load()?;";
        let loaders = vec![("pool_state".to_string(), "PoolState".to_string())];

        let transformed = transform_account_loader_usage(body, &loaders);

        assert!(transformed.contains("unsafe { PoolState::load(pool_state)? }"));
    }
}
```

---

## Implementation Checklist

- [ ] Add zero-copy fields to AnchorStateStruct (`src/ir.rs`)
- [ ] Add AccountLoader to AccountType (`src/ir.rs`)
- [ ] Implement has_zero_copy_attribute() (`src/parser/mod.rs`)
- [ ] Implement is_account_loader_type() (`src/parser/mod.rs`)
- [ ] Update parse_state_struct() for zero-copy (`src/parser/mod.rs`)
- [ ] Implement transform_account_loader_usage() (`src/transformer/mod.rs`)
- [ ] Create zero_copy.rs module
- [ ] Update emit_state_rs() for zero-copy (`src/emitter/mod.rs`)
- [ ] Add zero-copy tests
- [ ] Update documentation
- [ ] Add example (CLMM pool state)

---

*Design Complete: 2025-12-23*
*Ready for Implementation: uncpi v0.4.0*

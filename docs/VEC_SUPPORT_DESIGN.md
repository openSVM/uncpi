# Vec<T> Support Design Document

**Feature**: Transform `Vec<T>` to fixed-size arrays with length tracking
**Target**: uncpi v0.4.0
**Status**: Design Complete, Ready for Implementation

---

## Overview

Solana programs cannot use `Vec<T>` in no_std environments. This feature transforms `Vec<T>` to `[T; N]` arrays with separate length tracking.

---

## Use Cases

### Multisig Programs
```rust
// Anchor
#[account]
pub struct Multisig {
    pub signers: Vec<Pubkey>,
    pub threshold: u64,
}
```

### Dynamic Lists
```rust
// Anchor
#[account]
pub struct Proposal {
    pub voters: Vec<Pubkey>,
    pub vote_counts: Vec<u64>,
}
```

---

## Transformation Strategy

### Input Detection

**Pattern 1: Vec in State Structs**
```rust
#[account]
pub struct MyState {
    #[max_len(10)]
    pub items: Vec<Pubkey>,
}
```

**Pattern 2: Vec in Arguments**
```rust
pub fn process(
    ctx: Context<Process>,
    #[max_len(5)]
    signers: Vec<Pubkey>,
) -> Result<()>
```

**Pattern 3: Vec without max_len**
```rust
pub items: Vec<u64>,  // Use default size
```

### Output Generation

**Transformed State**
```rust
#[repr(C)]
pub struct MyState {
    pub items: [Pubkey; 10],
    pub items_len: u8,
}
```

**Size Calculation**
```rust
impl MyState {
    pub const SIZE: usize =
        32 * 10 +  // items array
        1;         // items_len
}
```

---

## IR Extensions

### New Types in `src/ir.rs`

```rust
/// Represents a Vec field that needs transformation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VecField {
    /// Field name
    pub name: String,

    /// Element type (e.g., "Pubkey", "u64")
    pub element_type: String,

    /// Maximum length from #[max_len(N)] attribute
    pub max_len: Option<usize>,

    /// Inferred or default maximum length
    pub resolved_max_len: usize,

    /// Whether this is mutable
    pub is_mutable: bool,
}

/// Default sizes for common types
pub const DEFAULT_VEC_SIZES: &[(&str, usize)] = &[
    ("Pubkey", 32),      // Max signers in multisig
    ("u64", 100),        // Max amounts/counters
    ("u8", 256),         // Max bytes
    ("String", 10),      // Max string items
    ("AccountInfo", 16), // Max remaining accounts
];

impl VecField {
    /// Get the resolved maximum length
    pub fn get_max_len(&self) -> usize {
        if let Some(len) = self.max_len {
            return len;
        }

        // Look up default for this type
        for (ty, default_len) in DEFAULT_VEC_SIZES {
            if self.element_type == *ty {
                return *default_len;
            }
        }

        // Conservative fallback
        32
    }

    /// Get the length field name
    pub fn length_field_name(&self) -> String {
        format!("{}_len", self.name)
    }

    /// Get the element size in bytes
    pub fn element_size(&self) -> usize {
        match self.element_type.as_str() {
            "Pubkey" => 32,
            "u64" => 8,
            "u32" => 4,
            "u16" => 2,
            "u8" => 1,
            "i64" => 8,
            "i32" => 4,
            "i16" => 2,
            "i8" => 1,
            _ => 0, // Unknown, needs manual size
        }
    }
}

/// Add to StateField
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateField {
    pub name: String,
    pub ty: String,
    pub max_len: Option<usize>,

    // NEW: Track if this is a Vec
    pub is_vec: bool,
    pub vec_info: Option<VecField>,
}
```

---

## Parser Changes

### File: `src/parser/mod.rs`

```rust
/// Parse #[max_len(N)] attribute
fn extract_max_len_for_vec(attrs: &[Attribute]) -> Option<usize> {
    for attr in attrs {
        if attr.path().is_ident("max_len") {
            if let Ok(Meta::List(meta_list)) = attr.parse_meta() {
                for nested in meta_list.nested {
                    if let NestedMeta::Lit(Lit::Int(lit_int)) = nested {
                        if let Ok(val) = lit_int.base10_parse::<usize>() {
                            return Some(val);
                        }
                    }
                }
            }
        }
    }
    None
}

/// Detect if type is Vec<T>
fn is_vec_type(ty: &Type) -> Option<String> {
    if let Type::Path(TypePath { path, .. }) = ty {
        if let Some(segment) = path.segments.last() {
            if segment.ident == "Vec" {
                if let PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(GenericArgument::Type(inner_ty)) = args.args.first() {
                        return Some(quote!(#inner_ty).to_string());
                    }
                }
            }
        }
    }
    None
}

/// Parse state field with Vec detection
fn parse_state_field(field: &Field) -> StateField {
    let field_name = field.ident.as_ref().unwrap().to_string();
    let field_ty = quote!(#field.ty).to_string();
    let max_len = extract_max_len_for_vec(&field.attrs);

    // Check if this is a Vec
    let (is_vec, vec_info) = if let Some(element_type) = is_vec_type(&field.ty) {
        let vec_field = VecField {
            name: field_name.clone(),
            element_type: element_type.clone(),
            max_len,
            resolved_max_len: 0, // Will be resolved in transformer
            is_mutable: true,
        };
        (true, Some(vec_field))
    } else {
        (false, None)
    };

    StateField {
        name: field_name,
        ty: field_ty,
        max_len,
        is_vec,
        vec_info,
    }
}
```

---

## Transformer Changes

### File: `src/transformer/mod.rs`

```rust
/// Transform Vec fields to arrays
fn transform_vec_fields(state: &AnchorState) -> Vec<PinocchioField> {
    let mut fields = Vec::new();
    let mut offset = 0;

    for field in &state.fields {
        if field.is_vec {
            let vec_info = field.vec_info.as_ref().unwrap();
            let max_len = vec_info.get_max_len();
            let element_size = vec_info.element_size();

            // Add array field
            fields.push(PinocchioField {
                name: field.name.clone(),
                ty: format!("[{}; {}]", vec_info.element_type, max_len),
                size: element_size * max_len,
                offset,
                max_len: Some(max_len),
            });
            offset += element_size * max_len;

            // Add length field
            fields.push(PinocchioField {
                name: vec_info.length_field_name(),
                ty: "u8".to_string(),
                size: 1,
                offset,
                max_len: None,
            });
            offset += 1;
        } else {
            // Regular field
            let size = estimate_field_size(&field.ty);
            fields.push(PinocchioField {
                name: field.name.clone(),
                ty: field.ty.clone(),
                size,
                offset,
                max_len: field.max_len,
            });
            offset += size;
        }
    }

    fields
}

/// Transform Vec operations in function body
fn transform_vec_operations(body: &str, vec_fields: &[VecField]) -> String {
    let mut result = body.to_string();

    for vec_field in vec_fields {
        let vec_name = &vec_field.name;
        let len_name = vec_field.length_field_name();

        // vec.push(item) → vec[len] = item; len += 1;
        let push_pattern = format!("{}.push(", vec_name);
        let push_replacement = format!(
            "{{ if {} >= {} {{ return Err(Error::VecOverflow); }} \
            {}[{}] = ",
            len_name, vec_field.get_max_len(), vec_name, len_name
        );
        result = result.replace(&push_pattern, &push_replacement);
        // TODO: Close the block properly

        // vec.len() → len
        result = result.replace(
            &format!("{}.len()", vec_name),
            &len_name
        );

        // vec.is_empty() → len == 0
        result = result.replace(
            &format!("{}.is_empty()", vec_name),
            &format!("{} == 0", len_name)
        );

        // vec.iter() → vec[..len].iter()
        result = result.replace(
            &format!("{}.iter()", vec_name),
            &format!("{}[..{} as usize].iter()", vec_name, len_name)
        );

        // vec.clear() → len = 0
        result = result.replace(
            &format!("{}.clear()", vec_name),
            &format!("{} = 0", len_name)
        );

        // Vec::new() → [Default::default(); N]; len = 0
        result = result.replace(
            &format!("Vec::new()"),
            &format!("[Default::default(); {}]", vec_field.get_max_len())
        );
    }

    result
}
```

---

## Emitter Changes

### File: `src/emitter/mod.rs`

```rust
/// Emit state struct with Vec transformed to arrays
pub fn emit_state_struct(state: &PinocchioState) -> String {
    let mut content = String::new();

    content.push_str(&format!("#[repr(C)]\npub struct {} {{\n", state.name));

    for field in &state.fields {
        content.push_str(&format!("    pub {}: {},\n", field.name, field.ty));
    }

    content.push_str("}\n\n");

    // Add SIZE constant
    let total_size: usize = state.fields.iter().map(|f| f.size).sum();
    content.push_str(&format!("impl {} {{\n", state.name));
    content.push_str(&format!("    pub const SIZE: usize = {};\n", total_size));

    // Add from_account_info
    content.push_str(&format!("
    pub fn from_account_info(info: &AccountInfo) -> Result<&Self, ProgramError> {{
        let data = info.try_borrow_data()?;
        if data.len() < Self::SIZE {{
            return Err(ProgramError::InvalidAccountData);
        }}
        Ok(unsafe {{ &*(data.as_ptr() as *const Self) }})
    }}

    pub fn from_account_info_mut(info: &AccountInfo) -> Result<&mut Self, ProgramError> {{
        let mut data = info.try_borrow_mut_data()?;
        if data.len() < Self::SIZE {{
            return Err(ProgramError::InvalidAccountData);
        }}
        Ok(unsafe {{ &mut *(data.as_mut_ptr() as *mut Self) }})
    }}
"));

    content.push_str("}\n");

    content
}
```

---

## Error Handling

### New Error in `error.rs`

```rust
#[error_code]
pub enum Error {
    // ... existing errors ...

    #[msg("Vec overflow: attempted to push beyond max length")]
    VecOverflow,
}
```

---

## Example Transformation

### Input (Anchor)

```rust
#[account]
#[derive(InitSpace)]
pub struct Multisig {
    #[max_len(10)]
    pub signers: Vec<Pubkey>,
    pub threshold: u64,
    pub nonce: u8,
}

pub fn add_signer(
    ctx: Context<AddSigner>,
    new_signer: Pubkey,
) -> Result<()> {
    let multisig = &mut ctx.accounts.multisig;

    require!(
        multisig.signers.len() < 10,
        ErrorCode::MaxSignersReached
    );

    multisig.signers.push(new_signer);
    Ok(())
}
```

### Output (Pinocchio)

```rust
#[repr(C)]
pub struct Multisig {
    pub signers: [Pubkey; 10],
    pub signers_len: u8,
    pub threshold: u64,
    pub nonce: u8,
}

impl Multisig {
    pub const SIZE: usize =
        32 * 10 +  // signers array
        1 +        // signers_len
        8 +        // threshold
        1;         // nonce

    // from_account_info methods...
}

pub fn add_signer(
    accounts: &[AccountInfo],
    data: &[u8],
) -> Result<(), ProgramError> {
    let multisig_info = &accounts[0];
    let multisig = Multisig::from_account_info_mut(multisig_info)?;

    let new_signer = Pubkey::from(
        data.get(0..32)
            .ok_or(ProgramError::InvalidInstructionData)?
            .try_into()
            .unwrap()
    );

    if multisig.signers_len >= 10 {
        return Err(Error::MaxSignersReached.into());
    }

    if multisig.signers_len >= 10 {
        return Err(Error::VecOverflow.into());
    }
    multisig.signers[multisig.signers_len as usize] = new_signer;
    multisig.signers_len += 1;

    Ok(())
}
```

---

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vec_detection() {
        let code = r#"
            pub struct MyState {
                #[max_len(10)]
                pub items: Vec<Pubkey>,
            }
        "#;

        let parsed = parse_anchor_file(code).unwrap();
        let state = &parsed.state_structs[0];

        assert!(state.fields[0].is_vec);
        assert_eq!(state.fields[0].vec_info.as_ref().unwrap().max_len, Some(10));
    }

    #[test]
    fn test_vec_transformation() {
        let vec_field = VecField {
            name: "items".to_string(),
            element_type: "Pubkey".to_string(),
            max_len: Some(10),
            resolved_max_len: 10,
            is_mutable: true,
        };

        let body = "items.push(new_item);";
        let transformed = transform_vec_operations(body, &[vec_field]);

        assert!(transformed.contains("items[items_len]"));
        assert!(transformed.contains("items_len += 1"));
    }
}
```

### Integration Test

```rust
#[test]
fn test_multisig_transpilation() {
    let input = include_str!("../examples/multisig.rs");
    let output_dir = tempfile::tempdir().unwrap();

    transpile(input, output_dir.path()).unwrap();

    // Verify generated files
    let state_file = output_dir.path().join("src/state.rs");
    let state_content = std::fs::read_to_string(state_file).unwrap();

    assert!(state_content.contains("signers: [Pubkey; 10]"));
    assert!(state_content.contains("signers_len: u8"));
}
```

---

## Implementation Checklist

- [ ] Add VecField to IR (`src/ir.rs`)
- [ ] Implement is_vec_type() parser (`src/parser/mod.rs`)
- [ ] Implement extract_max_len_for_vec() (`src/parser/mod.rs`)
- [ ] Update parse_state_field() to detect Vec (`src/parser/mod.rs`)
- [ ] Implement transform_vec_fields() (`src/transformer/mod.rs`)
- [ ] Implement transform_vec_operations() (`src/transformer/mod.rs`)
- [ ] Update emit_state_struct() for arrays (`src/emitter/mod.rs`)
- [ ] Add VecOverflow error (`src/error.rs`)
- [ ] Write unit tests
- [ ] Write integration tests
- [ ] Update documentation
- [ ] Add example program (multisig)

---

## Future Enhancements

### Dynamic Sizing
- Support runtime Vec sizing with realloc
- Requires Solana 1.10+ features

### Advanced Operations
- `vec.retain(|x| condition)`
- `vec.remove(index)`
- `vec.insert(index, item)`

### Optimization
- Use smaller length type for small vecs (u8 vs usize)
- Pack length into unused bits

---

*Design Complete: 2025-12-23*
*Ready for Implementation: uncpi v0.4.0*

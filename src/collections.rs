//! Vec and VecDeque transformation support
//!
//! This module provides transformations for collection types that aren't
//! available in no_std environments.

use crate::ir::{StateField, VecField};
use serde::{Deserialize, Serialize};

/// Default maximum sizes for Vec<T> when no #[max_len] is specified
pub const DEFAULT_VEC_SIZES: &[(&str, usize)] = &[
    ("Pubkey", 32),      // Max signers in multisig
    ("u64", 100),        // Max amounts/counters
    ("u32", 100),        // Max counts
    ("u16", 256),        // Max small counts
    ("u8", 256),         // Max bytes
    ("i64", 100),        // Max signed amounts
    ("String", 10),      // Max string items
    ("AccountInfo", 16), // Max remaining accounts
];

impl VecField {
    /// Get the resolved maximum length for this Vec
    pub fn get_max_len(&self) -> usize {
        if let Some(len) = self.max_len {
            return len;
        }

        // Look up default for this type
        for (ty, default_len) in DEFAULT_VEC_SIZES {
            if self.element_type == *ty || self.element_type.contains(ty) {
                return *default_len;
            }
        }

        // Conservative fallback
        32
    }

    /// Get the length field name (e.g., "items_len" for "items")
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
            "bool" => 1,
            _ => {
                // For custom types, we can't determine size
                // This will need manual annotation
                0
            }
        }
    }

    /// Get the appropriate length type (u8 for small vecs, u16 for larger)
    pub fn length_type(&self) -> &'static str {
        let max_len = self.get_max_len();
        if max_len <= 255 {
            "u8"
        } else if max_len <= 65535 {
            "u16"
        } else {
            "usize"
        }
    }
}

/// Transform Vec operations in function body
///
/// Replaces:
/// - `vec.push(item)` → bounds check + array assignment + len increment
/// - `vec.len()` → len field access
/// - `vec.is_empty()` → len == 0
/// - `vec.iter()` → array slice iteration
/// - `vec.clear()` → len = 0
/// - `Vec::new()` → array initialization + len = 0
pub fn transform_vec_operations(body: &str, vec_fields: &[VecField]) -> String {
    let mut result = body.to_string();

    for vec_field in vec_fields {
        let vec_name = &vec_field.name;
        let len_name = vec_field.length_field_name();
        let max_len = vec_field.get_max_len();
        let len_ty = vec_field.length_type();

        // Transform vec.push(item)
        // Pattern: vec.push(item)
        // Result: { if len >= MAX { return Err(VecOverflow); } vec[len] = item; len += 1; }
        let push_pattern = format!("{}.push(", vec_name);
        if result.contains(&push_pattern) {
            // This is complex - need to handle the closing paren and semicolon
            // For now, mark it for manual handling in transformer
            result = result.replace(
                &push_pattern,
                &format!("/* TODO: transform push */ {}.push(", vec_name)
            );
        }

        // Transform vec.len()
        result = result.replace(
            &format!("{}.len()", vec_name),
            &format!("{} as usize", len_name)
        );

        // Transform vec.is_empty()
        result = result.replace(
            &format!("{}.is_empty()", vec_name),
            &format!("({} == 0)", len_name)
        );

        // Transform vec.iter()
        result = result.replace(
            &format!("{}.iter()", vec_name),
            &format!("{}[..{} as usize].iter()", vec_name, len_name)
        );

        // Transform vec.clear()
        result = result.replace(
            &format!("{}.clear()", vec_name),
            &format!("{} = 0", len_name)
        );

        // Transform Vec::new()
        result = result.replace(
            "Vec::new()",
            &format!("[Default::default(); {}]", max_len)
        );

        // Transform vec[index]
        // This stays the same - array access works

        // Transform vec.get(index)
        result = result.replace(
            &format!("{}.get(", vec_name),
            &format!("if {} as usize > {}.len() {{ None }} else {{ {}.get(", len_name, vec_name, vec_name)
        );
    }

    result
}

/// Generate Vec helper functions for a state struct
pub fn generate_vec_helpers(state_name: &str, vec_fields: &[VecField]) -> String {
    let mut content = String::new();

    for vec_field in vec_fields {
        let vec_name = &vec_field.name;
        let len_name = vec_field.length_field_name();
        let element_type = &vec_field.element_type;
        let max_len = vec_field.get_max_len();

        content.push_str(&format!("
impl {} {{
    /// Push an item to {}
    pub fn push_{}(&mut self, item: {}) -> Result<(), ProgramError> {{
        if self.{} as usize >= {} {{
            return Err(ProgramError::Custom(0)); // VecOverflow
        }}
        self.{}[self.{} as usize] = item;
        self.{} += 1;
        Ok(())
    }}

    /// Get the length of {}
    pub fn {}_len(&self) -> usize {{
        self.{} as usize
    }}

    /// Check if {} is empty
    pub fn {}_is_empty(&self) -> bool {{
        self.{} == 0
    }}

    /// Clear {} (set length to 0)
    pub fn clear_{}(&mut self) {{
        self.{} = 0;
    }}

    /// Get an iterator over {}
    pub fn {}_iter(&self) -> impl Iterator<Item = &{}> {{
        self.{}[..self.{} as usize].iter()
    }}
}}
", state_name,
   vec_name, vec_name, element_type, len_name, max_len,
   vec_name, len_name, len_name,
   vec_name, vec_name, len_name,
   vec_name, vec_name, len_name,
   vec_name, vec_name, len_name,
   vec_name, vec_name, element_type, vec_name, len_name
        ));
    }

    content
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vec_field_get_max_len() {
        let vec_field = VecField {
            name: "items".to_string(),
            element_type: "Pubkey".to_string(),
            max_len: Some(10),
            resolved_max_len: 0,
            is_mutable: true,
        };

        assert_eq!(vec_field.get_max_len(), 10);
    }

    #[test]
    fn test_vec_field_default_max_len() {
        let vec_field = VecField {
            name: "items".to_string(),
            element_type: "Pubkey".to_string(),
            max_len: None,
            resolved_max_len: 0,
            is_mutable: true,
        };

        assert_eq!(vec_field.get_max_len(), 32); // Default for Pubkey
    }

    #[test]
    fn test_length_field_name() {
        let vec_field = VecField {
            name: "signers".to_string(),
            element_type: "Pubkey".to_string(),
            max_len: Some(10),
            resolved_max_len: 0,
            is_mutable: true,
        };

        assert_eq!(vec_field.length_field_name(), "signers_len");
    }

    #[test]
    fn test_element_size() {
        let vec_field = VecField {
            name: "items".to_string(),
            element_type: "u64".to_string(),
            max_len: None,
            resolved_max_len: 0,
            is_mutable: true,
        };

        assert_eq!(vec_field.element_size(), 8);
    }

    #[test]
    fn test_transform_vec_len() {
        let vec_field = VecField {
            name: "items".to_string(),
            element_type: "Pubkey".to_string(),
            max_len: Some(10),
            resolved_max_len: 10,
            is_mutable: true,
        };

        let body = "let count = items.len();";
        let transformed = transform_vec_operations(body, &[vec_field]);

        assert!(transformed.contains("items_len as usize"));
    }

    #[test]
    fn test_transform_vec_is_empty() {
        let vec_field = VecField {
            name: "items".to_string(),
            element_type: "Pubkey".to_string(),
            max_len: Some(10),
            resolved_max_len: 10,
            is_mutable: true,
        };

        let body = "if items.is_empty() { return; }";
        let transformed = transform_vec_operations(body, &[vec_field]);

        assert!(transformed.contains("(items_len == 0)"));
    }

    #[test]
    fn test_transform_vec_iter() {
        let vec_field = VecField {
            name: "signers".to_string(),
            element_type: "Pubkey".to_string(),
            max_len: Some(10),
            resolved_max_len: 10,
            is_mutable: true,
        };

        let body = "for signer in signers.iter() {}";
        let transformed = transform_vec_operations(body, &[vec_field]);

        assert!(transformed.contains("signers[..signers_len as usize].iter()"));
    }
}

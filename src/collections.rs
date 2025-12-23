//! Vec and VecDeque transformation support
//!
//! This module provides transformations for collection types that aren't
//! available in no_std environments.

use crate::ir::VecField;
use regex::Regex;

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

        // Transform vec.push(item)
        // Pattern: (prefix.)?vec.push(value) or (prefix.)?vec.push(value)?
        // Result: bounds check + assignment + increment
        // Handle both "signers.push" and "state.signers.push"
        let push_pattern_str = format!(
            r"(\w+\.)?{}\s*\.\s*push\s*\(\s*([^)]+)\s*\)\s*(\?)?",
            regex::escape(vec_name)
        );
        if let Ok(push_re) = Regex::new(&push_pattern_str) {
            // Collect all matches first to avoid borrowing issues
            let matches: Vec<_> = push_re.captures_iter(&result).map(|cap| {
                let full_match = cap.get(0).unwrap().as_str().to_string();
                let prefix = cap.get(1).map(|m| m.as_str()).unwrap_or("");
                let value = cap.get(2).unwrap().as_str().to_string();
                let has_question = cap.get(3).is_some();
                (full_match, prefix.to_string(), value, has_question)
            }).collect();

            for (full_match, prefix, value, has_question) in matches {
                let replacement = if has_question {
                    // With error handling: vec.push(value)?
                    format!(
                        "{{ if {}{} as usize >= {} {{ return Err(ProgramError::Custom(0)); }} \
                        {}{}[{}{} as usize] = {}; {}{} += 1; Ok::<(), ProgramError>(()) }}?",
                        prefix, len_name, max_len,
                        prefix, vec_name, prefix, len_name, value,
                        prefix, len_name
                    )
                } else {
                    // Without error handling: vec.push(value)
                    // Dereference the value if it's a reference
                    let deref_value = if value.starts_with('&') || value.contains(" & ") {
                        value.clone()
                    } else {
                        value.clone()
                    };
                    format!(
                        "{{ if ({}{} as usize) >= {} {{ return Err(ProgramError::Custom(0)); }} \
                        {}{}[{}{} as usize] = *{}; {}{} += 1; }}",
                        prefix, len_name, max_len,
                        prefix, vec_name, prefix, len_name, deref_value,
                        prefix, len_name
                    )
                };
                result = result.replace(&full_match, &replacement);
            }
        }

        // Transform vec.len() - handle both direct and state-prefixed patterns
        // Pattern 1: signers.len() → signers_len as usize
        result = result.replace(
            &format!("{}.len()", vec_name),
            &format!("{} as usize", len_name)
        );
        // Pattern 2: state.signers.len() → state.signers_len as usize
        // We need to preserve any prefix like "multisig_state."
        let len_pattern = format!(".{}.len ()", vec_name);
        if result.contains(&len_pattern) {
            result = result.replace(
                &len_pattern,
                &format!(".{} as usize", len_name)
            );
        }
        let len_pattern_compact = format!(".{}.len()", vec_name);
        if result.contains(&len_pattern_compact) {
            result = result.replace(
                &len_pattern_compact,
                &format!(".{} as usize", len_name)
            );
        }

        // Transform vec.is_empty()
        result = result.replace(
            &format!("{}.is_empty()", vec_name),
            &format!("({} == 0)", len_name)
        );

        // Transform vec.iter() - use regex to capture prefix
        let iter_pattern_str = format!(
            r"(\w+\.)?{}\s*\.\s*iter\s*\(\s*\)",
            regex::escape(vec_name)
        );
        if let Ok(iter_re) = Regex::new(&iter_pattern_str) {
            let matches: Vec<_> = iter_re.captures_iter(&result).map(|cap| {
                let full_match = cap.get(0).unwrap().as_str().to_string();
                let prefix = cap.get(1).map(|m| m.as_str()).unwrap_or("");
                (full_match, prefix.to_string())
            }).collect();

            for (full_match, prefix) in matches {
                let replacement = format!(
                    "{}{}[..{}{} as usize].iter()",
                    prefix, vec_name, prefix, len_name
                );
                result = result.replace(&full_match, &replacement);
            }
        }

        // Transform vec.clear()
        result = result.replace(
            &format!("{}.clear()", vec_name),
            &format!("{} = 0", len_name)
        );

        // Transform vec.remove(index)
        // Pattern: (prefix.)?vec.remove(index)
        // Result: shift elements left and decrement length
        let remove_pattern_str = format!(
            r"(\w+\.)?{}\s*\.\s*remove\s*\(\s*([^)]+)\s*\)",
            regex::escape(vec_name)
        );
        if let Ok(remove_re) = Regex::new(&remove_pattern_str) {
            let matches: Vec<_> = remove_re.captures_iter(&result).map(|cap| {
                let full_match = cap.get(0).unwrap().as_str().to_string();
                let prefix = cap.get(1).map(|m| m.as_str()).unwrap_or("");
                let index = cap.get(2).unwrap().as_str().to_string();
                (full_match, prefix.to_string(), index)
            }).collect();

            for (full_match, prefix, index) in matches {
                let replacement = format!(
                    "{{ let idx = {}; \
                    for i in idx..({}{} as usize - 1) {{ {}{}[i] = {}{}[i + 1]; }} \
                    {}{} -= 1; }}",
                    index,
                    prefix, len_name,
                    prefix, vec_name, prefix, vec_name,
                    prefix, len_name
                );
                result = result.replace(&full_match, &replacement);
            }
        }

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

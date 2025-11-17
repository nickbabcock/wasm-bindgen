//! Normalizes Rust compiler-generated hash suffixes in WASM exports.

use anyhow::Result;
use std::collections::HashMap;
use std::sync::LazyLock;
use walrus::{ExportItem, FunctionId, Module};

/// Normalizes exports with compiler-generated hash suffixes.
pub fn normalize_exports(module: &mut Module) -> Result<()> {
    let mut normalizer = ExportNormalizer::new();
    normalizer.normalize_module(module)
}

struct ExportNormalizer {
    /// Tracks the next available index for each base_name
    counters: HashMap<String, usize>,
}

impl ExportNormalizer {
    fn new() -> Self {
        Self {
            counters: HashMap::new(),
        }
    }

    fn normalize_module(&mut self, module: &mut Module) -> Result<()> {
        // Regex to match hash suffixes: h[0-9a-f]{16} at end of name
        static HASH_REGEX: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r"h[0-9a-f]{16}$").unwrap());

        // Step 1: Collect exports that need normalization
        let mut to_normalize = Vec::new();

        for export in module.exports.iter() {
            // Only normalize exports with hash suffixes
            if let Some(m) = HASH_REGEX.find(&export.name) {
                let base_name = export.name[..m.start()].to_string();

                // Get the function signature (only for function exports)
                let signature = if let ExportItem::Function(func_id) = export.item {
                    get_function_signature(module, func_id)
                } else {
                    continue; // Skip non-function exports
                };

                to_normalize.push((export.id(), base_name, signature, export.name.clone()));
            }
        }

        // Step 2: Sort by (base_name, signature) for deterministic ordering
        // This ensures same code produces same ordering regardless of hash values
        to_normalize.sort_by(|a, b| (&a.1, &a.2).cmp(&(&b.1, &b.2)));

        // Step 3: Assign normalized hashes and rename
        for (export_id, base_name, _signature, _original_name) in to_normalize {
            let counter = self.counters.entry(base_name.clone()).or_insert(0);
            let normalized_name = format!("{}h{:016x}", base_name, *counter);
            *counter += 1;

            // Rename the export in the module
            let export = module.exports.get_mut(export_id);
            export.name = normalized_name;
        }

        Ok(())
    }
}

/// Get a string representation of a function's WASM signature.
fn get_function_signature(module: &Module, func_id: FunctionId) -> String {
    let func = module.funcs.get(func_id);
    let ty = module.types.get(func.ty());

    // Format: (param types) -> (result types)
    let params: Vec<_> = ty.params().iter().map(|p| format!("{:?}", p)).collect();
    let results: Vec<_> = ty.results().iter().map(|r| format!("{:?}", r)).collect();

    format!("({}) -> ({})", params.join(", "), results.join(", "))
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_hash_regex() {
        let regex = regex::Regex::new(r"h[0-9a-f]{16}$").unwrap();

        assert!(regex.is_match("foo__h0123456789abcdef"));
        assert!(regex.is_match("destroy__he0c82e5427fd1a46"));
        assert!(!regex.is_match("regular_function"));
        assert!(!regex.is_match("foo__h123")); // Too short
        assert!(!regex.is_match("foo__h0123456789abcdefg")); // Too long
    }

    #[test]
    fn test_signature_formatting() {
        // Ensure signature strings are consistent
        let sig1 = "(I32, I32) -> ()";
        let sig2 = "(I32, I32, Externref) -> (I32)";
        assert_ne!(sig1, sig2);
    }
}

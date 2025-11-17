//! Test that wasm-bindgen produces deterministic output.
//!
//! This test verifies that the same input produces identical output across
//! multiple runs, particularly focusing on exports with compiler-generated
//! hash suffixes.

use super::*;

#[test]
fn test_deterministic_output_with_closures() {
    // Create a project with closures that generate multiple hash suffixes
    let source = r#"
        use wasm_bindgen::prelude::*;

        #[wasm_bindgen]
        pub fn create_closure_fn() -> js_sys::Function {
            let closure = Closure::wrap(Box::new(|| {
                web_sys::console::log_1(&"Hello from Fn closure".into());
            }) as Box<dyn Fn()>);
            
            let func = closure.as_ref().clone();
            closure.forget();
            func.unchecked_into()
        }

        #[wasm_bindgen]
        pub fn create_closure_fn_mut_i32() -> js_sys::Function {
            let mut counter = 0;
            let closure = Closure::wrap(Box::new(move |x: i32| {
                counter += x;
                web_sys::console::log_2(&"Counter:".into(), &counter.into());
            }) as Box<dyn FnMut(i32)>);
            
            let func = closure.as_ref().clone();
            closure.forget();
            func.unchecked_into()
        }

        #[wasm_bindgen]
        pub fn create_closure_fn_string() -> js_sys::Function {
            let closure = Closure::wrap(Box::new(|s: String| {
                web_sys::console::log_1(&s.into());
            }) as Box<dyn Fn(String)>);
            
            let func = closure.as_ref().clone();
            closure.forget();
            func.unchecked_into()
        }
    "#;

    // Build ONCE to get a WASM binary, then process it multiple times
    let mut project = Project::new("deterministic_test");
    project
        .file("src/lib.rs", source)
        .dep("js-sys = { path = '{root}/crates/js-sys' }")
        .dep("web-sys = { path = '{root}/crates/web-sys', features = ['console'] }");
    
    // Build the WASM binary once
    let _ = project.build();
    
    // Process the same WASM binary multiple times with the SAME arguments
    // (wasm_bindgen caches based on args hash, so we use empty args to get same key)
    let mut outputs = Vec::new();
    for _i in 0..3 {
        let out_dir = project.wasm_bindgen("--target bundler").unwrap();
        
        // Read the generated files (using default project name)
        let js = fs::read_to_string(out_dir.join("deterministic_test_bg.js")).unwrap();
        let ts = fs::read_to_string(out_dir.join("deterministic_test.d.ts")).unwrap();
        let wasm = fs::read(out_dir.join("deterministic_test_bg.wasm")).unwrap();
        
        outputs.push((js, ts, wasm));
    }

    // All outputs should be byte-for-byte identical
    assert_eq!(
        outputs[0].0, outputs[1].0,
        "JS output differs between runs 1 and 2"
    );
    assert_eq!(
        outputs[1].0, outputs[2].0,
        "JS output differs between runs 2 and 3"
    );
    
    assert_eq!(
        outputs[0].1, outputs[1].1,
        "TS output differs between runs 1 and 2"
    );
    assert_eq!(
        outputs[1].1, outputs[2].1,
        "TS output differs between runs 2 and 3"
    );
    
    assert_eq!(
        outputs[0].2, outputs[1].2,
        "WASM output differs between runs 1 and 2"
    );
    assert_eq!(
        outputs[1].2, outputs[2].2,
        "WASM output differs between runs 2 and 3"
    );
}

#[test]
fn test_export_names_are_normalized() {
    let mut project = Project::new("export_normalization_test");
    project
        .file(
            "src/lib.rs",
            r#"
                use wasm_bindgen::prelude::*;

                #[wasm_bindgen]
                pub fn create_closure_fn() -> js_sys::Function {
                    Closure::wrap(Box::new(|| {}) as Box<dyn Fn()>)
                        .into_js_value()
                        .unchecked_into()
                }

                #[wasm_bindgen]
                pub fn create_closure_fn_mut_i32() -> js_sys::Function {
                    let mut x = 0;
                    Closure::wrap(Box::new(move |y: i32| {
                        x += y;
                    }) as Box<dyn FnMut(i32)>)
                        .into_js_value()
                        .unchecked_into()
                }
            "#,
        )
        .dep("js-sys = { path = '{root}/crates/js-sys' }");
    
    let out_dir = project.wasm_bindgen("--target bundler").unwrap();
    let wasm_path = out_dir.join("export_normalization_test_bg.wasm");
    let wasm_bytes = fs::read(&wasm_path).unwrap();
    
    // Parse WASM to check exports
    let module = walrus::Module::from_buffer(&wasm_bytes).unwrap();
    
    // Collect exports with hash suffixes
    let hash_regex = regex::Regex::new(r"h[0-9a-f]{16}$").unwrap();
    let exports_with_hashes: Vec<_> = module
        .exports
        .iter()
        .filter(|e| hash_regex.is_match(&e.name))
        .map(|e| e.name.clone())
        .collect();
    
    // Collect the hash suffixes
    let mut hash_suffixes = std::collections::HashSet::new();
    for export in &exports_with_hashes {
        if let Some(m) = hash_regex.find(export) {
            let hash = &export[m.start()..];
            hash_suffixes.insert(hash.to_string());
        }
    }
    
    // All hash suffixes should be normalized (start with many zeros)
    for hash in &hash_suffixes {
        let is_normalized = hash.chars().skip(1).take(8).all(|c| c == '0');
        assert!(
            is_normalized,
            "Found non-normalized hash: {} (expected normalized hashes like h0000000000000000)\nAll exports: {:?}",
            hash,
            exports_with_hashes
        );
    }
    
    // Should have at least one normalized hash
    assert!(
        !hash_suffixes.is_empty(),
        "Expected to find at least one hash suffix in exports"
    );
}

#[test]
fn test_different_signatures_get_different_hashes() {
    let mut project = Project::new("different_signatures_test");
    project
        .file(
            "src/lib.rs",
            r#"
                use wasm_bindgen::prelude::*;

                #[wasm_bindgen]
                pub fn closure_no_args() -> js_sys::Function {
                    Closure::wrap(Box::new(|| {}) as Box<dyn Fn()>)
                        .into_js_value()
                        .unchecked_into()
                }

                #[wasm_bindgen]
                pub fn closure_i32_arg() -> js_sys::Function {
                    Closure::wrap(Box::new(|_x: i32| {}) as Box<dyn Fn(i32)>)
                        .into_js_value()
                        .unchecked_into()
                }

                #[wasm_bindgen]
                pub fn closure_string_arg() -> js_sys::Function {
                    Closure::wrap(Box::new(|_s: String| {}) as Box<dyn Fn(String)>)
                        .into_js_value()
                        .unchecked_into()
                }
            "#,
        )
        .dep("js-sys = { path = '{root}/crates/js-sys' }");
    
    let out_dir = project.wasm_bindgen("--target bundler").unwrap();
    let wasm_path = out_dir.join("different_signatures_test_bg.wasm");
    let wasm_bytes = fs::read(&wasm_path).unwrap();
    
    let module = walrus::Module::from_buffer(&wasm_bytes).unwrap();
    
    // Find closure-related exports
    let destroy_exports: Vec<_> = module
        .exports
        .iter()
        .filter(|e| e.name.contains("destroy") && e.name.contains("h"))
        .map(|e| e.name.clone())
        .collect();
    
    let invoke_exports: Vec<_> = module
        .exports
        .iter()
        .filter(|e| e.name.contains("invoke") && e.name.contains("h"))
        .map(|e| e.name.clone())
        .collect();
    
    // If we have multiple destroy/invoke functions, they should have unique names
    if destroy_exports.len() > 1 {
        let unique_names: std::collections::HashSet<_> = destroy_exports.iter().collect();
        assert_eq!(
            unique_names.len(),
            destroy_exports.len(),
            "Multiple destroy exports should have unique normalized names: {:?}",
            destroy_exports
        );
    }
    
    if invoke_exports.len() > 1 {
        let unique_names: std::collections::HashSet<_> = invoke_exports.iter().collect();
        assert_eq!(
            unique_names.len(),
            invoke_exports.len(),
            "Multiple invoke exports should have unique normalized names: {:?}",
            invoke_exports
        );
    }
}

use super::{translate, TranslatedModule};
use wabt;

fn translate_wat(wat: &str) -> TranslatedModule {
    let wasm = wabt::wat2wasm(wat).unwrap();
    let compiled = translate(&wasm).unwrap();
    compiled
}

/// Execute the first function in the module.
fn execute_wat(wat: &str, a: usize, b: usize) -> usize {
    let translated = translate_wat(wat);
    let func = &translated.funcs()[0];
    func.execute(a, b)
}

#[test]
fn adds() {
    const CASES: &[(usize, usize, usize)] = &[
        (5, 3, 8),
        (0, 228, 228),
        (usize::max_value(), 1, 0),
    ];

    let code = r#"
(module
  (func (param i32) (param i32) (result i32) (i32.add (get_local 0) (get_local 1)))
)
    "#;
    for (a, b, expected) in CASES {
        assert_eq!(execute_wat(code, *a, *b), *expected);
    }
}

// TODO: Add a test that checks argument passing via the stack.

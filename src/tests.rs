use super::{translate, TranslatedModule};
use wabt;

fn translate_wat(wat: &str) -> TranslatedModule {
    let wasm = wabt::wat2wasm(wat).unwrap();
    let compiled = translate(&wasm).unwrap();
    compiled
}

/// Execute the first function in the module.
fn execute_wat(wat: &str, a: u32, b: u32) -> u32 {
    let translated = translate_wat(wat);
    unsafe { translated.execute_func(0, (a, b)) }
}

#[test]
fn empty() {
    let _ = translate_wat("(module (func))");
}

macro_rules! binop_test {
    ($op:ident, $func:path) => {
        quickcheck! {
            fn $op(a: u32, b: u32) -> bool {
                static CODE: &str = concat!(
                    "(module (func (param i32) (param i32) (result i32) (i32.",
                    stringify!($op),
                    " (get_local 0) (get_local 1))))"
                );

                lazy_static! {
                    static ref TRANSLATED: TranslatedModule = translate_wat(CODE);
                }

                unsafe { TRANSLATED.execute_func::<(u32, u32), u32>(0, (a, b)) == $func(a, b) }
            }
        }
    };
}

binop_test!(add, u32::wrapping_add);
binop_test!(sub, u32::wrapping_sub);
binop_test!(and, std::ops::BitAnd::bitand);
binop_test!(or, std::ops::BitOr::bitor);
binop_test!(xor, std::ops::BitXor::bitxor);
binop_test!(mul, u32::wrapping_mul);

#[test]
fn relop_eq() {
    const CASES: &[(u32, u32, u32)] = &[
        (0, 0, 1),
        (0, 1, 0),
        (1, 0, 0),
        (1, 1, 1),
        (1312, 1, 0),
        (1312, 1312, 1),
    ];

    let code = r#"
(module
  (func (param i32) (param i32) (result i32) (i32.eq (get_local 0) (get_local 1)))
)
    "#;

    for (a, b, expected) in CASES {
        assert_eq!(execute_wat(code, *a, *b), *expected);
    }
}

#[test]
fn if_then_else() {
    const CASES: &[(u32, u32, u32)] = &[
        (0, 1, 1),
        (0, 0, 0),
        (1, 0, 0),
        (1, 1, 1),
        (1312, 1, 1),
        (1312, 1312, 1312),
    ];

    let code = r#"
(module
  (func (param i32) (param i32) (result i32)
    (if (result i32)
      (i32.eq
        (get_local 0)
        (get_local 1)
      )
      (then (get_local 0))
      (else (get_local 1))
    )
  )
)
    "#;

    for (a, b, expected) in CASES {
        assert_eq!(execute_wat(code, *a, *b), *expected, "{}, {}", a, b);
    }
}

#[test]
fn if_without_result() {
    let code = r#"
(module
  (func (param i32) (param i32) (result i32)
    (if
      (i32.eq
        (get_local 0)
        (get_local 1)
      )
      (then (unreachable))
    )

    (get_local 0)
  )
)
    "#;

    assert_eq!(execute_wat(code, 2, 3), 2);
}

#[test]
fn function_call() {
    let code = r#"
(module
  (func (param i32) (param i32) (result i32)
    (call $assert_zero
      (get_local 1)
    )
    (get_local 0)
  )

  (func $assert_zero (param $v i32)
    (local i32)
    (if (get_local $v)
      (unreachable)
    )
  )
)
    "#;

    assert_eq!(execute_wat(code, 2, 0), 2);
}

#[test]
fn large_function_call() {
    let code = r#"
(module
  (func (param i32) (param i32) (param i32) (param i32)
        (param i32) (param i32)
        (result i32)

    (call $assert_zero
      (get_local 5)
    )
    (get_local 0)
  )

  (func $assert_zero (param $v i32)
    (local i32)
    (if (get_local $v)
      (unreachable)
    )
  )
)
    "#;

    assert_eq!(
        {
            let translated = translate_wat(code);
            let out: u32 = unsafe { translated.execute_func(0, (5, 4, 3, 2, 1, 0)) };
            out
        },
        5
    );
}

#[test]
fn literals() {
    let code = r#"
(module
  (func (param i32) (param i32) (result i32)
    (i32.const 228)
  )
)
    "#;

    assert_eq!(execute_wat(code, 0, 0), 228);
}

#[test]
fn fib() {
    let code = r#"
(module
  (func $fib (param $n i32) (param $_unused i32) (result i32)
    (if (result i32)
      (i32.eq
        (i32.const 0)
        (get_local $n)
      )
      (then
        (i32.const 1)
      )
      (else
        (if (result i32)
          (i32.eq
            (i32.const 1)
            (get_local $n)
          )
          (then
            (i32.const 1)
          )
          (else
            (i32.add
              ;; fib(n - 1)
              (call $fib
                (i32.add
                  (get_local $n)
                  (i32.const -1)
                )
                (i32.const 0)
              )
              ;; fib(n - 2)
              (call $fib
                (i32.add
                  (get_local $n)
                  (i32.const -2)
                )
                (i32.const 0)
              )
            )
          )
        )
      )
    )
  )
)
    "#;

    // fac(x) = y <=> (x, y)
    const FIB_SEQ: &[u32] = &[1, 1, 2, 3, 5, 8, 13, 21, 34, 55];

    for x in 0..10 {
        assert_eq!(execute_wat(code, x, 0), FIB_SEQ[x as usize]);
    }
}

// TODO: Add a test that checks argument passing via the stack.

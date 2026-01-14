//! Integration tests for the Kitlang interpreter.
//! These tests verify that the interpreter produces correct output for various programs.

use std::sync::Mutex;

use kitlang::interpreter::mir_interpreter::Value;
use kitlang::{
    execute_source_string, execute_source_string_no_io, execute_source_string_no_std,
    register_native_fn,
};

mod common;
#[allow(unused_imports)]
use common::*;
use kitlang_macros::kitlang_native_fn;

macro_rules! execute_code {
    ($code:expr, $native_fns:expr) => {{ execute_source_string($code, $native_fns, false) }};
    (no_io, $code:expr, $native_fns:expr) => {{ execute_source_string_no_io($code, $native_fns, false) }};
    (no_lib, $code:expr, $native_fns:expr) => {{ execute_source_string_no_std($code, $native_fns, false) }};
}

macro_rules! assert_execution {
    (no_lib, $program:expr, $native_fns:expr) => {{
        let result = execute_code!(no_lib, $program, |_| {});
        match result {
            Ok(value) => value,
            Err(e) => panic!("Program execution failed: {}", e),
        }
    }};
    (no_lib, $program:expr) => {
        assert_execution!(no_lib, $program, |_| {})
    };
    ($program:expr, $native_fns:expr) => {{
        let result = execute_code!($program, $native_fns);
        match result {
            Ok(value) => value,
            Err(e) => {
                let msg = e.format_as_error_message($program);
                panic!("{}", msg);
            }
        }
    }};
    ($program:expr) => {
        assert_execution!($program, |_| {})
    };
}

macro_rules! assert_execution_result {
    (no_lib, $program:expr, $expected:expr) => {{
        let result = assert_execution!(no_lib, $program);
        assert_eq!(
            result, $expected,
            "Program produced {:?} but expected {:?}",
            result, $expected
        )
    }};
    ($program:expr, $expected:expr) => {{
        let result = assert_execution!($program);
        assert_eq!(
            result, $expected,
            "Program produced {:?} but expected {:?}",
            result, $expected
        )
    }};
}

macro_rules! assert_execution_any_int {
    ($program:expr, $expected:expr) => {{
        let result = assert_execution!($program);
        match result {
            Value::Integer(value) => {
                assert_eq!(
                    value, $expected,
                    "Expected integer {} but got {}",
                    $expected, value
                )
            }
            Value::UnsignedInteger(value) => {
                assert_eq!(
                    value, $expected as u64,
                    "Expected integer {} but got {}",
                    $expected, value
                )
            }
            value => panic!("Program produced non-integer value: {:?}", value),
        }
    }};
}

macro_rules! assert_execution_float {
    ($program:expr, $expected:expr) => {
        let result = assert_execution!($program);
        match result {
            Value::Float(value) => {
                assert!(
                    (value - $expected).abs() < 1e-10,
                    "Expected float {} but got {}",
                    $expected,
                    value
                )
            }
            value => panic!("Program produced non-float value: {:?}", value),
        }
    };
}

macro_rules! assert_execution_string {
    ($program:expr, $expected:expr) => {
        let result = assert_execution!($program);
        match result {
            Value::String(value) => {
                assert_eq!(
                    &value, $expected,
                    "Expected string \"{}\" but got \"{}\"",
                    $expected, value
                )
            }
            value => panic!("Program produced non-string value: {:?}", value),
        }
    };
}

macro_rules! assert_execution_bool {
    ($program:expr, $expected:expr) => {
        let result = assert_execution!($program);
        match result {
            Value::Boolean(value) => {
                assert_eq!(
                    value, $expected,
                    "Expected boolean {} but got {}",
                    $expected, value
                )
            }
            value => panic!("Program produced non-boolean value: {:?}", value),
        }
    };
}

// Test the standard library compiles successfully.

#[test]
fn interp_stdlib_compiles() {
    let program = main!("");
    assert_execution_result!(program, Value::Unit);
}

#[test]
fn interp_no_stdlib_compiles() {
    let program = main!("");
    assert_execution_result!(no_lib, program, Value::Unit);
}

// Integer arithmetic..

#[test]
fn interp_addition() {
    assert_execution_any_int!(main!("2 + 3", "i32"), 5);
}

#[test]
fn interp_subtraction() {
    assert_execution_any_int!(main!("10 - 4", "i32"), 6);
}

#[test]
fn interp_multiplication() {
    assert_execution_any_int!(main!("5 * 6", "i32"), 30);
}

#[test]
fn interp_division() {
    assert_execution_any_int!(main!("20 / 4", "i32"), 5);
}

#[test]
fn interp_modulo() {
    assert_execution_any_int!(main!("17 % 5", "i32"), 2);
}

#[test]
fn interp_complex_arithmetic() {
    assert_execution_any_int!(main!("10 + 5 * 2 - 8 / 2", "i32"), 16);
}

#[test]
fn interp_negative_numbers() {
    assert_execution_any_int!(main!("-5 + 10", "i32"), 5);
}

// Floats..

#[test]
fn interp_float_addition() {
    assert_execution_float!(main!("2.5 + 3.5", "f32"), 6.0);
}

#[test]
fn interp_float_division() {
    assert_execution_float!(main!("10.0 / 4.0", "f32"), 2.5);
}

#[test]
fn interp_mixed_arithmetic() {
    assert_execution_float!(main!("5 as f32 + 2.5", "f32"), 7.5);
}

// Variable bindings..

#[test]
fn interp_variable_binding() {
    assert_execution_any_int!(main!("let x = 42; x", "i32"), 42);
}

#[test]
fn interp_multiple_variables() {
    assert_execution_any_int!(main!("let x = 10; let y = 20; x + y", "i32"), 30);
}

#[test]
fn interp_variable_shadowing() {
    assert_execution_any_int!(main!("let x = 5; let x = 10; x", "i32"), 10);
}

// Comparisons..

#[test]
fn interp_equality() {
    assert_execution_bool!(main!("5 == 5", "bool"), true);
}

#[test]
fn interp_inequality() {
    assert_execution_bool!(main!("5 != 3", "bool"), true);
}

#[test]
fn interp_less_than() {
    assert_execution_bool!(main!("3 < 5", "bool"), true);
}

#[test]
fn interp_greater_than() {
    assert_execution_bool!(main!("5 > 3", "bool"), true);
}

#[test]
fn interp_less_than_or_equal() {
    assert_execution_bool!(main!("5 <= 5", "bool"), true);
}

#[test]
fn interp_greater_than_or_equal() {
    assert_execution_bool!(main!("5 >= 3", "bool"), true);
}

#[test]
fn interp_logical_and_true() {
    assert_execution_bool!(main!("true && true", "bool"), true);
}

#[test]
fn interp_logical_and_false() {
    assert_execution_bool!(main!("true && false", "bool"), false);
}

#[test]
fn interp_logical_or_true() {
    assert_execution_bool!(main!("false || true", "bool"), true);
}

#[test]
fn interp_logical_or_false() {
    assert_execution_bool!(main!("false || false", "bool"), false);
}

#[test]
fn interp_logical_not() {
    assert_execution_bool!(main!("!false", "bool"), true);
}

// If statements..

#[test]
fn interp_if_conditions() {
    assert_execution_any_int!(main!("if true { 21 } else { 42 }", "i32"), 21);
    assert_execution_any_int!(main!("if false { 21 } else { 42 }", "i32"), 42);
}

#[test]
fn interp_if_without_else() {
    assert_execution_any_int!(main!("if true { 42 }", "i32"), 42);
}

#[test]
fn interp_nested_if() {
    assert_execution_any_int!(
        main!(
            "
            if true {
                if false {
                    0
                } else {
                    10
                }
            } else {
                20
            }",
            "i32"
        ),
        10
    );
}

#[test]
fn interp_if_with_comparison() {
    assert_execution_any_int!(main!("if 5 > 3 { 100 } else { 50 }", "i32"), 100);
}

// Functions..

#[test]
fn interp_function_call() {
    let program = r#"
        fn add(a: i32, b: i32) -> i32 {
            a + b
        }
        
        fn main() -> i32 {
            add(3, 4)
        }
    "#;
    assert_execution_any_int!(program, 7);
}

#[test]
fn interp_function_with_local_variables() {
    let program = r#"
        fn multiply(x: i32, y: i32) -> i32 {
            let result = x * y;
            result
        }
        
        fn main() -> i32 {
            multiply(6, 7)
        }
    "#;
    assert_execution_any_int!(program, 42);
}

#[test]
fn interp_recursive_function() {
    let program = r#"
        fn factorial(n: i32) -> i32 {
            if n <= 1 {
                1
            } else {
                n * factorial(n - 1)
            }
        }
        
        fn main() -> i32 {
            factorial(5)
        }
    "#;
    assert_execution_any_int!(program, 120);
}

#[test]
fn interp_fibonacci() {
    let program = r#"
        fn fibonacci(n: i32) -> i32 {
            if n <= 1 {
                n
            } else {
                fibonacci(n - 1) + fibonacci(n - 2)
            }
        }
        
        fn main() -> i32 {
            fibonacci(6)
        }
    "#;
    assert_execution_any_int!(program, 8);
}

// Strings..

#[test]
fn interp_string_literal() {
    assert_execution_string!(main!(r#""Hello, World!""#, "string"), "Hello, World!");
}

#[test]
fn interp_string_concatenation() {
    assert_execution_string!(
        main!(r#""Hello" + ", " + "World!""#, "string"),
        "Hello, World!"
    );
}

#[test]
fn interp_string_len() {
    assert_execution_any_int!(main!(r#"let s = "Hello, World!"; s.len()"#, "usize"), 13);
    assert_execution_any_int!(main!(r#"let s = ""; s.len()"#, "usize"), 0);
}

#[test]
fn interp_string_empty() {
    assert_execution_bool!(
        main!(r#"let s = "Hello, World!"; s.is_empty()"#, "bool"),
        false
    );
    assert_execution_bool!(main!(r#"let s = ""; s.is_empty()"#, "bool"), true);
}

// Loops..

#[test]
fn interp_while_loop() {
    let program = main!(
        r#"
        let mut count = 0;
        let mut sum = 0;
        while count < 5 {
            sum = sum + count;
            count = count + 1;
        }
        sum
    "#,
        "i32"
    );
    assert_execution_any_int!(program, 10);
}

#[test]
fn interp_for_loop() {
    let program = main!(
        r#"
        let mut sum = 0;
        for i in 0..5 {
            sum = sum + i;
        }
        sum
    "#,
        "i32"
    );
    assert_execution_any_int!(program, 10);
}

#[test]
fn interp_for_loop_range_inclusive() {
    let program = main!(
        r#"
        let mut sum = 0;
        for i in 0..=4 {
            sum = sum + i;
        }
        sum
    "#,
        "i32"
    );
    assert_execution_any_int!(program, 10);
}

// Arrays..

#[test]
fn interp_array_creation() {
    let program = main!(
        r#"
        let arr = [1, 2, 3, 4, 5];
        arr[2]
    "#,
        "i32"
    );
    assert_execution_any_int!(program, 3);
}

#[test]
fn interp_array_indexing() {
    let program = main!(
        r#"
        let arr = [10, 20, 30, 40, 50];
        arr[2]
    "#,
        "i32"
    );
    assert_execution_any_int!(program, 30);

    let program = main!(
        r#"
        let arr = [10, 20, 30, 40, 50];
        arr[4]
    "#,
        "i32"
    );
    assert_execution_any_int!(program, 50);

    let program = main!(
        r#"
        let arr = [10, 20, 30, 40, 50];
        arr[0]
    "#,
        "i32"
    );
    assert_execution_any_int!(program, 10);
}

// TODO: Array iteration..

// Structs..

#[test]
fn interp_struct_creation() {
    let program = r#"
        struct Point { x: i32, y: i32 }
        
        fn main() -> i32 {
            let p = Point { x: 10, y: 20 };
            p.x
        }
        "#;
    assert_execution_any_int!(program, 10);
}

#[test]
fn interp_struct_field_access() {
    let program = r#"
        struct Point { x: i32, y: i32 }
        
        fn main() -> i32 {
            let p = Point { x: 5, y: 15 };
            p.x + p.y
        }
    "#;
    assert_execution_any_int!(program, 20);
}

#[test]
fn interp_struct_method() {
    let program = r#"
        struct Circle { radius: f32 }
        
        impl Circle {
            fn area(self) -> f32 {
                3.14159 * self.radius * self.radius
            }
        }
        
        fn main() -> f32 {
            let c = Circle { radius: 5.0 };
            c.area()
        }
    "#;
    assert_execution_float!(program, 78.53975);
}

// Casts..

#[test]
fn interp_int_to_float_cast() {
    assert_execution_float!(main!("5 as f32", "f32"), 5.0);
}

#[test]
fn interp_float_to_int_cast() {
    assert_execution_any_int!(main!("5.7 as i32", "i32"), 5);
}

// Edge case like tests..

#[test]
fn interp_nested_function_calls() {
    let program = r#"
        fn add(a: i32, b: i32) -> i32 { a + b }
        
        fn multiply(a: i32, b: i32) -> i32 { a * b }
        
        fn main() -> i32 {
            multiply(add(2, 3), add(4, 5))
        }
    "#;
    assert_execution_any_int!(program, 45); // multiply(5, 9) = 45
}

#[test]
fn interp_multiple_if_conditions() {
    let program = main!(
        r#"
        let x = 15;
        if x < 10 {
            1
        } else if x < 20 {
            2
        } else {
            3
        }
    "#,
        "i32"
    );
    assert_execution_any_int!(program, 2);
}

#[test]
fn interp_mixed_types_in_expression() {
    let program = main!(
        r#"
        let x = 10;
        let y = 20;
        let z = 5.5;
        (x + y) as f32 + z
    "#,
        "f32"
    );
    assert_execution_float!(program, 35.5);
}

#[test]
fn interp_empty_function_returns_unit() {
    let program = r#"
        fn noop() { }
        
        fn main() {
            noop()
        }
    "#;
    assert_execution_result!(program, Value::Unit);
}

#[test]
fn interp_break_statement() {
    let program = main!(
        r#"
        let mut sum = 0;
        for i in 0..100 {
            if i == 10 {
                break;
            }
            sum = sum + i;
        }
        sum
    "#,
        "i32"
    );
    assert_execution_any_int!(program, 45); // 0+1+2+...+9 = 45
}

#[test]
fn interp_continue_statement() {
    let program = main!(
        r#"
        let mut sum = 0;
        for i in 0..10 {
            if i % 2 == 0 {
                continue;
            }
            sum = sum + i;
        }
        sum
    "#,
        "i32"
    );
    assert_execution_any_int!(program, 25); // 1+3+5+7+9 = 25
}

#[test]
fn interp_tuple_creation() {
    let program = main!(
        r#"
        let t = (5, 10, 15);
        t.0 + t.1 + t.2
    "#,
        "i32"
    );
    assert_execution_any_int!(program, 30);
}

#[test]
fn interp_tuple_destructuring() {
    let program = main!(
        r#"
        let (a, b, c) = (1, 2, 3);
        a + b + c
    "#,
        "i32"
    );
    assert_execution_any_int!(program, 6);
}

// Standard library tests..

#[test]
fn interp_stdlib_to_string() {
    assert_execution_string!(main!("(42 as i8).to_string()", "string"), "42");
    assert_execution_string!(main!("(42 as i16).to_string()", "string"), "42");
    assert_execution_string!(main!("(42 as i32).to_string()", "string"), "42");
    assert_execution_string!(main!("(42 as i64).to_string()", "string"), "42");
    assert_execution_string!(main!("(42 as i128).to_string()", "string"), "42");
    assert_execution_string!(main!("(42 as isize).to_string()", "string"), "42");

    assert_execution_string!(main!("(42 as u8).to_string()", "string"), "42");
    assert_execution_string!(main!("(42 as u16).to_string()", "string"), "42");
    assert_execution_string!(main!("(42 as u32).to_string()", "string"), "42");
    assert_execution_string!(main!("(42 as u64).to_string()", "string"), "42");
    assert_execution_string!(main!("(42 as u128).to_string()", "string"), "42");
    assert_execution_string!(main!("(42 as usize).to_string()", "string"), "42");

    assert_execution_string!(
        main!("(3.05 as f32).to_string()", "string"),
        "3.049999952316284"
    );
    assert_execution_string!(main!("(3.05 as f64).to_string()", "string"), "3.05");

    assert_execution_string!(main!("true.to_string()", "string"), "true");

    assert_execution_string!(main!("let x = {}; x.to_string()", "string"), "()");

    assert_execution_string!(main!(r#""hello".to_string()"#, "string"), "hello");
}

macro_rules! test_from_string {
    ($ty:literal, $valid_src:literal, $invalid_src:literal, $expected:expr) => {
        assert_execution_result!(
            concat!(
                "fn main() -> ",
                $ty,
                "{ let (val, err) = ",
                $ty,
                r#"::from_string(""#,
                $valid_src,
                r#""); val }"#
            ),
            $expected
        );
        assert_execution_bool!(
            concat!(
                "fn main() -> bool { let (val, err) = ",
                $ty,
                r#"::from_string(""#,
                $valid_src,
                r#""); err }"#
            ),
            false
        );
        assert_execution_bool!(
            concat!(
                "fn main() -> bool { let (val, err) = ",
                $ty,
                r#"::from_string(""#,
                $invalid_src,
                r#""); err }"#
            ),
            true
        );
    };
}

#[test]
fn interp_stdlib_from_string() {
    test_from_string!("u8", "42", "invalid", Value::UnsignedInteger(42));
    test_from_string!("u16", "42", "invalid", Value::UnsignedInteger(42));
    test_from_string!("u32", "42", "invalid", Value::UnsignedInteger(42));
    test_from_string!("u64", "42", "invalid", Value::UnsignedInteger(42));
    test_from_string!("u128", "42", "invalid", Value::UnsignedInteger(42));
    test_from_string!("usize", "42", "invalid", Value::UnsignedInteger(42));

    test_from_string!("i8", "-42", "invalid", Value::Integer(-42));
    test_from_string!("i16", "-42", "invalid", Value::Integer(-42));
    test_from_string!("i32", "-42", "invalid", Value::Integer(-42));
    test_from_string!("i64", "42", "invalid", Value::Integer(42));
    test_from_string!("i128", "42", "invalid", Value::Integer(42));
    test_from_string!("isize", "42", "invalid", Value::Integer(42));

    test_from_string!("f32", "3.05", "invalid", Value::Float(3.049999952316284));
    test_from_string!("f64", "3.05", "invalid", Value::Float(3.05));

    test_from_string!("bool", "true", "invalid", Value::Boolean(true));
    test_from_string!("bool", "false", "invalid", Value::Boolean(false));
}

#[test]
fn interp_stdlib_sqrt() {
    assert_execution_float!(main!("(16.0 as f32).sqrt()", "f32"), 4.0);
    assert_execution_float!(main!("(20.25 as f64).sqrt()", "f64"), 4.5);

    assert_execution_any_int!(main!("(25 as i8).sqrt()", "i8"), 5);
    assert_execution_any_int!(main!("(25 as i16).sqrt()", "i16"), 5);
    assert_execution_any_int!(main!("(25 as i32).sqrt()", "i32"), 5);
    assert_execution_any_int!(main!("(25 as i64).sqrt()", "i64"), 5);
    assert_execution_any_int!(main!("(25 as i128).sqrt()", "i128"), 5);
    assert_execution_any_int!(main!("(25 as isize).sqrt()", "isize"), 5);

    assert_execution_any_int!(main!("(25 as u8).sqrt()", "u8"), 5);
    assert_execution_any_int!(main!("(25 as u16).sqrt()", "u16"), 5);
    assert_execution_any_int!(main!("(25 as u32).sqrt()", "u32"), 5);
    assert_execution_any_int!(main!("(25 as u64).sqrt()", "u64"), 5);
    assert_execution_any_int!(main!("(25 as u128).sqrt()", "u128"), 5);
    assert_execution_any_int!(main!("(25 as usize).sqrt()", "usize"), 5);
}

#[test]
fn interp_stdlib_abs() {
    assert_execution_float!(main!("(-16.0 as f32).abs()", "f32"), 16.0);
    assert_execution_float!(main!("(-20.25 as f64).abs()", "f64"), 20.25);

    assert_execution_any_int!(main!("(-25 as i8).abs()", "i8"), 25);
    assert_execution_any_int!(main!("(-25 as i16).abs()", "i16"), 25);
    assert_execution_any_int!(main!("(-25 as i32).abs()", "i32"), 25);
    assert_execution_any_int!(main!("(-25 as i64).abs()", "i64"), 25);
    assert_execution_any_int!(main!("(-25 as i128).abs()", "i128"), 25);
    assert_execution_any_int!(main!("(-25 as isize).abs()", "isize"), 25);
}

// Storage of printed output for verification in tests.
static TEST_OUTPUT: Mutex<Vec<String>> = Mutex::new(Vec::new());

#[kitlang_native_fn]
fn println(s: String) {
    if let Ok(mut output) = TEST_OUTPUT.lock() {
        output.push(s.to_string());
    }
}

fn clear_test_output() {
    if let Ok(mut output) = TEST_OUTPUT.lock() {
        output.clear();
    }
}

fn get_test_output() -> Vec<String> {
    if let Ok(output) = TEST_OUTPUT.lock() {
        output.clone()
    } else {
        Vec::new()
    }
}

#[test]
fn interp_site_example() {
    const PROGRAM_SRC: &str = include_str!("./programs/modified_site_example.purr");

    clear_test_output();

    execute_code!(no_io, PROGRAM_SRC, |i| {
        register_native_fn!(i, println);
    })
    .expect("Site example program should execute without errors");

    assert_eq!(
        get_test_output(),
        vec![
            "[Adventure] Launching quest line",
            "[Adventure] Running warm-up iterations",
            "[Adventure]  Warm-up round 1",
            "[Adventure]  Warm-up round 2",
            "[Adventure]  Warm-up round 3",
            "[Adventure] Drilling fundamentals",
            "[Adventure] Attempting: Training Yard",
            " + gained experience 2",
            "[Adventure] Energy remaining: 3",
            "[Adventure] Exploring the Iterable Ruins",
            "[Adventure] Attempting: Iterable Ruins",
            " + gained experience 3",
            "[Adventure] Energy remaining: 2",
            "[Adventure] Facing the Compiler Guardian",
            "[Adventure] Attempting: Compiler Guardian",
            " + gained experience 5",
            "[Adventure] Hero is out of energy, relying on experience gains.",
            "[Tracker] Adventure complete",
            "[Tracker] Report generated",
            "Hero { name: Compiler Cat, energy: 0, experience: 10 }",
            "Recorded 2 events."
        ]
    );
}

use std::cell::RefCell;

use crate::intermediate::types::KitTy;
use crate::interpreter::mir_interpreter::{Interpreter, Value};
use crate::register_native_fn;
use kitlang_macros::kitlang_native_fn;
use wasm_bindgen::prelude::*;

#[cfg(feature = "logging")]
use log::error;

thread_local! {
    static PRINT_CALLBACK: RefCell<Option<js_sys::Function>> = const { RefCell::new(None) };
    static INPUT: RefCell<Option<js_sys::Function>> = const { RefCell::new(None) };

    static NATIVE_FNS: RefCell<Vec<(String, js_sys::Function, String)>> = const { RefCell::new(Vec::new()) };
}

/// Adds a native function to be available in the interpreter.
/// * `name` is the name of the function to be used in kitlang code.
/// * `func` is the JavaScript function to be called.
/// * `ret_val` is the kitlang return type of the function as a string (e.g., "i32", "string", etc.)
#[wasm_bindgen]
pub fn add_native_function(name: &str, func: js_sys::Function, ret_val: &str) {
    NATIVE_FNS.with(|slot| {
        slot.borrow_mut()
            .push((name.to_string(), func, ret_val.to_string()));
    });
}

/// Clears all registered native functions bindings.
#[wasm_bindgen]
pub fn clear_native_functions() {
    NATIVE_FNS.with(|slot| {
        slot.borrow_mut().clear();
    });
}

/// Sets the print callback function to be used by the `print` and `println` native functions.
/// * `cb` is the JavaScript function to be called for printing.
#[wasm_bindgen]
pub fn set_print_callback(cb: js_sys::Function) {
    PRINT_CALLBACK.with(|slot| {
        *slot.borrow_mut() = Some(cb);
    });
}

/// Sets the input callback function to be used by the `input` native function.
/// * `cb` is the JavaScript function to be called for input.
/// # Note
/// This function should dispatch both the case where the input has no default value, and the case where it does.
/// I.e. This function is responsible for handling both `input(prompt: string)` and `input_placeholder(prompt: string, default: string)`.
#[wasm_bindgen]
pub fn set_input_callback(cb: js_sys::Function) {
    INPUT.with(|slot| {
        *slot.borrow_mut() = Some(cb);
    });
}

fn add_native_functions(interpreter: &mut Interpreter) {
    register_native_functions(interpreter);

    NATIVE_FNS.with(|slot| {
        for (name, func, ret_val) in slot.borrow().iter() {
            let js_func = func.clone();
            let ret_val_c = ret_val.clone();
            interpreter.register_native_function(name, move |args: &[Value]| {
                let js_args = args.iter().map(to_js_value).collect::<js_sys::Array>();
                match js_func.call1(&JsValue::NULL, &JsValue::from(js_args)) {
                    Ok(result) => {
                        if let Some(return_value_type) = KitTy::from_primitive_ty_str(&ret_val_c) {
                            Some(from_js_value_expected(&result, return_value_type))
                        } else {
                            Some(from_js_value(&result))
                        }
                    }
                    Err(e) => {
                        error!("Error calling native JS function: {e:?}");
                        None
                    }
                }
            });
        }
    });
}

fn internal_execute_source_string(source: &str, time_execution: bool) -> Result<JsValue, JsValue> {
    match crate::prelude::execute_source_string(source, register_native_functions, time_execution) {
        Ok(value) => serde_json::to_string(&value)
            .map_err(|e| JsValue::from_str(&e.to_string()))
            .map(|s| JsValue::from_str(&s)),
        Err(err) => Err(JsValue::from_str(&err.format_as_error_message(source))),
    }
}

/// Parses and executes the given Kitlang source code string, returning the result as a [`JsValue`].
/// # Returns
/// * `Ok(JsValue)` - The result of the execution as a [`JsValue`].
/// * `Err(JsValue)` - An error message as a [`JsValue`] `string` if execution failed.
/// # Errors
/// If there was an error during parsing or execution, the error message is formatted and converted to a javascript string.
#[wasm_bindgen]
pub fn wasm_execute_source_string(source: &str, time_execution: bool) -> Result<JsValue, JsValue> {
    internal_execute_source_string(source, time_execution)
}

/// Parses and executes the given Kitlang source code string, with registered native functions,
/// returning the result as a [`JsValue`].
/// # Returns
/// * `Ok(JsValue)` - The result of the execution as a [`JsValue`].
/// * `Err(JsValue)` - An error message as a [`JsValue`] `string` if execution failed.
/// # Errors
/// If there was an error during parsing or execution, the error message is formatted and converted to a javascript string.
#[wasm_bindgen]
pub fn wasm_execute_source_string_with_native_fns(
    source: &str,
    time_execution: bool,
) -> Result<JsValue, JsValue> {
    match crate::prelude::execute_source_string(source, add_native_functions, time_execution) {
        Ok(value) => serde_json::to_string(&value)
            .map_err(|e| JsValue::from_str(&e.to_string()))
            .map(|s| JsValue::from_str(&s)),
        Err(err) => Err(JsValue::from_str(&err.format_as_error_message(source))),
    }
}

/// Initializes logging for the WebAssembly module, to ensure logs are printed to the browser console.
#[wasm_bindgen]
pub fn init_logging() {
    #[cfg(feature = "logging")]
    fn setup_logging() {
        console_log::init_with_level(log::Level::Debug).expect("Failed to initialize logging");
    }
    #[cfg(feature = "logging")]
    setup_logging();
}

fn register_native_functions(interpreter: &mut Interpreter) {
    register_native_fn!(
        interpreter,
        print,
        println,
        input,
        input_placeholder,
        read_line
    );
}

// Value conversions

/// Converts a [`Value`] into a [`JsValue`].
fn to_js_value(value: &Value) -> JsValue {
    #[allow(clippy::cast_precision_loss)]
    match value {
        Value::Integer(i) => JsValue::from_f64(*i as f64),
        Value::UnsignedInteger(i) => JsValue::from_f64(*i as f64),
        Value::Float(f) => JsValue::from_f64(*f),
        Value::String(s) => JsValue::from_str(s),
        Value::Boolean(b) => JsValue::from_bool(*b),
        Value::Unit => JsValue::NULL,
        _ => JsValue::UNDEFINED,
    }
}

/// Converts a [`JsValue`] into a [`Value`], with expected types.
fn from_js_value_expected(value: &JsValue, expected: KitTy) -> Value {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    match expected {
        KitTy::Int(..) => {
            if let Some(f) = value.as_f64() {
                return Value::Integer(f as i64);
            }
        }
        KitTy::UInt(..) => {
            if let Some(f) = value.as_f64() {
                return Value::UnsignedInteger(f as u64);
            }
        }
        KitTy::Float(..) => {
            if let Some(f) = value.as_f64() {
                return Value::Float(f);
            }
        }
        KitTy::Boolean => {
            if let Some(b) = value.as_bool() {
                return Value::Boolean(b);
            }
        }
        KitTy::String => {
            if let Some(s) = value.as_string() {
                return Value::String(s);
            }
        }
        _ => {}
    }

    Value::Unit
}

/// Converts a [`JsValue`] into a [`Value`].
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn from_js_value(value: &JsValue) -> Value {
    if value.is_string()
        && let Some(s) = value.as_string()
    {
        return Value::String(s);
    }

    if value.is_instance_of::<js_sys::Number>()
        && let Some(f) = value.as_f64()
    {
        return if f.fract() == 0.0 {
            Value::Integer(f as i64)
        } else {
            Value::Float(f)
        };
    }

    if value.is_instance_of::<js_sys::Boolean>() {
        value
            .as_bool()
            .map_or(Value::Boolean(false), Value::Boolean)
    } else {
        Value::Unit
    }
}

// Native functions for interacting with the browser.

#[kitlang_native_fn]
fn print(s: String) {
    PRINT_CALLBACK.with(|slot| {
        if let Some(cb) = &*slot.borrow() {
            cb.call2(&JsValue::NULL, &JsValue::from_str(&s), &JsValue::FALSE)
                .unwrap();
        }
    });
}
#[kitlang_native_fn]
fn println(s: String) {
    PRINT_CALLBACK.with(|slot| {
        if let Some(cb) = &*slot.borrow() {
            cb.call2(&JsValue::NULL, &JsValue::from_str(&s), &JsValue::TRUE)
                .unwrap();
        }
    });
}
#[kitlang_native_fn]
fn read_line() -> String {
    let mut result = String::new();
    INPUT.with(|slot| {
        if let Some(cb) = &*slot.borrow()
            && let Ok(js_value) = cb.call2(&JsValue::NULL, &JsValue::NULL, &JsValue::NULL)
            && let Some(s) = js_value.as_string()
        {
            result = s;
        }
    });
    result
}
#[kitlang_native_fn]
fn input(s: String) -> String {
    let mut result = String::new();
    INPUT.with(|slot| {
        if let Some(cb) = &*slot.borrow()
            && let Ok(js_value) = cb.call2(&JsValue::NULL, &JsValue::from_str(&s), &JsValue::NULL)
            && let Some(s) = js_value.as_string()
        {
            result = s;
        }
    });
    result
}
#[kitlang_native_fn]
fn input_placeholder(s: String, s2: String) -> String {
    let mut result = String::new();
    INPUT.with(|slot| {
        if let Some(cb) = &*slot.borrow()
            && let Ok(js_value) = cb.call2(
                &JsValue::NULL,
                &JsValue::from_str(&s),
                &JsValue::from_str(&s2),
            )
            && let Some(s) = js_value.as_string()
        {
            result = s;
        }
    });
    result
}

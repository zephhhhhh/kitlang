use std::cell::RefCell;

use crate::intermediate::types::KitTy;
use crate::interpreter::mir_interpreter::{Interpreter, Value};
use crate::register_native_fn;
use kitlang_macros::kitlang_native_fn;
use wasm_bindgen::prelude::*;

#[cfg(feature = "logging")]
use log::*;

thread_local! {
    static PRINT_CALLBACK: RefCell<Option<js_sys::Function>> = const { RefCell::new(None) };
    static INPUT: RefCell<Option<js_sys::Function>> = const { RefCell::new(None) };

    static NATIVE_FNS: RefCell<Vec<(String, js_sys::Function, String)>> = const { RefCell::new(Vec::new()) };
}

#[wasm_bindgen]
pub fn add_native_function(name: &str, func: js_sys::Function, ret_val: &str) {
    NATIVE_FNS.with(|slot| {
        slot.borrow_mut()
            .push((name.to_string(), func, ret_val.to_string()));
    });
}

#[wasm_bindgen]
pub fn clear_native_functions() {
    NATIVE_FNS.with(|slot| {
        slot.borrow_mut().clear();
    });
}

#[wasm_bindgen]
pub fn set_print_callback(cb: js_sys::Function) {
    PRINT_CALLBACK.with(|slot| {
        *slot.borrow_mut() = Some(cb);
    });
}

#[wasm_bindgen]
pub fn set_input_callback(cb: js_sys::Function) {
    INPUT.with(|slot| {
        *slot.borrow_mut() = Some(cb);
    });
}

pub fn return_value_type(ret_val: &str) -> Option<KitTy> {
    KitTy::from_primitive_ty_str(ret_val)
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
                        error!("Error calling native JS function: {:?}", e);
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

#[wasm_bindgen]
pub fn wasm_execute_source_string(source: &str, time_execution: bool) -> Result<JsValue, JsValue> {
    internal_execute_source_string(source, time_execution)
}

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
    register_native_fn!(interpreter, print, println, input, input_placeholder);
}

// Value conversions

/// Converts a [`Value`] into a [`JsValue`].
fn to_js_value(value: &Value) -> JsValue {
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
fn from_js_value(value: &JsValue) -> Value {
    if value.is_string()
        && let Some(s) = value.as_string()
    {
        return Value::String(s);
    }

    if value.is_instance_of::<js_sys::Number>()
        && let Some(f) = value.as_f64()
    {
        if f.fract() == 0.0 {
            return Value::Integer(f as i64);
        } else {
            return Value::Float(f);
        }
    }

    if value.is_instance_of::<js_sys::Boolean>() {
        return value
            .as_bool()
            .map(Value::Boolean)
            .unwrap_or(Value::Boolean(false));
    }

    Value::Unit
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

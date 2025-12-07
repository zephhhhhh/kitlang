use std::cell::RefCell;

use crate::interpreter::mir_interpreter::{Interpreter, Value};
use crate::register_native_fn;
use wasm_bindgen::prelude::*;

#[cfg(feature = "logging")]
use log::*;

thread_local! {
    static PRINT_CALLBACK: RefCell<Option<js_sys::Function>> = const { RefCell::new(None) };
    static INPUT: RefCell<Option<js_sys::Function>> = const { RefCell::new(None) };

    static NATIVE_FNS: RefCell<Vec<(String, js_sys::Function)>> = const { RefCell::new(Vec::new()) };
}

#[wasm_bindgen]
pub fn add_native_function(name: &str, func: js_sys::Function) {
    NATIVE_FNS.with(|slot| {
        slot.borrow_mut().push((name.to_string(), func));
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

fn add_native_functions(interpreter: &mut Interpreter) {
    register_native_functions(interpreter);

    NATIVE_FNS.with(|slot| {
        for (name, func) in slot.borrow().iter() {
            let js_func = func.clone();
            interpreter.register_native_function(name, move |args: &[Value]| {
                let js_args = args.iter().map(to_js_value).collect::<js_sys::Array>();
                match js_func.call1(&JsValue::NULL, &JsValue::from(js_args)) {
                    Ok(result) => {
                        info!("Native JS function returned: {:?}", result);
                        Some(from_js_value(&result))
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
    register_native_fn!(
        interpreter,
        println,
        input,
        input_placeholder,
        to_lower,
        is_empty,
        i32_to_string,
        string_to_i32,
        f32_to_string,
        string_to_f32
    );
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

fn expect_string_value(value: &Value) -> String {
    if let Value::String(s) = value {
        s.clone()
    } else {
        String::new()
    }
}

// "Compiler" defined intrinsics.

fn i32_to_string(a: &[Value]) -> String {
    if let Some(Value::Integer(i)) = a.first() {
        return i.to_string();
    }
    String::new()
}

fn string_to_i32(a: &[Value]) -> i32 {
    if let Some(Value::String(s)) = a.first()
        && let Ok(i) = s.parse::<i32>()
    {
        return i;
    }
    0
}

fn f32_to_string(a: &[Value]) -> String {
    if let Some(Value::Float(f)) = a.first() {
        return f.to_string();
    }
    String::new()
}

fn string_to_f32(a: &[Value]) -> f32 {
    if let Some(Value::String(s)) = a.first()
        && let Ok(f) = s.parse::<f32>()
    {
        return f;
    }
    0.0
}

fn is_empty(a: &[Value]) -> bool {
    if let Some(Value::String(s)) = a.first() {
        return s.is_empty();
    }
    true
}

fn println(a: &[Value]) {
    if let Some(v) = a.first() {
        let string = v.repr_string();
        PRINT_CALLBACK.with(|slot| {
            if let Some(cb) = &*slot.borrow() {
                cb.call1(&JsValue::NULL, &JsValue::from_str(&string))
                    .unwrap();
            }
        });
    }
}

fn to_lower(a: &[Value]) -> String {
    if let Some(Value::String(s)) = a.first() {
        return s.to_lowercase();
    }
    "".to_string()
}

fn input(a: &[Value]) -> String {
    if a.len() != 1 {
        return String::new();
    }

    let mut result = String::new();
    INPUT.with(|slot| {
        if let Some(cb) = &*slot.borrow() {
            let input_arg = expect_string_value(&a[0]);
            if let Ok(js_value) = cb.call2(
                &JsValue::NULL,
                &JsValue::from_str(&input_arg),
                &JsValue::NULL,
            ) && let Some(s) = js_value.as_string()
            {
                result = s;
            }
        }
    });
    result
}

fn input_placeholder(a: &[Value]) -> String {
    if a.len() != 2 {
        return String::new();
    }

    let mut result = String::new();
    INPUT.with(|slot| {
        if let Some(cb) = &*slot.borrow() {
            let input_arg = expect_string_value(&a[0]);
            let default_arg = expect_string_value(&a[1]);
            if let Ok(js_value) = cb.call2(
                &JsValue::NULL,
                &JsValue::from_str(&input_arg),
                &JsValue::from_str(&default_arg),
            ) && let Some(s) = js_value.as_string()
            {
                result = s;
            }
        }
    });
    result
}

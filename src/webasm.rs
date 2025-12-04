use std::cell::RefCell;

use crate::interpreter::mir_interpreter::{Interpreter, Value};
use crate::register_native_fn;
use wasm_bindgen::prelude::*;

thread_local! {
    static PRINT_CALLBACK: RefCell<Option<js_sys::Function>> = const { RefCell::new(None) };
    static INPUT: RefCell<Option<js_sys::Function>> = const { RefCell::new(None) };

    static NATIVE_FNS: RefCell<Vec<(String, js_sys::Function)>> = const { RefCell::new(Vec::new()) };
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
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
                log("Called alert");
                let js_args = js_sys::Array::new();
                for arg in args {
                    let js_value = match arg {
                        Value::Integer(i) => JsValue::from_f64(*i as f64),
                        Value::String(s) => JsValue::from_str(s),
                        _ => JsValue::NULL,
                    };
                    js_args.push(&js_value);
                }
                match js_func.call1(&JsValue::NULL, &JsValue::from(js_args)) {
                    Ok(result) => {
                        log(&format!("Native function returned: {:?}", result));
                        if let Some(s) = result.as_string() {
                            Some(Value::String(s))
                        } else if let Some(f) = result.as_f64() {
                            Some(Value::Integer(f as i64))
                        } else {
                            Some(Value::Unit)
                        }
                    }
                    Err(_) => None,
                }
            });
        }
    });
}

fn internal_execute_source_string(source: &str) -> Result<JsValue, JsValue> {
    match crate::prelude::execute_source_string(source, register_native_functions, false) {
        Ok(value) => serde_json::to_string(&value)
            .map_err(|e| JsValue::from_str(&e.to_string()))
            .map(|s| JsValue::from_str(&s)),
        Err(err) => Err(JsValue::from_str(&err.format_as_error_message(source))),
    }
}

#[wasm_bindgen]
pub fn wasm_execute_source_string(source: &str) -> Result<JsValue, JsValue> {
    internal_execute_source_string(source)
}

#[wasm_bindgen]
pub fn wasm_execute_source_string_with_native_fns(source: &str) -> Result<JsValue, JsValue> {
    match crate::prelude::execute_source_string(source, add_native_functions, false) {
        Ok(value) => serde_json::to_string(&value)
            .map_err(|e| JsValue::from_str(&e.to_string()))
            .map(|s| JsValue::from_str(&s)),
        Err(err) => Err(JsValue::from_str(&err.format_as_error_message(source))),
    }
}

fn register_native_functions(interpreter: &mut Interpreter) {
    register_native_fn!(
        interpreter,
        println,
        input,
        input_placeholder,
        to_lower,
        is_empty,
        int_to_string,
        string_to_int
    );
}

// "Compiler" defined intrinsics.

fn int_to_string(a: &[Value]) -> String {
    if let Some(Value::Integer(i)) = a.first() {
        return i.to_string();
    }
    String::new()
}

fn string_to_int(a: &[Value]) -> i32 {
    if let Some(Value::String(s)) = a.first()
        && let Ok(i) = s.parse::<i32>()
    {
        return i;
    }
    0
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
    let mut result = String::new();
    INPUT.with(|slot| {
        if let Some(cb) = &*slot.borrow() {
            let input_arg = if let Some(Value::String(s)) = a.first() {
                s.clone()
            } else {
                String::new()
            };
            if let Ok(js_value) = cb.call2(
                &JsValue::NULL,
                &JsValue::from_str(&input_arg),
                &JsValue::NULL,
            ) {
                if let Some(s) = js_value.as_string() {
                    result = s;
                }
            }
        }
    });
    result
}

fn input_placeholder(a: &[Value]) -> String {
    let mut result = String::new();
    INPUT.with(|slot| {
        if let Some(cb) = &*slot.borrow() {
            let input_arg = if let Some(Value::String(s)) = a.first() {
                s.clone()
            } else {
                String::new()
            };
            let placeholder_arg = if let Some(Value::String(s)) = a.get(1) {
                s.clone()
            } else {
                String::new()
            };
            if let Ok(js_value) = cb.call2(
                &JsValue::NULL,
                &JsValue::from_str(&input_arg),
                &JsValue::from_str(&placeholder_arg),
            ) {
                if let Some(s) = js_value.as_string() {
                    result = s;
                }
            }
        }
    });
    result
}

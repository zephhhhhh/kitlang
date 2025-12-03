use std::cell::RefCell;

use crate::interpreter::mir_interpreter::{Interpreter, InterpreterState, Value};
use crate::register_native_fn;
use serde::de;
use wasm_bindgen::prelude::*;

thread_local! {
    static CALLBACK: RefCell<Option<js_sys::Function>> = const { RefCell::new(None) };
}

#[wasm_bindgen]
pub fn set_print_callback(cb: js_sys::Function) {
    CALLBACK.with(|slot| {
        *slot.borrow_mut() = Some(cb);
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
    // TODO: DEAR LORD FIX THIS
    const HACK_INJECT_DEFINITIONS: bool = true;

    if HACK_INJECT_DEFINITIONS {
        let definitions = r#"
            native fn to_lower(s: string) -> string;
            native fn println(s: string);
            native fn is_empty(s: string) -> bool;
            native fn int_to_string(i: i32) -> string;

        "#;

        let final_str = source.to_string() + definitions;

        internal_execute_source_string(&final_str)
    } else {
        internal_execute_source_string(source)
    }
}

fn register_native_functions(interpreter: &mut Interpreter) {
    register_native_fn!(interpreter, println, to_lower, is_empty, int_to_string);
}

// "Compiler" defined intrinsics.

fn int_to_string(_s: &mut InterpreterState, a: &[Value]) -> String {
    if let Some(Value::Integer(i)) = a.first() {
        return i.to_string();
    }
    String::new()
}

fn is_empty(_s: &mut InterpreterState, a: &[Value]) -> bool {
    if let Some(Value::String(s)) = a.first() {
        return s.is_empty();
    }
    true
}

fn println(_s: &mut InterpreterState, a: &[Value]) {
    if let Some(v) = a.first() {
        let string = v.repr_string();
        CALLBACK.with(|slot| {
            if let Some(cb) = &*slot.borrow() {
                cb.call1(&JsValue::NULL, &JsValue::from_str(&string))
                    .unwrap();
            }
        });
    }
}

fn to_lower(_s: &mut InterpreterState, a: &[Value]) -> String {
    if let Some(Value::String(s)) = a.first() {
        return s.to_lowercase();
    }
    "".to_string()
}

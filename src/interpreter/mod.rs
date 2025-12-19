//! Contains the HIR interpreter, as well as implementations and management of native functions for binding.
//!
//! This module is mostly temporary and will be eventually replaced with a more robust and faster either bytecode or sandboxed
//! machine code based executor.

pub mod mir_interpreter;
pub mod native_functions;

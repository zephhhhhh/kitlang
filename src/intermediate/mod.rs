//! Houses the different levels of IR used in the compiler's architecture, as well as resolution and type checking.
//!
//! The different levels are:
//! * High level IR (HIR): A more abstract representation that closely resembles the source code [AST](`crate::ast::ASTRoot`)
//!   representation, but in a more 'flattened' and analyzable form, where each expression, function, statement etc,
//!   is easily indexable with a unique ID.
//! * Middle level IR (MIR): A lower-level representation that is closer to machine code, focusing on control flow and data flow. This
//!   form is essentially comprised of just data operations, jumps, function calls and assignments.

pub mod hir;
pub mod mir;
pub mod resolver;
pub mod type_check;
pub mod types;

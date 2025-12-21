//! Public re-exports of commonly used items from the Kitlang compiler crate.

pub use crate::lexer::{tokenise, tokenise_stripped};
pub use crate::token::{Keyword, Punctuation, Token, TokenKind};

pub use crate::compiler::Compiler;
pub use crate::intermediate::hir::ProgramMetaData;
pub use crate::intermediate::mir::MIR;
pub use crate::register_native_fn;
pub use crate::{KitlangError, KitlangResult, execute_source_string, parse_source_string_to_mir};

#[cfg(feature = "logging")]
pub use log::*;

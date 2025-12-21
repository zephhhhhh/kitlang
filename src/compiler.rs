//! Provides an interface for interacting with the Kitlang compiler and toolchain.
//!
//! This module provides the main [`Compiler`] struct and provides a shared context for the different stages to use
//! during the parsing and lowering process.
//!
//! # Example
//!
//! ```
//! let mut compiler = Compiler::new("fn main() -> i32 { 1 }");
//! let mir = compiler.compile_to_mir()?;
//! ```

use std::cell::RefCell;

use string_interner::DefaultStringInterner;

use crate::KitlangResult;
use crate::ast::SourceSpan;
use crate::intermediate::hir::ProgramMetaData;
use crate::intermediate::resolver::{Namespace, TypeRegistry};
use crate::intermediate::type_check::TypeMap;

use crate::token::Token;

// String interning..
pub type IdentSymbol = string_interner::symbol::SymbolU32;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SpannedSymbol {
    pub symbol: IdentSymbol,
    pub span: SourceSpan,
}

impl SpannedSymbol {
    #[inline]
    #[must_use]
    pub fn new(symbol: IdentSymbol, span: SourceSpan) -> Self {
        Self { symbol, span }
    }
}

impl From<&SpannedSymbol> for IdentSymbol {
    #[inline]
    fn from(val: &SpannedSymbol) -> Self {
        val.symbol
    }
}

impl From<SpannedSymbol> for IdentSymbol {
    #[inline]
    fn from(val: SpannedSymbol) -> Self {
        val.symbol
    }
}

pub trait SymbolExt {
    fn resolve_str(&self, context: &CompilerContext) -> String;
}

impl SymbolExt for IdentSymbol {
    #[inline]
    fn resolve_str(&self, context: &CompilerContext) -> String {
        context.resolve_sym(*self)
    }
}

impl SymbolExt for SpannedSymbol {
    #[inline]
    fn resolve_str(&self, context: &CompilerContext) -> String {
        context.resolve_sym(self.symbol)
    }
}


// Compiler context stuff..

/// A context structure for the compiler, holds state relevant to the compilation process.
#[derive(Debug)]
pub struct CompilerContext {
    /// The source code being compiled.
    source: String,
    /// Metadata about the program being compiled.
    pub meta: ProgramMetaData,
    /// Whether to compile without the standard library.
    pub no_stdlib: bool,
    /// String interner for the compiler.
    pub intern: RefCell<DefaultStringInterner>,
}

impl CompilerContext {
    /// Create a new compiler context with the given source code and metadata.
    #[inline]
    #[must_use]
    pub fn new(source: String) -> Self {
        Self {
            source,
            meta: ProgramMetaData::default(),
            no_stdlib: false,
            intern: RefCell::new(DefaultStringInterner::new()),
        }
    }
}

// Helper methods for accessing state.
impl CompilerContext {
    /// Get a reference to the program's meta data.
    #[inline]
    #[must_use]
    pub fn meta(&self) -> &ProgramMetaData {
        &self.meta
    }

    /// Get a mutable reference to the program's namespace.
    #[inline]
    #[must_use]
    pub fn meta_mut(&mut self) -> &mut ProgramMetaData {
        &mut self.meta
    }

    /// Get a reference to the program's namespace.
    #[inline]
    #[must_use]
    pub fn namespace(&self) -> &Namespace {
        &self.meta.namespace
    }

    /// Get a mutable reference to the program's namespace.
    #[inline]
    #[must_use]
    pub fn namespace_mut(&mut self) -> &mut Namespace {
        &mut self.meta.namespace
    }

    /// Get a reference to the program's type registry.
    #[inline]
    #[must_use]
    pub fn type_registry(&self) -> &TypeRegistry {
        &self.meta.type_registry
    }

    /// Get a mutable reference to the program's type registry.
    #[inline]
    #[must_use]
    pub fn type_registry_mut(&mut self) -> &mut TypeRegistry {
        &mut self.meta.type_registry
    }

    /// Get a reference to the program's type registry.
    #[inline]
    #[must_use]
    pub fn type_map(&self) -> &TypeMap {
        &self.meta.type_map
    }

    /// Get a mutable reference to the program's type registry.
    #[inline]
    #[must_use]
    pub fn type_map_mut(&mut self) -> &mut TypeMap {
        &mut self.meta.type_map
    }

    /// Intern a string and get its symbol, or just get the symbol if already interned.
    #[inline]
    #[must_use]
    pub fn intern(&self, string: impl AsRef<str>) -> IdentSymbol {
        self.intern.borrow_mut().get_or_intern(string.as_ref())
    }

    /// Intern a string and get its symbol, or just get the symbol if already interned.
    #[inline]
    #[must_use]
    pub fn intern_spanned_ident(&self, i: &crate::ast::SpannedIdent) -> SpannedSymbol {
        let symbol = self.intern(i.str());
        SpannedSymbol {
            symbol,
            span: i.span,
        }
    }

    /// Resolve a symbol into a string, returning `None` if not found.
    #[inline]
    #[must_use]
    pub fn resolve_sym_checked(&self, symbol: impl Into<IdentSymbol>) -> Option<String> {
        self.intern
            .borrow()
            .resolve(symbol.into())
            .map(std::string::ToString::to_string)
    }

    /// Resolve a symbol into a string, or returning `"Unresolved Symbol"` if not found.
    #[inline]
    #[must_use]
    pub fn resolve_sym(&self, symbol: impl Into<IdentSymbol>) -> String {
        self.resolve_sym_checked(symbol)
            .unwrap_or_else(|| "Unresolved Symbol".to_string())
    }
}

impl CompilerContext {
    const CORE_LIB: &str = include_str!("../kit-std/core.purr");
    const INTRINSICS_LIB: &str = include_str!("../kit-std/intrinsics.purr");

    /// Get the full source code, with standard library injected if applicable.
    #[must_use]
    pub fn source(&self) -> String {
        if self.no_stdlib {
            self.source.clone()
        } else {
            let mut source_with_std = String::with_capacity(
                self.source.len() + Self::CORE_LIB.len() + Self::INTRINSICS_LIB.len() + 2,
            );
            source_with_std.extend([
                &self.source,
                "\n",
                Self::CORE_LIB,
                "\n",
                Self::INTRINSICS_LIB,
            ]);
            source_with_std
        }
    }

    /// Get the raw source code, without standard library injected, regardless of settings.
    #[must_use]
    pub fn raw_source(&self) -> &str {
        self.source.as_str()
    }
}

/// The main compiler structure, holds the compiler context and compilation state and logic.
#[derive(Debug)]
pub struct Compiler {
    pub context: CompilerContext,
    pub profile_stages: bool,
}

impl Compiler {
    /// Create a new compiler with the given source code.
    #[inline]
    #[must_use]
    pub fn new(source: impl AsRef<str>) -> Self {
        Self {
            context: CompilerContext::new(source.as_ref().to_string()),
            profile_stages: false,
        }
    }

    /// Create a new compiler with the given source code, with profiling enabled.
    #[inline]
    #[must_use]
    pub fn new_with_profiler(source: impl AsRef<str>) -> Self {
        Self {
            context: CompilerContext::new(source.as_ref().to_string()),
            profile_stages: true,
        }
    }
}

// Set context options, in a 'builder' like pattern.
impl Compiler {
    /// Profile the different compilation stages.
    #[inline]
    #[must_use]
    pub fn profile_stages(mut self, should_profile: bool) -> Self {
        self.profile_stages = should_profile;
        self
    }

    /// Set whether to compile without the standard library.
    #[inline]
    #[must_use]
    pub fn no_stdlib(mut self, no_stdlib: bool) -> Self {
        self.context.no_stdlib = no_stdlib;
        self
    }
}

// Compilation logic methods.
// Supports compiling to different stages.
impl Compiler {
    /// Tokenise the source code into a stream of tokens.
    /// This function does not skip whitespace or comments.
    /// If this behaviour is desired, use `tokenise_stripped` instead.
    /// # Note
    /// This will collect al tokens into a vector before returning an iterator,
    /// so may use more memory for large source files.
    /// # Returns
    /// An iterator over the tokens in the source code.
    pub fn tokenise(&self) -> impl Iterator<Item = Token> {
        crate::lexer::tokenise(&self.context.source())
            .collect::<Vec<_>>()
            .into_iter()
    }

    /// Tokenise the source code into a stream of tokens.
    /// This function skips whitespace and comments.
    /// If this behaviour is not desired, use `tokenise` instead.
    /// # Note
    /// This will collect al tokens into a vector before returning an iterator,
    /// so may use more memory for large source files.
    /// # Returns
    /// An iterator over the tokens in the source code.
    pub fn tokenise_stripped(&self) -> impl Iterator<Item = Token> {
        crate::lexer::tokenise_stripped(&self.context.source())
            .collect::<Vec<_>>()
            .into_iter()
    }

    /// Parse up to the AST stage from the source code provided in the compiler context.
    /// # Errors
    /// This function will return an error if any part of parsing the token stream fails.
    /// The returned error will contain information about where in the source code the error occurred,
    /// and a message about the nature of the error.
    /// # Returns
    /// * `Ok(ASTRoot)` - The parsed AST root node if parsing was successful.
    /// * `Err(KitlangError)` - An error indicating what went wrong during parsing
    pub fn parse_ast(&mut self) -> KitlangResult<crate::ast::ASTRoot> {
        let source_string = self.context.source();
        Ok(if self.profile_stages {
            crate::profiling::print_execution_named("Parse", || {
                crate::parser::parse_from_tokens(
                    crate::lexer::tokenise_stripped(&source_string),
                    &mut self.context,
                )
            })
        } else {
            crate::parser::parse_from_tokens(
                crate::lexer::tokenise_stripped(&source_string),
                &mut self.context,
            )
        }?)
    }

    /// Compile the provided source code up to the HIR stage, without doing any resolution.
    /// # Note
    /// This function does not do any name resolution or type checking, and will produce a HIR
    /// that will contain unresolved names and types.
    /// If this is not desired, use `compile_to_hir` instead.
    /// # Errors
    /// This function will return an error if any part of the AST cannot be lowered properly.
    /// The returned error will contain a diagnostic message indicating the nature and location of any failures,
    /// as well as which stage of lowering it occurred in.
    /// # Returns
    /// * `Ok(HLIR)` - The lowered HIR if successful.
    /// * `Err(KitlangError)` - An error indicating what went wrong during lowering
    pub fn compile_to_hir_no_resolve(&mut self) -> KitlangResult<crate::intermediate::hir::HLIR> {
        let ast = self.parse_ast()?;
        Ok(if self.profile_stages {
            crate::profiling::print_execution_named("Lower to HIR (No resolve)", || {
                crate::intermediate::hir::lower_ast_to_hir(&ast, &mut self.context)
            })
        } else {
            crate::intermediate::hir::lower_ast_to_hir(&ast, &mut self.context)
        }?)
    }

    /// Compile the provided source code up to the HIR stage.
    /// # Note
    /// This function will do all the required resolving and type checking to produce a valid HIR.
    /// # Errors
    /// This function will return an error if any part of the AST cannot be lowered properly.
    /// The returned error will contain a diagnostic message indicating the nature and location of any failures,
    /// as well as which stage of lowering it occurred in.
    /// # Returns
    /// * `Ok(HLIR)` - The lowered HIR if successful.
    /// * `Err(KitlangError)` - An error indicating what went wrong during lowering
    pub fn compile_to_hir(&mut self) -> KitlangResult<crate::intermediate::hir::HLIR> {
        let ast = self.parse_ast()?;
        Ok(if self.profile_stages {
            crate::profiling::print_execution_named("Lower to HIR", || {
                crate::intermediate::hir::parse_ast_to_hir_processed_with(&ast, &mut self.context)
            })
        } else {
            crate::intermediate::hir::parse_ast_to_hir_processed_with(&ast, &mut self.context)
        }?)
    }

    /// Compile the provided source code up to the MIR stage.
    /// This function will parse to AST, then compile to HIR, then lower the HIR to MIR.
    /// # Errors
    /// This function will return an error if any part of the parsing or lowering process fails.
    /// The returned error will contain a diagnostic message indicating the nature and location of any failures, as well
    /// as which stage they originate from.
    ///
    /// These errors can be formatted into user-friendly error messages using the `format_as_error_message`
    /// method on the `KitlangError` on the `Err` variant.
    /// # Returns
    /// * `Ok(MIR)` - The lowered MIR if successful.
    /// * `Err(KitlangError)` - An error indicating what went wrong during any lowering or parsing stage.
    pub fn compile_to_mir(&mut self) -> KitlangResult<crate::intermediate::mir::MIR> {
        let hir = self.compile_to_hir()?;
        Ok(if self.profile_stages {
            crate::profiling::print_execution_named("Lower to MIR", || {
                crate::intermediate::mir::lower_hir_to_mir(&hir, &self.context)
            })
        } else {
            crate::intermediate::mir::lower_hir_to_mir(&hir, &self.context)
        }?)
    }
}

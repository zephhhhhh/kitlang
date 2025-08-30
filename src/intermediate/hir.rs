use crate::{ast::ASTRoot, intermediate::errors::LowerResult};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LocalDefId(u32);

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DefId {
    pub module_id: u32,
    pub def_id: LocalDefId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HLIR {
    // definitions
    //
}

pub fn lower_ast_to_hir(ast: &ASTRoot) -> LowerResult<HLIR> {
    todo!()
}
